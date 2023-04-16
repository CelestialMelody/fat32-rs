#![allow(unused)]

use crate::block_cache::get_block_cache;
use crate::block_device::BlockDevice;
use crate::read_le_u32;
use crate::BlockDeviceError;
use crate::BLOCK_SIZE;
use crate::END_OF_CLUSTER;
use crate::FAT_BUFFER_SIZE;

use lazy_static::*;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Debug;
use spin::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatError {
    ReadError,
    WriteError,
}

#[derive(Clone)]
/// FAT Table for a Cluster Chain
///
/// Like a Dual-Linked List.
//
//  单个文件/目录的簇号链表
//  注意, 整个 Fat 表的簇号从 2 开始, 0 和 1 为保留簇号, 0 表示无效簇号, 1 表示最后一个簇号
pub struct ClusterChain {
    pub(crate) device: Arc<dyn BlockDevice>,
    // FAT表的偏移, 也是 start_cluster 的第一个 sector 的偏移
    // 如果是 FAT1, 可以通过 BIOSParameterBlock::fat1() 方法获取
    pub(crate) fat_offset: usize,
    // 簇号链表的起始簇号
    pub(crate) start_cluster: u32,
    pub(crate) previous_cluster: u32,
    /// if current_cluster == 0, then ClusterChain is invalid (initial).
    /// Therefore, previous_cluster and next_cluster are invalid.
    /// Use next() to get the first cluster.
    //
    //  current_cluster == 0 相当于头节点, 此时 previous_cluster, next_cluster 无效.
    //  需要调用 .next() 方法获取第一个簇号
    pub(crate) current_cluster: u32,
    pub(crate) next_cluster: Option<u32>,
    // 当前块的缓冲区
    // 一个块/扇区的大小为 512 字节, 一个簇一般为 8 个扇区, 一个簇的大小为 4KB
    pub(crate) buffer: [u8; FAT_BUFFER_SIZE],
}

impl ClusterChain {
    pub(crate) fn new(cluster: u32, device: Arc<dyn BlockDevice>, fat_offset: usize) -> Self {
        Self {
            device: Arc::clone(&device),
            fat_offset,
            start_cluster: cluster,
            previous_cluster: 0,
            current_cluster: 0,
            next_cluster: None,
            buffer: [0; FAT_BUFFER_SIZE],
        }
    }

    // 从FAT表中找到空闲的簇
    // TODO: 优化 Fat 维护空簇号
    pub(crate) fn blank_cluster(&mut self) -> u32 {
        // Q: 应该从 0 开始吗? 从 2 开始?
        let mut cluster = 0;
        let mut done = false;

        for block in 0.. {
            self.device
                .read_blocks(&mut self.buffer, self.fat_offset + block * BLOCK_SIZE, 1)
                .unwrap();
            for i in (0..BLOCK_SIZE).step_by(4) {
                if read_le_u32(&self.buffer[i..i + 4]) == 0 {
                    done = true;
                    break;
                } else {
                    cluster += 1;
                }
            }
            if done {
                break;
            }
        }
        cluster
    }

    // 写入 cluster 的 value
    // 在簇号 cluster 中写入下一个簇号
    pub(crate) fn write(&mut self, cluster: u32, value: u32) {
        // Given any valid cluster number N, where in the FAT(s) is the entry for that cluster number?
        //
        // FATOffset = N * 4;
        // ThisFATSecNum = BPB_ResvdSecCnt + (FATOffset / BPB_BytsPerSec);
        // ThisFATEntOffset = REM(FATOffset / BPB_BytsPerSec);
        //
        let offset = (cluster as usize) * 4;
        let block_offset = offset / BLOCK_SIZE;
        let offset_left = offset % BLOCK_SIZE;
        let offset = self.fat_offset + block_offset * BLOCK_SIZE;

        let mut value: [u8; 4] = value.to_be_bytes();
        value.reverse();

        // 读取一个扇区的数据
        self.device
            .read_blocks(&mut self.buffer, offset, 1)
            .unwrap();
        // 在偏移处写入数据
        self.buffer[offset_left..offset_left + 4].copy_from_slice(&value);
        // 写回磁盘
        self.device.write_blocks(&self.buffer, offset, 1).unwrap();
    }

    pub(crate) fn refresh(&mut self, start_cluster: u32) {
        self.current_cluster = 0;
        self.start_cluster = start_cluster;
    }

    /// Change current cluster to previous cluster
    pub(crate) fn previous(&mut self) {
        if self.current_cluster != 0 {
            self.next_cluster = Some(self.current_cluster);
            self.current_cluster = self.previous_cluster;
        }
    }

    pub(crate) fn next_is_none(&self) -> bool {
        self.next_cluster.is_none()
    }

    fn current_cluster_usize(&self) -> usize {
        self.current_cluster as usize
    }
}

impl Iterator for ClusterChain {
    type Item = Self;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_cluster == 0 {
            self.current_cluster = self.start_cluster;
        } else {
            let next_cluster = self.next_cluster;
            if next_cluster.is_some() {
                self.previous_cluster = self.current_cluster;
                self.current_cluster = next_cluster.unwrap();
            } else {
                return None;
            }
        }

        let offset = self.current_cluster_usize() * 4;
        let block_offset = offset / BLOCK_SIZE;
        let offset_left = offset % BLOCK_SIZE;

        self.device
            .read_blocks(
                &mut self.buffer,
                self.fat_offset + block_offset * BLOCK_SIZE,
                1,
            )
            .unwrap();

        let next_cluster = read_le_u32(&self.buffer[offset_left..offset_left + 4]);
        let next_cluster = if next_cluster == END_OF_CLUSTER {
            None
        } else {
            Some(next_cluster)
        };

        self.next_cluster = next_cluster;

        Some(Self {
            next_cluster,
            device: Arc::clone(&self.device),
            ..(*self)
        })
    }
}

pub struct FatManager {
    device: Arc<dyn BlockDevice>,
    recycled_cluster: Vec<u32>,
    buffer: [u8; FAT_BUFFER_SIZE],
    fat_offset: usize,
}

impl FatManager {
    fn new(fat_offset: usize, device: Arc<dyn BlockDevice>) -> Self {
        Self {
            device: Arc::clone(&device),
            recycled_cluster: Vec::new(),
            buffer: [0; FAT_BUFFER_SIZE],
            fat_offset,
        }
    }

    // 从FAT表中找到空闲的簇
    fn find_blank_cluster(&mut self) -> u32 {
        // Q: 应该从 0 开始吗? 从 2 开始?
        // A: (数据区) 从 0 开始; (磁盘上) 从 first_data_sector 开始
        let mut cluster = 0;
        let mut done = false;

        for block in 0.. {
            self.device
                .read_blocks(&mut self.buffer, self.fat_offset + block * BLOCK_SIZE, 1)
                .unwrap();
            for i in (0..BLOCK_SIZE).step_by(4) {
                if read_le_u32(&self.buffer[i..i + 4]) == 0 {
                    done = true;
                    break;
                } else {
                    cluster += 1;
                }
            }
            if done {
                break;
            }
        }
        cluster
    }

    fn blank_cluster(&mut self) -> u32 {
        if let Some(cluster) = self.recycled_cluster.pop() {
            cluster
        } else {
            self.find_blank_cluster()
        }
    }

    fn recycle(&mut self, cluster: u32) {
        self.recycled_cluster.push(cluster);
    }
}
