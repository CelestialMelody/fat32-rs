//! FAT表管理
//!
//!
//!
//! 注意:
//! - 从数据区开始, 对 cluster 进行编号, 编号从 2 开始;
//!   计算在磁盘中的偏移 offset = BLOCK_SIZE * (bpb.first_data_sector + (cluster - 2) * bpb.sector_per_cluster)
//! - block_id 在存储介质从 0 开始 从 0 编号;
//!   计算在磁盘中的偏移 offset = BLOCK_SIZE * block_id
//! - 其他命名尽量容易理解 如 block_id_in_cluster 为簇内块号

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::{
    assert,
    clone::Clone,
    fmt::Debug,
    iter::Iterator,
    option::Option,
    option::Option::{None, Some},
};

use super::{
    cache::{get_block_cache, Cache},
    device::BlockDevice,
    read_le_u32, BLOCK_SIZE, CLUSTER_MASK, END_OF_CLUSTER, NEW_VIR_FILE_CLUSTER,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterChainErr {
    ReadError,
    WriteError,
    NonePreviousCluster,
    NoneNextCluster,
}

#[derive(Clone)]
/// Cluster Chain in FAT Table.
///
/// Like a Dual-Linked List.
//
//  单个文件/目录的簇号链表
//  注意, 整个 Fat 表的簇号从 2 开始, 0 和 1 为保留簇号;
//  根据 cluster_id 求出偏移时, 数据区以 cluster_size 为单位从 0 开始计算, cluster_id - 2
pub struct ClusterChain {
    pub(crate) device: Arc<dyn BlockDevice>,
    // FAT表的偏移, 也是 start_cluster 的第一个 sector 的偏移
    // 目前仅指 FAT1, 可以通过 BIOSParameterBlock::fat1() 方法获取
    // TODO 支持 FAT2
    pub(crate) fat1_offset: usize, // read_only
    // 簇号链表的起始簇号 (短目录项可以提供)
    pub(crate) start_cluster: u32, // 创建一次不再改变
    pub(crate) previous_cluster: Option<u32>,
    /// if current_cluster == 0, then ClusterChain is invalid (initial).
    /// Therefore, previous_cluster and next_cluster are invalid.
    /// Use next() to get the first cluster.
    //
    //  注意:
    //  1. current_cluster == 0 相当于头节点, 此时 previous_cluster, next_cluster 无效.
    //  2. 需要调用 .next() 方法获取第一个簇号
    //  3. 当使用迭代器时, 会自动从 start_cluster 开始
    pub(crate) current_cluster: u32,
    pub(crate) next_cluster: Option<u32>,
}

impl Debug for ClusterChain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClusterChain")
            .field("start_cluster", &self.start_cluster)
            .field("current_cluster", &self.current_cluster)
            .field("previous_cluster", &self.previous_cluster)
            .field("next_cluster", &self.next_cluster)
            .finish()
    }
}

#[allow(unused)]
impl ClusterChain {
    pub(crate) fn new(cluster: u32, device: Arc<dyn BlockDevice>, fat_offset: usize) -> Self {
        Self {
            device: Arc::clone(&device),
            fat1_offset: fat_offset,
            start_cluster: cluster,
            previous_cluster: None,
            current_cluster: 0,
            next_cluster: None,
        }
    }

    pub(crate) fn refresh(&mut self, start_cluster: u32) {
        self.current_cluster = NEW_VIR_FILE_CLUSTER;
        self.start_cluster = start_cluster;
    }

    pub(crate) fn next_is_none(&self) -> bool {
        self.next_cluster.is_none()
    }

    pub(crate) fn previous_is_none(&self) -> bool {
        self.previous_cluster.is_none()
    }
}

impl Iterator for ClusterChain {
    type Item = Self;

    // 最后一个 fat 簇:
    // - current_cluster = EOC(仍然有数据)
    // - next_cluster = None
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_cluster == NEW_VIR_FILE_CLUSTER {
            // 对于 write_append 结合 refresh 有其他作用:
            // write_append 需要使用最后一个 fat 簇, 最后一个 fat 簇的 next_cluster 为 None.
            // 由于调用 refresh 时, current_cluster 为 0, start_cluster 为新建簇, 所以
            // 调用 next 时跳转到新建簇 (current_cluster == start_cluster), next_cluster
            // 则到磁盘或缓存中的 fat 表中读取.
            // 注意 next() 结束后 previous_cluster, start_cluster 被修改为正确的值.
            self.current_cluster = self.start_cluster;
            self.previous_cluster = None;
        } else {
            let next_cluster = self.next_cluster;
            if next_cluster.is_some() {
                self.previous_cluster = Some(self.current_cluster);
                self.current_cluster = next_cluster.unwrap();
            } else {
                return None;
            }
        }

        let offset = self.current_cluster as usize * 4;
        let block_offset = offset / BLOCK_SIZE;
        let offset_left = offset % BLOCK_SIZE;

        let block_id = self.fat1_offset / BLOCK_SIZE + block_offset;
        let mut buffer = [0u8; BLOCK_SIZE];

        get_block_cache(block_id, Arc::clone(&self.device))
            .read()
            .read(0, |buf: &[u8; BLOCK_SIZE]| {
                buffer.copy_from_slice(buf);
            });

        let next_cluster = read_le_u32(&buffer[offset_left..offset_left + 4]);
        let next_cluster = if next_cluster >= END_OF_CLUSTER {
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

//  整个 Fat 表的簇号从 2 开始, 0 和 1 为保留簇号, 0 表示无效簇号, 1 表示最后一个簇号,
//  在数据区以 cluster_size 为单位从 0 开始编号, 故根据 cluster_id 求出偏移时 cluster_id - 2
//  通过 bpb.first_data_sector() 可得到从磁盘0号扇区开始编号的数据区的第一个扇区号(距离磁盘0号扇区的扇区数)
//
//  TODO 目前只做了FAT1 (FAT2相当于对FAT1的备份, 可以在每次打开文件系统时复制FAT1到FAT2)
//  TODO 将整个 FAT 放入内存中进行管理(查空块, 写簇, 簇链随机分配), 以提高性能
pub struct FATManager {
    device: Arc<dyn BlockDevice>,
    recycled_cluster: VecDeque<u32>,
    fat1_offset: usize,
}

impl FATManager {
    pub fn open(fat_offset: usize, device: Arc<dyn BlockDevice>) -> Self {
        Self {
            device: Arc::clone(&device),
            recycled_cluster: VecDeque::new(),
            fat1_offset: fat_offset,
        }
    }

    pub fn new(fat_offset: usize, device: Arc<dyn BlockDevice>) -> Self {
        let fat = Self {
            device: Arc::clone(&device),
            recycled_cluster: VecDeque::new(),
            fat1_offset: fat_offset,
        };

        // Initialize FAT1 Table
        // 由于簇号从 2 开始, 现在将簇号 0, 1 的内容填充方便找到正确的簇(防止误操作)
        let block_id = fat.fat1_offset / BLOCK_SIZE;

        assert!(fat.fat1_offset % BLOCK_SIZE == 0);
        get_block_cache(block_id, Arc::clone(&device))
            .write()
            .modify(0, |buf: &mut [u32; 2]| {
                buf[0] = END_OF_CLUSTER;
                buf[1] = END_OF_CLUSTER;
            });

        fat
    }

    // 给出 FAT 表的下标(clsuter_id_in_fat数据区簇号), 返回这个下标 (fat表的) 相对于磁盘的扇区数 (block_id) 与扇区内偏移
    /// index: cluster_id_in_fat 从 2 开始有效
    pub fn cluster_id_pos(&self, index: u32) -> (usize, usize) {
        // Given any valid cluster number N, where in the FAT(s) is the entry for that cluster number?
        //
        // FATOffset = N * 4;
        // ThisFATSecNum = BPB_ResvdSecCnt + (FATOffset / BPB_BytsPerSec);
        // ThisFATEntOffset = REM(FATOffset / BPB_BytsPerSec);
        //
        // 不需要 断言 index >= 2, 理由:
        // 1. fs::open 时对 fat_manager 预处理了
        // 2. 新建文件的 cluster_id = 0 会 panic
        let offset = index as usize * 4 + self.fat1_offset;
        let block_id = offset / BLOCK_SIZE;
        let offset_in_block = offset % BLOCK_SIZE;
        (block_id, offset_in_block)
    }

    // 从FAT表中找到空闲的簇
    // 从 start_from 开始找, 提高查找效率
    fn find_blank_cluster(&self, start_from: u32) -> u32 {
        // 加 1 过滤已经分配的簇号 (该簇号还未初始值为EOC, 防止找到同样的簇号)
        let mut cluster = start_from + 1;
        let mut done = false;
        let mut buffer = [0u8; BLOCK_SIZE];

        loop {
            let (block_id, offset) = self.cluster_id_pos(cluster);
            get_block_cache(block_id, Arc::clone(&self.device))
                .read()
                .read(0, |buf: &[u8; BLOCK_SIZE]| {
                    buffer.copy_from_slice(buf);
                });
            for i in (offset..BLOCK_SIZE).step_by(4) {
                if read_le_u32(&buffer[i..i + 4]) == 0 {
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

        cluster & CLUSTER_MASK
    }

    pub fn blank_cluster(&mut self, start_from: u32) -> u32 {
        if let Some(cluster) = self.recycled_cluster.pop_front() {
            cluster & CLUSTER_MASK
        } else {
            self.find_blank_cluster(start_from)
        }
    }

    pub fn recycle(&mut self, cluster: u32) {
        self.recycled_cluster.push_back(cluster);
    }

    // Query the next cluster of the specific cluster
    //
    // 最后一个簇的值, next_cluster 可能等于 EOC
    pub fn get_next_cluster(&self, cluster: u32) -> Option<u32> {
        let (block_id, offset_in_block) = self.cluster_id_pos(cluster);

        let next_cluster: u32 = get_block_cache(block_id, Arc::clone(&self.device))
            .read()
            .read(offset_in_block, |&next_cluster: &u32| next_cluster);

        assert!(next_cluster >= 2);
        if next_cluster >= END_OF_CLUSTER {
            None
        } else {
            Some(next_cluster)
        }
    }

    // Set the next cluster of the specific cluster
    //
    // 在磁盘的FAT表中的簇号 cluster(offset) 处写入 cluster 的 value(下一个簇号)
    pub fn set_next_cluster(&self, cluster: u32, next_cluster: u32) {
        let (block_id, offset_in_block) = self.cluster_id_pos(cluster);
        get_block_cache(block_id, Arc::clone(&self.device))
            .write()
            .modify(offset_in_block, |value: &mut u32| {
                *value = next_cluster;
            });
    }

    // Get the ith cluster of a cluster chain
    pub fn get_cluster_at(&self, start_cluster: u32, index: u32) -> Option<u32> {
        let mut cluster = start_cluster;
        for _ in 0..index {
            let option = self.get_next_cluster(cluster);
            if let Some(c) = option {
                cluster = c
            } else {
                return None;
            }
        }
        Some(cluster & CLUSTER_MASK)
    }

    // Get the last cluster of a cluster chain
    pub fn cluster_chain_tail(&self, start_cluster: u32) -> u32 {
        let mut curr_cluster = start_cluster;
        // start cluster 是 fat 表中的 index, 从 2 开始有效
        assert!(curr_cluster >= 2);
        loop {
            let option = self.get_next_cluster(curr_cluster);
            if let Some(cluster) = option {
                curr_cluster = cluster
            } else {
                return curr_cluster & CLUSTER_MASK;
            }
        }
    }

    // Get all clusters of a cluster chain starting from the specified cluster
    pub fn get_all_cluster_id(&self, start_cluster: u32) -> Vec<u32> {
        let mut curr_cluster = start_cluster;
        let mut vec: Vec<u32> = Vec::new();
        loop {
            vec.push(curr_cluster & CLUSTER_MASK);
            let option = self.get_next_cluster(curr_cluster);
            if let Some(next_cluster) = option {
                curr_cluster = next_cluster;
            } else {
                return vec;
            }
        }
    }

    pub fn cluster_chain_len(&self, start_cluster: u32) -> u32 {
        let mut curr_cluster = start_cluster;
        let mut len = 0;
        loop {
            len += 1;
            let option = self.get_next_cluster(curr_cluster);
            if let Some(next_cluster) = option {
                curr_cluster = next_cluster;
            } else {
                return len;
            }
        }
    }
}
