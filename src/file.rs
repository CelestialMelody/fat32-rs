use crate::block_cache::get_block_cache;
use crate::block_cache::Cache;
use crate::block_device::BlockDevice;
use crate::bpb::BIOSParameterBlock;
use crate::dir::DirIter;
use crate::entry::Entry;
use crate::fat::ClusterChain;
use crate::END_OF_CLUSTER;

use crate::BLOCK_SIZE;
use crate::FILE_BUFFER_SIZE;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cmp;
use core::fmt::Debug;

use crate::get_needed_sector;

/// Define FileError
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FileError {
    BufTooSmall,
    WriteError,
    ReadOutOfBound,
}

/// Define WriteType
pub enum WriteType {
    OverWritten,
    Append,
}

#[derive(Clone)]
pub struct File<'a> {
    pub(crate) device: Arc<dyn BlockDevice>,
    pub(crate) bpb: &'a BIOSParameterBlock,
    pub(crate) dir_cluster: u32,
    pub(crate) sde: Entry,
    // pub(crate) lde: Vec<Entry>,
    pub(crate) fat: ClusterChain,
}

/// To Read File Per Sector By Iterator
pub struct ReadIter<'a> {
    device: Arc<dyn BlockDevice>,
    // TODO: Use blcok cache manager to manage cache/buffer
    buffer: [u8; FILE_BUFFER_SIZE],
    bpb: &'a BIOSParameterBlock,
    fat: ClusterChain,
    left_length: usize,
    read_count: usize,
    need_count: usize,
}

impl<'a> File<'a> {
    /// 将文件内容从 offset 字节开始的部分读到内存中的缓冲区 buf 中, 并返回实际读到的字节数
    pub fn read_at(&self, buf: &mut [u8], offset: usize) -> Result<usize, FileError> {
        let spc = self.bpb.sector_per_cluster_usize();
        let cluster_size = spc * BLOCK_SIZE;

        // 1. 确定范围 [start, end) 中间的那些块需要被读取
        let mut file_start_pointer = offset;
        // min(): 如果文件剩下的内容还足够多, 那么缓冲区会被填满; 否则文件剩下的全部内容都会被读到缓冲区中
        let file_end_bound = (file_start_pointer + buf.len()).min(self.sde.file_size().unwrap());
        if file_start_pointer >= file_end_bound {
            // 如果 start >= end, 则说明 offset 已经超过了文件的大小, 无法读取
            return Err(FileError::ReadOutOfBound);
        }

        // 2. 确定起始簇号, 以及簇内块号以及块号
        // block_id_in_file: 目前是文件内部第多少个数据块
        let block_id_in_file = file_start_pointer / BLOCK_SIZE as usize;
        let mut fat = self.fat.clone();
        // pre_cluster_count: offset 之前的簇号
        let pre_cluster_count = block_id_in_file / spc;
        // start_block_byte_in_file: 以 byte 为单位, 文件内部块的起始位置
        let mut start_block_byte_in_file = 0;
        for _ in 0..pre_cluster_count {
            start_block_byte_in_file += cluster_size;
            fat = fat.next().unwrap();
        }

        let mut read_size = 0;
        // 3. 读取数据
        while file_start_pointer < file_end_bound {
            // cluster offset byte in disk
            let cluster_offset_in_disk = self.bpb.offset(fat.current_cluster as u32);
            // block id range of the current cluster
            let start_block_id_in_disk = cluster_offset_in_disk / BLOCK_SIZE;
            let end_block_id_in_disk = cluster_offset_in_disk / BLOCK_SIZE + spc;

            for (block_id_in_cluster, block_id) in
                (start_block_id_in_disk..end_block_id_in_disk).enumerate()
            {
                // file start pointer in block byte range
                if file_start_pointer >= start_block_byte_in_file
                    && file_start_pointer < start_block_byte_in_file + BLOCK_SIZE
                    && file_start_pointer < file_end_bound
                {
                    let option = get_block_cache(block_id, Arc::clone(&self.device));

                    if option.is_some() {
                        let cache = option.unwrap();
                        // distance of file start pointer from the start of the block
                        let offset_in_block = file_start_pointer - start_block_byte_in_file;
                        let len =
                            (BLOCK_SIZE - offset_in_block).min(file_end_bound - file_start_pointer);
                        cache.read().read(0, |cache: &[u8; BLOCK_SIZE]| {
                            buf[read_size..read_size + len]
                                .copy_from_slice(&cache[offset_in_block..offset_in_block + len]);
                        });
                        file_start_pointer += len;
                        read_size += len;
                    } else {
                        // cache 无法获取: lru_cache 暂时没法释放一个 cache, 此时直接从磁盘读取
                        let offset_in_block = file_start_pointer - start_block_byte_in_file;
                        let len = (BLOCK_SIZE * (spc - block_id_in_cluster) - offset_in_block)
                            .min(file_end_bound - file_start_pointer);

                        // TODO perf: add cluster cache for BlockCache
                        let mut cluster_buffer = Vec::<u8>::with_capacity(cluster_size);
                        self.device
                            .read_blocks(
                                cluster_buffer.as_mut_slice(),
                                start_block_id_in_disk * BLOCK_SIZE,
                                spc,
                            )
                            .unwrap();
                        let offset_in_cluster = block_id_in_cluster * BLOCK_SIZE + offset_in_block;
                        buf[read_size..read_size + len].copy_from_slice(
                            &cluster_buffer[offset_in_cluster..offset_in_cluster + len],
                        );
                        file_start_pointer += len;
                        read_size += len;
                        break;
                    }
                }

                if file_start_pointer >= file_end_bound {
                    // return Ok(read_size);
                    break;
                }

                start_block_byte_in_file += BLOCK_SIZE;
            }

            fat = fat.next().unwrap();
        }

        Ok(read_size)
    }

    /// Read File To Buffer, Return File Length
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, FileError> {
        let length = self.sde.file_size().unwrap();
        let spc = self.bpb.sector_per_cluster_usize();
        let cluster_size = spc * BLOCK_SIZE;
        let mut block_cnt = spc;

        if buf.len() < length {
            return Err(FileError::BufTooSmall);
        }

        let mut index = 0;
        self.fat
            .clone()
            .map(|f| {
                let offset = self.bpb.offset(f.current_cluster);

                let end = if (length - index) < cluster_size {
                    // 读取长度在一个簇之内
                    let bytes_left = length % cluster_size;
                    block_cnt = get_needed_sector(bytes_left);
                    index + bytes_left
                } else {
                    // 读取长度超过一个簇的大小
                    index + cluster_size
                };
                self.device
                    .read_blocks(&mut buf[index..end], offset, block_cnt)
                    .unwrap();
                index += cluster_size;
            })
            .last();

        Ok(length)
    }

    /// Write Data To File, Using Append OR OverWritten
    pub fn write(&mut self, buf: &[u8], write_type: WriteType) -> Result<(), FileError> {
        let cluster_id = match write_type {
            WriteType::OverWritten => self.num_cluster(buf.len()),
            WriteType::Append => self.num_cluster(buf.len() + self.sde.file_size().unwrap()),
        };

        match write_type {
            WriteType::OverWritten => {
                // 将之前的链接情况清除
                self.fat
                    .clone()
                    .map(|mut f| f.write(f.current_cluster, 0))
                    .last(); // 迭代器是懒惰求值的, 只有在它们被消耗时才会执行操作, 故这里使用 last() 来触发迭代器的执行

                // 重新设置链接情况
                self.write_blank_fat(cluster_id);
                self._write(buf, &self.fat);
            }
            WriteType::Append => {
                let mut fat = self.fat.clone();
                let exist_fat = fat.clone().count();
                // 修改 fat: 迭代 fat 使 fat.current_cluster 为簇链的最后一个簇, 即找到最后一个簇的位置
                fat.find(|_| false);

                // 填充当前 sector 空余的地方
                let (need_new_cluster, index) = self.fill_left_sector(buf, fat.current_cluster);
                if need_new_cluster {
                    // buf: 还未写的数据
                    let buf = &buf[index..];
                    let bl = self.fat.blank_cluster();

                    fat.write(fat.current_cluster, bl);
                    self.write_blank_fat(cluster_id - exist_fat);
                    fat.refresh(bl);

                    self._write(buf, &fat);
                }
            }
        }

        match write_type {
            WriteType::OverWritten => self.update_file_size(buf.len()),
            WriteType::Append => self.update_file_size(buf.len() + self.sde.file_size().unwrap()),
        };

        Ok(())
    }

    /// Read Per Sector, Return ReadIter
    pub fn read_per_sector(&self) -> ReadIter {
        let left_length = self.sde.file_size().unwrap();
        ReadIter {
            device: Arc::clone(&self.device),
            buffer: [0; FILE_BUFFER_SIZE],
            bpb: self.bpb,
            fat: self.fat.clone(),
            left_length,
            read_count: 0,
            need_count: get_needed_sector(left_length),
        }
    }

    #[inline(always)]
    /// Get Clusters The File Has
    fn num_cluster(&self, length: usize) -> usize {
        let spc = self.bpb.sector_per_cluster_usize();
        let cluster_size = spc * BLOCK_SIZE;
        if length % cluster_size != 0 {
            length / cluster_size + 1
        } else {
            length / cluster_size
        }
    }

    /// Write Buffer from one to another one
    ///
    /// - sec_idx: sector id in one cluster
    fn buf_write(&self, from: &[u8], sec_idx: usize, to: &mut [u8]) {
        let index = sec_idx * BLOCK_SIZE;
        let index_end = index + BLOCK_SIZE;
        if from.len() < index_end {
            to.copy_from_slice(&[0; BLOCK_SIZE]);
            to[0..from.len() - index].copy_from_slice(&from[index..])
        } else {
            to.copy_from_slice(&from[index..index_end])
        }
    }

    /// Fill Left Sectors in Given Cluster
    //
    //  使用 buf 填充簇中剩余的扇区
    fn fill_left_sector(&self, buf: &[u8], cluster: u32) -> (bool, usize) {
        let spc = self.bpb.sector_per_cluster_usize();
        // Q: 如果length 超过一个簇的大小怎么办?
        let length = self.sde.file_size().unwrap();
        // 获取已经使用的扇区数
        let get_used_sector = |len: usize| {
            if len % (spc * BLOCK_SIZE) == 0 && length != 0 {
                // 刚好占满一个簇
                spc
            } else {
                (len % (spc * BLOCK_SIZE)) / BLOCK_SIZE
            }
        };
        let left_start = length % BLOCK_SIZE;
        let blank_size = BLOCK_SIZE - left_start;

        // 已经填充的字节数
        let mut already_fill = 0;
        // buf 中是否还有剩余
        let mut buf_has_left = true;
        let mut index = 0;
        let mut used_sector = get_used_sector(length);
        let mut data = [0; BLOCK_SIZE];
        let mut offset = self.bpb.offset(cluster) + used_sector * BLOCK_SIZE;

        // 先尝试填充一个扇区/块
        if left_start != 0 {
            // 不是刚好占满一个扇区/块
            self.device.read_blocks(&mut data, offset, 1).unwrap();
            if buf.len() <= blank_size {
                // buf 长度小于等于剩余空间
                data[left_start..left_start + buf.len()].copy_from_slice(&buf[0..]);
                buf_has_left = false;
            } else {
                // buf 长度大于剩余空间, buf 中剩余的数据需要写入下一个簇
                data[left_start..].copy_from_slice(&buf[0..blank_size]);
                already_fill = blank_size;
                index = already_fill;
                used_sector = get_used_sector(length + already_fill);
                buf_has_left = true;
            };
            self.device.write_blocks(&data, offset, 1).unwrap();
            offset = self.bpb.offset(cluster) + BLOCK_SIZE;
        }

        // 填充剩余的扇区/块
        if buf_has_left {
            let buf_needed_sector = get_needed_sector(buf.len() - already_fill);
            let the_cluster_left_sector = spc - used_sector;
            // 如果 buf_needed_sector 大于剩余的扇区数, 则只写入剩余的扇区数
            // 说明跨簇了
            let num_sector = cmp::min(the_cluster_left_sector, buf_needed_sector);
            for s in 0..num_sector {
                self.buf_write(&buf[index..], s, &mut data);
                self.device
                    .write_blocks(&data, offset + s * BLOCK_SIZE, 1)
                    .unwrap();
                index += BLOCK_SIZE;
            }

            // 如果 buf_needed_sector 大于剩余的扇区数, 则只写入剩余的扇区数
            // 说明跨簇了
            if buf_needed_sector > the_cluster_left_sector {
                return (true, index);
            }
        }

        (false, 0)
    }

    /// Update File Length
    fn update_file_size(&mut self, length: usize) {
        let fat = ClusterChain::new(self.dir_cluster, Arc::clone(&self.device), self.bpb.fat1());
        let mut iter = DirIter::new(Arc::clone(&self.device), fat, self.bpb);
        iter.find(|d| {
            !d.is_deleted() && !d.is_lfn() && d.first_cluster() == self.sde.first_cluster()
        })
        .unwrap();

        self.sde.set_file_size(length);
        iter.previous();
        iter.update_item(&self.sde.sde_to_bytes_array().unwrap());
        iter.update();
    }

    /// Write Blank FAT
    //
    //  在簇链中添加一个簇
    fn write_blank_fat(&mut self, num_cluster: usize) {
        for n in 0..num_cluster {
            // 类似于创建一个空节点
            let bl1 = self.fat.blank_cluster();
            self.fat.write(bl1, END_OF_CLUSTER);

            let bl2 = self.fat.blank_cluster();
            if n != num_cluster - 1 {
                // 类似于 bl1.val = bl2
                self.fat.write(bl1, bl2);
            }
        }
    }

    /// Basic Write Function
    fn _write(&self, buf: &[u8], fat: &ClusterChain) {
        let spc = self.bpb.sector_per_cluster_usize();
        let mut buf_write = [0; BLOCK_SIZE];
        // sec_cnt: sector counts need to write
        let mut sec_cnt_to_write = get_needed_sector(buf.len());

        let func = |start: usize, sec_cnt: usize| -> &[u8] {
            &buf[start * BLOCK_SIZE..(start + sec_cnt) * BLOCK_SIZE]
        };

        // 已经写入的扇区数
        let mut writen_sec = 0;
        fat.clone()
            .map(|f| {
                // blk_cnt: block count need to write
                let blk_cnt_to_write = if sec_cnt_to_write / spc > 0 {
                    // 如果需要写入的扇区数超过了一个簇的扇区数
                    sec_cnt_to_write -= spc;
                    spc
                } else {
                    sec_cnt_to_write
                };

                let offset = self.bpb.offset(f.current_cluster);
                if blk_cnt_to_write == spc {
                    // 如果需要写入的扇区数超过了一个簇的扇区数
                    if (writen_sec + blk_cnt_to_write) * BLOCK_SIZE > buf.len() {
                        // buf 中的数据可以写完
                        self.buf_write(&buf, writen_sec, &mut buf_write);
                        self.device.write_blocks(&buf_write, offset, 1).unwrap();
                    } else {
                        // buf 中的数据还没写完
                        self.device
                            .write_blocks(
                                func(writen_sec, blk_cnt_to_write),
                                offset,
                                blk_cnt_to_write,
                            )
                            .unwrap();
                    }
                    writen_sec += blk_cnt_to_write;
                } else {
                    // 如果需要写入的扇区数没有超过一个簇的扇区数
                    self.device
                        .write_blocks(
                            func(writen_sec, blk_cnt_to_write - 1),
                            offset,
                            blk_cnt_to_write - 1,
                        )
                        .unwrap();

                    writen_sec += blk_cnt_to_write - 1;

                    self.buf_write(&buf, writen_sec, &mut buf_write);

                    self.device
                        .write_blocks(&buf_write, offset + (blk_cnt_to_write - 1) * BLOCK_SIZE, 1)
                        .unwrap();
                }
            })
            .last();
    }
}

impl<'a> Iterator for ReadIter<'a> {
    type Item = ([u8; BLOCK_SIZE], usize);

    fn next(&mut self) -> Option<Self::Item> {
        let spc = self.bpb.sector_per_cluster_usize();
        if self.read_count == self.need_count {
            return None;
        }
        if self.read_count % spc == 0 {
            self.fat.next();
        }

        let offset =
            self.bpb.offset(self.fat.current_cluster) + (self.read_count % spc) * BLOCK_SIZE;
        self.device
            .read_blocks(&mut self.buffer, offset, 1)
            .unwrap();
        self.read_count += 1;

        Some(if self.read_count == self.need_count {
            (self.buffer, self.left_length)
        } else {
            self.left_length -= BLOCK_SIZE;
            (self.buffer, BLOCK_SIZE)
        })
    }
}
