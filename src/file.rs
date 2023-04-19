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
    pub(crate) fat: ClusterChain, // read only
}

/// To Read File Per Sector By Iterator
pub struct ReadIter<'a> {
    device: Arc<dyn BlockDevice>,
    buffer: [u8; FILE_BUFFER_SIZE],
    bpb: &'a BIOSParameterBlock,
    fat: ClusterChain,
    left_length: usize,
    read_count: usize,
    need_count: usize,
}

impl<'a> File<'a> {
    /// Read File at Offset
    ///
    /// Based on geiven file offset and read file content into buf
    ///
    /// Return Read Length
    pub fn read_at_(&self, buf: &mut [u8], offset: usize) -> Result<usize, FileError> {
        let spc = self.bpb.sector_per_cluster_usize();

        // 1. 确定范围 [start, end) 中间的那些块需要被读取
        // min(): 如果文件剩下的内容还足够多, 那么缓冲区会被填满; 否则文件剩下的全部内容都会被读到缓冲区中
        let end = (offset + buf.len()).min(self.sde.file_size().unwrap());
        if offset >= end {
            // 如果 start >= end, 则说明 offset 已经超过了文件的大小, 无法读取
            return Err(FileError::ReadOutOfBound);
        }
        let mut need_to_read_len = end - offset;
        // let cluster_cnt_to_read = self.num_cluster(need_to_read_len);

        // 2. 确定起始簇号, 以及簇内块号以及块号
        // block_id_in_file: 目前是文件内部第多少个数据块
        let block_id_in_file = offset / BLOCK_SIZE as usize;
        let mut fat = self.fat.clone();
        // pre_cluster_count: offset 之前的簇号
        let pre_cluster_count = block_id_in_file / spc;
        // 遍历到 offset 所在的簇
        for _ in 0..pre_cluster_count {
            fat = fat.next().unwrap();
        }

        // 3. 读取数据
        // 读取 offset 所在簇的余下数据
        let (need_read_next_cluster, mut already_read) =
            self.read_rest_sector_in_cluster(buf, offset, need_to_read_len, fat.current_cluster);
        need_to_read_len -= already_read;

        if need_read_next_cluster {
            let buf_left = &mut buf[already_read..];
            fat = fat.next().unwrap();

            already_read += self.basic_read(
                &mut buf_left[already_read..already_read + need_to_read_len],
                &fat,
            );
        }

        assert!(already_read == end - offset);
        Ok(already_read)
    }

    /// Read File
    ///
    /// Based on the given file offset, find the corresponding cluster and read the block/sector after the offset.
    pub fn read_rest_sector_in_cluster(
        &self,
        buf: &mut [u8],
        offset: usize,
        length_to_read: usize,
        cluster_id: u32,
    ) -> (bool, usize) {
        let spc = self.bpb.sector_per_cluster_usize();
        // Q: 如果length 超过一个簇的大小怎么办? -> retern false, 上层创建簇链再继续读
        // 获取 len 在簇中所在的块号
        let get_pre_sector = |len: usize| {
            if len % (spc * BLOCK_SIZE) == 0 && length_to_read != 0 {
                // 刚好占满一个簇
                spc
            } else {
                (len % (spc * BLOCK_SIZE)) / BLOCK_SIZE
            }
        };
        // 文件原本的大小是否刚好占满一个扇区/块
        let left_start = offset % BLOCK_SIZE;
        let left_size_in_block = BLOCK_SIZE - left_start;

        // 已经读取的字节数
        let mut already_read = 0;
        // buf 中是否还有剩余
        let mut buf_has_left = true;
        // TODO 合并already_read 和 index
        let mut index = 0;
        let mut block_id_in_cluster = get_pre_sector(length_to_read);
        let mut data = [0; BLOCK_SIZE];
        let mut offset = self.bpb.offset(cluster_id) + block_id_in_cluster * BLOCK_SIZE;
        assert!(offset % BLOCK_SIZE == 0);
        let block_id = offset / BLOCK_SIZE;

        // TODO
        // check
        // 先尝试读取一个扇区/块
        if left_start != 0 {
            // 文件原本的大小不是刚好占满一个扇区/块
            let option = get_block_cache(block_id, Arc::clone(&self.device));
            if let Some(cache) = option {
                cache.read().read(0, |buffer: &[u8; 512]| {
                    data.copy_from_slice(buffer);
                });
            } else {
                self.device.read_blocks(&mut data, offset, 1).unwrap();
            }

            // self.device.read_blocks(&mut data, offset, 1).unwrap();
            if length_to_read <= left_size_in_block {
                // buf 长度小于等于剩余空间
                buf[0..length_to_read]
                    .copy_from_slice(&data[left_start..left_start + length_to_read]);
                buf_has_left = false;
            } else {
                // buf 长度大于剩余空间, buf 中剩余的数据需要写入下一个簇
                buf[0..left_size_in_block].copy_from_slice(&data[left_start..]);
                already_read = left_size_in_block;
                index = already_read;
                block_id_in_cluster = get_pre_sector(length_to_read + already_read);
                buf_has_left = true;
            };

            offset = self.bpb.offset(cluster_id) + BLOCK_SIZE;
        }

        // 读取剩余的扇区/块
        if buf_has_left {
            let buf_needed_sector = get_needed_sector(length_to_read - already_read);
            let cluster_left_sector = spc - block_id_in_cluster;
            // 如果 buf_needed_sector 大于剩余的扇区数, 则只读取剩余的扇区数, 说明跨簇了
            let num_sector = cmp::min(cluster_left_sector, buf_needed_sector);
            for s in 0..num_sector {
                // 读取到 data 中
                let block_id = offset / BLOCK_SIZE + s;
                assert!(offset % BLOCK_SIZE == 0);
                let option = get_block_cache(block_id, Arc::clone(&self.device));
                if let Some(cache) = option {
                    cache.read().read(0, |buffer: &[u8; 512]| {
                        data.copy_from_slice(buffer);
                    });
                } else {
                    self.device
                        .read_blocks(&mut data, offset + s * BLOCK_SIZE, 1)
                        .unwrap();
                }
                // 写入到 buf 中
                self.buf_read(&data, s, &mut buf[index..]);
                index += BLOCK_SIZE;
                already_read += BLOCK_SIZE;
            }

            // 如果 buf_needed_sector 大于剩余的扇区数, 说明跨簇了
            if buf_needed_sector > cluster_left_sector {
                return (true, already_read);
            }
        }

        (false, already_read)
    }

    // TODO
    // check
    fn basic_read(&self, buf: &mut [u8], fat: &ClusterChain) -> usize {
        let spc = self.bpb.sector_per_cluster_usize();
        let mut buf_read = [0u8; BLOCK_SIZE];
        let mut sec_cnt_to_read = get_needed_sector(buf.len());

        let mut blocks_have_read = 0;
        fat.clone()
            .map(|f| {
                // blk_cnt: block count need to write
                let block_cnt_to_read = if sec_cnt_to_read / spc > 0 {
                    // 如果需要写入的扇区数超过了一个簇的扇区数
                    sec_cnt_to_read -= spc;
                    spc
                } else {
                    sec_cnt_to_read
                };

                let offset = self.bpb.offset(f.current_cluster);
                assert!(offset % BLOCK_SIZE == 0);
                let start_block_id = offset / BLOCK_SIZE;

                // 如果需要读的扇区数超过了一个簇的扇区数
                if block_cnt_to_read == spc {
                    // 可以填充 (读) 完 buf
                    if (blocks_have_read + block_cnt_to_read) * BLOCK_SIZE > buf.len() {
                        for i in 0..spc {
                            let block_id = start_block_id + i;
                            let option = get_block_cache(block_id, Arc::clone(&self.device));
                            if let Some(cache) = option {
                                cache.read().read(0, |buffer: &[u8; 512]| {
                                    buf_read.copy_from_slice(buffer);
                                });
                            } else {
                                self.device
                                    .read_blocks(&mut buf_read, offset + i * BLOCK_SIZE, 1)
                                    .unwrap();
                            }
                            self.buf_read(&buf_read, blocks_have_read, buf);
                            blocks_have_read += 1;
                        }
                    } else {
                        // buf 还未填充完, 可以读入完整的块
                        for i in 0..spc {
                            let block_id = start_block_id + i;
                            let option = get_block_cache(block_id, Arc::clone(&self.device));
                            if let Some(cache) = option {
                                cache.read().read(0, |buffer: &[u8; 512]| {
                                    buf[blocks_have_read * BLOCK_SIZE
                                        ..(blocks_have_read + 1) * BLOCK_SIZE]
                                        .copy_from_slice(buffer);
                                });
                                blocks_have_read += 1;
                            } else {
                                let block_cnt = block_cnt_to_read - i;
                                self.device
                                    .read_blocks(
                                        &mut buf[blocks_have_read * BLOCK_SIZE
                                            ..(blocks_have_read + 1) * BLOCK_SIZE],
                                        offset + i * BLOCK_SIZE,
                                        block_cnt,
                                    )
                                    .unwrap();
                                blocks_have_read += block_cnt;
                                break;
                            }
                        }
                    }
                } else {
                    // 需要读的扇区数不足一个簇的扇区数, 即最后一个簇

                    // -1 前block_cnt_to_read - 1个块可以读完
                    for i in 0..block_cnt_to_read - 1 {
                        let block_id = start_block_id + i;
                        let option = get_block_cache(block_id, Arc::clone(&self.device));
                        if let Some(cache) = option {
                            cache.read().read(0, |buffer: &[u8; 512]| {
                                buf[blocks_have_read * BLOCK_SIZE
                                    ..(blocks_have_read + 1) * BLOCK_SIZE]
                                    .copy_from_slice(buffer);
                            });
                            blocks_have_read += 1;
                        } else {
                            let block_cnt = block_cnt_to_read - 1 - i;
                            self.device
                                .read_blocks(
                                    &mut buf[blocks_have_read * BLOCK_SIZE
                                        ..(blocks_have_read + 1) * BLOCK_SIZE],
                                    offset + i * BLOCK_SIZE,
                                    block_cnt,
                                )
                                .unwrap();
                            blocks_have_read += block_cnt;
                            break;
                        }
                    }

                    let block_id = start_block_id + block_cnt_to_read - 1;
                    let option = get_block_cache(block_id, Arc::clone(&self.device));
                    if let Some(cache) = option {
                        cache.read().read(0, |buffer: &[u8; 512]| {
                            buf_read.copy_from_slice(buffer);
                        });
                    } else {
                        self.device
                            .read_blocks(
                                &mut buf_read,
                                offset + (block_cnt_to_read - 1) * BLOCK_SIZE,
                                1,
                            )
                            .unwrap();
                    }

                    self.buf_read(&buf_read, blocks_have_read, buf);
                    blocks_have_read += 1;
                }
            })
            .last();

        assert!(sec_cnt_to_read == 0);
        blocks_have_read * BLOCK_SIZE
    }

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

        let mut already_read = 0;
        // 3. 读取数据
        while file_start_pointer < file_end_bound {
            // cluster offset byte in disk
            let cluster_offset_in_disk = self.bpb.offset(fat.current_cluster as u32);
            // block id range of the current cluster
            let start_block_id_in_disk = cluster_offset_in_disk / BLOCK_SIZE;
            let end_block_id_in_disk = cluster_offset_in_disk / BLOCK_SIZE + spc;

            // for (block_id_in_cluster, block_id) in
            //     (start_block_id_in_disk..end_block_id_in_disk).enumerate()
            for (_, block_id) in (start_block_id_in_disk..end_block_id_in_disk).enumerate() {
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
                        cache.read().read(0, |buffer: &[u8; BLOCK_SIZE]| {
                            buf[already_read..already_read + len]
                                .copy_from_slice(&buffer[offset_in_block..offset_in_block + len]);
                        });
                        file_start_pointer += len;
                        already_read += len;
                    } else {
                        // cache 无法获取: lru_cache 暂时没法释放一个 cache, 此时直接从磁盘读取
                        // TODO
                        // 读取一个簇还是一个块?

                        // 读取一个块
                        let offset_in_block = file_start_pointer - start_block_byte_in_file;
                        let len =
                            (BLOCK_SIZE - offset_in_block).min(file_end_bound - file_start_pointer);
                        let mut buffer = [0u8; BLOCK_SIZE];
                        self.device
                            .read_blocks(&mut buffer, block_id * BLOCK_SIZE, 1)
                            .unwrap();
                        buf[already_read..already_read + len]
                            .copy_from_slice(&buffer[offset_in_block..offset_in_block + len]);
                        file_start_pointer += len;
                        already_read += len;

                        // 读取一个簇
                        // let offset_in_block = file_start_pointer - start_block_byte_in_file;
                        // let len = (BLOCK_SIZE * (spc - block_id_in_cluster) - offset_in_block)
                        //     .min(file_end_bound - file_start_pointer);
                        // TODO perf: add cluster cache for BlockCache
                        // let mut cluster_buffer = Vec::<u8>::with_capacity(cluster_size);
                        // self.device
                        //     .read_blocks(
                        //         cluster_buffer.as_mut_slice(),
                        //         start_block_id_in_disk * BLOCK_SIZE,
                        //         spc,
                        //     )
                        //     .unwrap();
                        // let offset_in_cluster = block_id_in_cluster * BLOCK_SIZE + offset_in_block;
                        // buf[read_size..read_size + len].copy_from_slice(
                        //     &cluster_buffer[offset_in_cluster..offset_in_cluster + len],
                        // );
                        // file_start_pointer += len;
                        // read_size += len;
                        // break;
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

        Ok(already_read)
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

        // TODO
        // check
        let mut file_pointer = 0;
        self.fat
            .clone()
            .map(|f| {
                let cluster_offset_in_disk = self.bpb.offset(f.current_cluster);

                let end = if (length - file_pointer) < cluster_size {
                    // 读取长度在一个簇之内
                    let bytes_left = length % cluster_size;
                    block_cnt = get_needed_sector(bytes_left);
                    file_pointer + bytes_left
                } else {
                    // 读取长度超过一个簇的大小
                    file_pointer + cluster_size
                };

                for i in 0..block_cnt {
                    assert!(cluster_offset_in_disk % BLOCK_SIZE == 0);
                    let block_id = cluster_offset_in_disk / BLOCK_SIZE + i;
                    let option = get_block_cache(block_id, Arc::clone(&self.device));
                    if let Some(cache) = option {
                        let len = (BLOCK_SIZE).min(end - file_pointer);
                        let mut block_buffer = [0u8; BLOCK_SIZE];
                        cache.read().read(0, |buffer: &[u8; BLOCK_SIZE]| {
                            block_buffer.copy_from_slice(buffer);
                        });
                        buf[file_pointer..file_pointer + len]
                            .copy_from_slice(&block_buffer[0..len]);
                        file_pointer += len;
                    } else {
                        // TODO
                        // 使用更小/合适的缓存
                        let mut cluster_buffer = Vec::<u8>::with_capacity(cluster_size);
                        let len = end - file_pointer;
                        self.device
                            .read_blocks(
                                cluster_buffer.as_mut_slice(),
                                cluster_offset_in_disk + i * BLOCK_SIZE,
                                block_cnt - i,
                            )
                            .unwrap();
                        buf[file_pointer..end]
                            .copy_from_slice(&cluster_buffer[i * BLOCK_SIZE..i * BLOCK_SIZE + len]);
                        file_pointer += cluster_size;
                    }
                }
            })
            .last();

        Ok(length)
    }

    /// Write Buffer To File, Return File Length
    ///
    /// Based on geiven file offset and write file content from buf
    //
    // pub fn write_at_(&mut self, buf: &[u8], offset: usize) -> Result<usize, FileError> {
    pub fn write_at_(&mut self, buf: &[u8], offset: usize) -> Result<(), FileError> {
        let spc = self.bpb.sector_per_cluster_usize();
        let cluster_size = spc * BLOCK_SIZE;

        let cluster_cnt = self.num_cluster(buf.len() + offset);
        let exist_cluster_cnt = self.num_cluster(self.sde.file_size().unwrap());
        let offset_cluster_cnt = offset % cluster_size;
        // let cluster_cnt_to_write = cluster_cnt - offset_cluster_cnt;
        let mut fat = self.fat.clone();
        for _ in 0..offset_cluster_cnt {
            fat = fat.next().unwrap();
        }

        // 簇号处理
        if cluster_cnt > exist_cluster_cnt {
            // 需要分配新的簇
            let mut f = fat.clone();
            f.find(|_| false);
            // 此时 fat 指向最后一个簇, 该簇在 fat 表中的内容为 EOC
            // 将 current_cluster 的值由 EOF 改为新分配的 bl
            let bl = self.write_blank_fat(cluster_cnt - exist_cluster_cnt);
            f.write(f.current_cluster, bl);
            f.refresh(bl);
        } else if cluster_cnt < exist_cluster_cnt {
            // 不需要分配新的簇
            // TODO
            // 释放多余的簇 todo :manage the empty cluster
            fat.clone()
                .map(|mut f| f.write(f.current_cluster, 0))
                .last();

            fat.write(fat.current_cluster, END_OF_CLUSTER);
        }

        // 填充当前 sector 空余的地方
        // TODO 已经有 cluster_cnt 是不是不用判断是否 need_new_cluster
        let (need_new_cluster, index) =
            self.fill_rest_sector_in_cluster(buf, fat.current_cluster, offset);
        if need_new_cluster {
            // buf: 还未写的数据
            let buf_left = &buf[index..];
            // 完善根据 buf 长度簇链 (分配簇)
            let bl = self.write_blank_fat(cluster_cnt - exist_cluster_cnt);
            fat.write(fat.current_cluster, bl);
            // TODO
            // Q: why refresh
            // A: refresh to trigger next() for updating prev_cluster and next_cluster
            fat.refresh(bl);

            self.basic_write(buf_left, &fat);
        }

        // 更新文件大小
        self.update_file_size(buf.len() + offset);

        Ok(())
    }

    /// Write Data To File, Using Append OR OverWritten
    pub fn write(&mut self, buf: &[u8], write_type: WriteType) -> Result<(), FileError> {
        let cluster_cnt = match write_type {
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

                // TODO bug: 似乎没有续上
                // 重新设置链接情况
                let bl = self.write_blank_fat(cluster_cnt);
                self.fat.write(self.fat.start_cluster, bl);
                self.basic_write(buf, &self.fat);
            }
            WriteType::Append => {
                let mut fat = self.fat.clone();
                let exist_cluster_cnt = fat.clone().count();
                // 修改 fat: 迭代 fat 使 fat.current_cluster 为簇链的最后一个簇, 即找到最后一个簇的位置
                fat.find(|_| false);

                // 填充当前 sector 空余的地方
                // TODO 已经有 cluster_cnt 是不是不用判断是否 need_new_cluster
                let (need_new_cluster, index) = self.fill_rest_sector_in_cluster(
                    buf,
                    fat.current_cluster,
                    self.sde.file_size().unwrap(),
                );
                if need_new_cluster {
                    // buf: 还未写的数据
                    let buf_left = &buf[index..];
                    // 完善根据 buf 长度簇链 (分配簇)
                    let bl = self.write_blank_fat(cluster_cnt - exist_cluster_cnt);
                    fat.write(fat.current_cluster, bl);
                    // TODO
                    // Q: why refresh
                    // A: refresh to trigger next() for updating prev_cluster and next_cluster
                    fat.refresh(bl);

                    self.basic_write(buf_left, &fat);
                }
            }
        }

        match write_type {
            WriteType::OverWritten => self.update_file_size(buf.len()),
            WriteType::Append => self.update_file_size(buf.len() + self.sde.file_size().unwrap()),
        };

        Ok(())
    }

    /// Fill Left Sectors in Given Cluster
    /// Used for Write Append
    //
    //  使用 buf 填充簇中剩余的扇区
    //  返回值
    //  - bool: 是否需要新的簇
    //  - usize: 已经填充的字节数
    fn fill_rest_sector_in_cluster(
        &self,
        buf: &[u8],
        cluster_id: u32,
        offset: usize,
    ) -> (bool, usize) {
        let spc = self.bpb.sector_per_cluster_usize();
        // Q: 如果length 超过一个簇的大小怎么办? -> retern false, 上层创建簇链再继续写
        // let length = self.sde.file_size().unwrap();
        let length = offset;
        // 获取已经使用的扇区数
        let get_used_sector = |len: usize| {
            if len % (spc * BLOCK_SIZE) == 0 && length != 0 {
                // 刚好占满一个簇
                spc
            } else {
                (len % (spc * BLOCK_SIZE)) / BLOCK_SIZE
            }
        };
        // 文件原本的大小是否刚好占满一个扇区/块
        let left_start = length % BLOCK_SIZE;
        let blank_size = BLOCK_SIZE - left_start;

        // 已经填充的字节数
        let mut already_fill = 0;
        // buf 中是否还有剩余
        let mut buf_has_left = true;
        // TODO 合并already_fill 和 index
        let mut index = 0;
        let mut used_sector = get_used_sector(length);
        let mut data = [0; BLOCK_SIZE];
        let mut offset = self.bpb.offset(cluster_id) + used_sector * BLOCK_SIZE;
        assert!(offset % BLOCK_SIZE == 0);
        let block_id = offset / BLOCK_SIZE;

        // TODO
        // check
        // 先尝试填充一个扇区/块
        if left_start != 0 {
            // 文件原本的大小不是刚好占满一个扇区/块
            let option = get_block_cache(block_id, Arc::clone(&self.device));
            if let Some(cache) = option {
                cache.read().read(0, |buffer: &[u8; 512]| {
                    data.copy_from_slice(buffer);
                });
            } else {
                self.device.read_blocks(&mut data, offset, 1).unwrap();
            }

            // self.device.read_blocks(&mut data, offset, 1).unwrap();
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
            let option = get_block_cache(block_id, Arc::clone(&self.device));
            if let Some(cache) = option {
                cache.write().modify(0, |buffer: &mut [u8; 512]| {
                    buffer.copy_from_slice(&data);
                });
            } else {
                self.device.write_blocks(&data, offset, 1).unwrap();
            }

            offset = self.bpb.offset(cluster_id) + BLOCK_SIZE;
        }

        // 填充剩余的扇区/块
        if buf_has_left {
            let buf_needed_sector = get_needed_sector(buf.len() - already_fill);
            let cluster_left_sector = spc - used_sector;
            // 如果 buf_needed_sector 大于剩余的扇区数, 则只写入剩余的扇区数, 说明跨簇了
            let num_sector = cmp::min(cluster_left_sector, buf_needed_sector);
            for s in 0..num_sector {
                self.buf_write(&buf[index..], s, &mut data);
                let block_id = offset / BLOCK_SIZE + s;
                assert!(offset % BLOCK_SIZE == 0);
                let option = get_block_cache(block_id, Arc::clone(&self.device));
                if let Some(cache) = option {
                    cache.write().modify(0, |buffer: &mut [u8; 512]| {
                        buffer.copy_from_slice(&data);
                    });
                } else {
                    self.device
                        .write_blocks(&data, offset + s * BLOCK_SIZE, 1)
                        .unwrap();
                }
                index += BLOCK_SIZE;
            }

            // 如果 buf_needed_sector 大于剩余的扇区数, 则只写入剩余的扇区数, 说明跨簇了
            if buf_needed_sector > cluster_left_sector {
                return (true, index);
            }
        }

        (false, index)
    }

    /// Basic Write Function
    fn basic_write(&self, buf: &[u8], fat: &ClusterChain) {
        let spc = self.bpb.sector_per_cluster_usize();
        let mut buf_write = [0; BLOCK_SIZE];
        // sec_cnt: sector counts need to write
        let mut sec_cnt_to_write = get_needed_sector(buf.len());

        let get_slice = |start_block_id: usize, block_cnt: usize| -> &[u8] {
            &buf[start_block_id * BLOCK_SIZE..(start_block_id + block_cnt) * BLOCK_SIZE]
        };

        // TODO
        // check
        // 已经写入的扇区数
        let mut writen_block = 0;
        fat.clone()
            .map(|f| {
                // blk_cnt: block count need to write
                let block_cnt_to_write = if sec_cnt_to_write / spc > 0 {
                    // 如果需要写入的扇区数超过了一个簇的扇区数
                    sec_cnt_to_write -= spc;
                    spc
                } else {
                    sec_cnt_to_write
                };

                let offset = self.bpb.offset(f.current_cluster);
                assert!(offset % BLOCK_SIZE == 0);
                let start_block_id = offset / BLOCK_SIZE;

                // 如果需要写入的扇区数超过了一个簇的扇区数
                if block_cnt_to_write == spc {
                    // buf 中的数据可以写完
                    if (writen_block + block_cnt_to_write) * BLOCK_SIZE > buf.len() {
                        for i in 0..block_cnt_to_write {
                            self.buf_write(&buf, writen_block, &mut buf_write);

                            // get one block cache
                            let block_id = start_block_id + i;
                            let option = get_block_cache(block_id, Arc::clone(&self.device));
                            if let Some(cache) = option {
                                cache.write().modify(0, |buffer: &mut [u8; 512]| {
                                    buffer.copy_from_slice(&buf_write);
                                });
                            } else {
                                self.device
                                    .write_blocks(&buf_write, offset + i * BLOCK_SIZE, 1)
                                    .unwrap();
                            }

                            writen_block += 1;
                        }
                    } else {
                        // buf 中的数据还没写完, 可以写完整的簇
                        for i in 0..block_cnt_to_write {
                            let block_id = start_block_id + i;
                            let option = get_block_cache(block_id, Arc::clone(&self.device));
                            if let Some(cache) = option {
                                cache.write().modify(0, |buffer: &mut [u8; 512]| {
                                    buffer.copy_from_slice(
                                        &buf[writen_block * BLOCK_SIZE
                                            ..(writen_block + 1) * BLOCK_SIZE],
                                    );
                                });
                                writen_block += 1;
                            } else {
                                let block_cnt = block_cnt_to_write - i;
                                self.device
                                    .write_blocks(
                                        get_slice(writen_block, block_cnt),
                                        offset + i * BLOCK_SIZE,
                                        block_cnt,
                                    )
                                    .unwrap();
                                writen_block += block_cnt;
                                break;
                            }
                        }
                    }
                } else {
                    // 如果需要写入的扇区数没有超过一个簇的扇区数

                    // block_cnt_to_write - 1 是因为 block_cnt_to_write 这个扇区不一定完全写完
                    for i in 0..block_cnt_to_write - 1 {
                        let block_id = start_block_id + i;
                        let option = get_block_cache(block_id, Arc::clone(&self.device));
                        if let Some(cache) = option {
                            cache.write().modify(0, |buffer: &mut [u8; 512]| {
                                buffer.copy_from_slice(
                                    &buf[writen_block * BLOCK_SIZE
                                        ..writen_block * BLOCK_SIZE + BLOCK_SIZE],
                                );
                            });
                            writen_block += 1;
                        } else {
                            let block_cnt = block_cnt_to_write - 1 - i;
                            self.device
                                .write_blocks(
                                    get_slice(writen_block, block_cnt),
                                    offset + i * BLOCK_SIZE,
                                    block_cnt,
                                )
                                .unwrap();
                            writen_block += block_cnt;
                            break;
                        }
                    }

                    // 对 block_cnt_to_write 这个扇区进行写
                    self.buf_write(&buf, writen_block, &mut buf_write);

                    let block_id = start_block_id + block_cnt_to_write - 1;
                    let option = get_block_cache(block_id, Arc::clone(&self.device));
                    if let Some(cache) = option {
                        cache.write().modify(0, |buffer: &mut [u8; 512]| {
                            buffer.copy_from_slice(&buf_write);
                        });
                    } else {
                        self.device
                            .write_blocks(
                                &buf_write,
                                offset + (block_cnt_to_write - 1) * BLOCK_SIZE,
                                1,
                            )
                            .unwrap();
                    }
                    writen_block += 1;
                }
            })
            .last();
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
        // 向上取整
        if length % cluster_size != 0 {
            length / cluster_size + 1
        } else {
            length / cluster_size
        }
    }

    /// Write Buffer from one to another one.
    /// Write length is BLOCK_SIZE
    ///
    /// - block_idx: block/sector id in file
    fn buf_write(&self, from: &[u8], block_idx: usize, to: &mut [u8]) {
        let index = block_idx * BLOCK_SIZE;
        let index_end = index + BLOCK_SIZE;
        if from.len() < index_end {
            let len = from.len() - index;
            to.copy_from_slice(&[0; BLOCK_SIZE]);
            to[0..len].copy_from_slice(&from[index..])
        } else {
            to.copy_from_slice(&from[index..index_end])
        }
    }

    // TODO check
    fn buf_read(&self, from: &[u8], block_idx: usize, to: &mut [u8]) {
        let index = block_idx * BLOCK_SIZE;
        let index_end = index + BLOCK_SIZE;
        let to_len = to.len();
        if to_len < index_end {
            let len = to_len - index;
            to[index..].copy_from_slice(&from[0..len])
        } else {
            to[index..index_end].copy_from_slice(&from);
        }
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
    //  完善簇链
    fn write_blank_fat(&mut self, num_cluster: usize) -> u32 {
        let mut ret = END_OF_CLUSTER;
        for n in 0..num_cluster {
            // 类似于创建一个空节点
            // 注意, 在调用 write_blank_fat 之前, 会调用
            let bl1 = self.fat.blank_cluster();
            if n == 0 {
                ret = bl1;
            }
            self.fat.write(bl1, END_OF_CLUSTER);

            if n != num_cluster - 1 {
                let bl2 = self.fat.blank_cluster();
                // 类似于 bl1.val = bl2
                self.fat.write(bl1, bl2);
            }
        }
        ret
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
