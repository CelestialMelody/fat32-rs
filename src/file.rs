use crate::block_device::BlockDevice;
use crate::bpb::BIOSParameterBlock;
use crate::dir::DirIter;
use crate::entry::Entry;
use crate::fat::FAT;
use crate::BlockDeviceError;
use crate::END_INVALID_CLUSTER;

use crate::BLOCK_SIZE;
use crate::FILE_BUFFER_SIZE;

use alloc::sync::Arc;
use core::cmp;
use core::fmt::Debug;

use crate::get_needed_sector;

/// Define FileError
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FileError {
    BufTooSmall,
    WriteError,
}

/// Define WriteType
pub enum WriteType {
    OverWritten,
    Append,
}

#[derive(Clone)]
pub struct File<'a> {
    pub(crate) device: Arc<dyn BlockDevice<Error = BlockDeviceError>>,
    pub(crate) bpb: &'a BIOSParameterBlock,
    pub(crate) dir_cluster: u32,
    pub(crate) detail: Entry,
    pub(crate) fat: FAT,
}

/// To Read File Per Sector By Iterator
pub struct ReadIter<'a> {
    device: Arc<dyn BlockDevice<Error = BlockDeviceError>>,
    // TODO: Use blcok cache manager to manage cache/buffer
    buffer: [u8; FILE_BUFFER_SIZE],
    bpb: &'a BIOSParameterBlock,
    fat: FAT,
    left_length: usize,
    read_count: usize,
    need_count: usize,
}

impl<'a> File<'a> {
    /// Read File To Buffer, Return File Length
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, FileError> {
        let length = self.detail.file_size().unwrap();
        let spc = self.bpb.sector_per_cluster_usize();
        let cluster_size = spc * BLOCK_SIZE;
        let mut block_id = spc;

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
                    block_id = get_needed_sector(bytes_left);
                    index + bytes_left
                } else {
                    // 读取长度超过一个簇的大小
                    index + cluster_size
                };
                self.device
                    .read(&mut buf[index..end], offset, block_id)
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
            WriteType::Append => self.num_cluster(buf.len() + self.detail.file_size().unwrap()),
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
                fat.find(|_| false);

                let (new_cluster, index) = self.fill_left_sector(buf, fat.current_cluster);
                if new_cluster {
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
            WriteType::OverWritten => self.update_length(buf.len()),
            WriteType::Append => self.update_length(buf.len() + self.detail.file_size().unwrap()),
        };

        Ok(())
    }

    /// Read Per Sector, Return ReadIter
    pub fn read_per_sector(&self) -> ReadIter {
        let left_length = self.detail.file_size().unwrap();
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
    fn buf_write(&self, from: &[u8], value: usize, to: &mut [u8]) {
        let index = value * BLOCK_SIZE;
        let index_end = index + BLOCK_SIZE;
        if from.len() < index_end {
            to.copy_from_slice(&[0; BLOCK_SIZE]);
            to[0..from.len() - index].copy_from_slice(&from[index..])
        } else {
            to.copy_from_slice(&from[index..index_end])
        }
    }

    /// Fill Left Sector
    fn fill_left_sector(&self, buf: &[u8], cluster: u32) -> (bool, usize) {
        let spc = self.bpb.sector_per_cluster_usize();
        let length = self.detail.file_size().unwrap();
        let get_used_sector = |len: usize| {
            if len % (spc * BLOCK_SIZE) == 0 && length != 0 {
                spc
            } else {
                len % (spc * BLOCK_SIZE) / BLOCK_SIZE
            }
        };
        let left_start = length % BLOCK_SIZE;
        let blank_size = BLOCK_SIZE - left_start;

        let mut already_fill = 0;
        let mut buf_has_left = true;
        let mut index = 0;
        let mut used_sector = get_used_sector(length);
        let mut data = [0; BLOCK_SIZE];
        let mut offset = self.bpb.offset(cluster) + used_sector * BLOCK_SIZE;

        if left_start != 0 {
            self.device.read(&mut data, offset, 1).unwrap();
            if buf.len() <= blank_size {
                data[left_start..left_start + buf.len()].copy_from_slice(&buf[0..]);
                buf_has_left = false;
            } else {
                data[left_start..].copy_from_slice(&buf[0..blank_size]);
                already_fill = blank_size;
                index = already_fill;
                used_sector = get_used_sector(length + already_fill);
                buf_has_left = true;
            };
            self.device.write(&data, offset, 1).unwrap();
            offset = self.bpb.offset(cluster) + BLOCK_SIZE;
        }

        if buf_has_left {
            let buf_needed_sector = get_needed_sector(buf.len() - already_fill);
            let the_cluster_left_sector = spc - used_sector;
            let num_sector = cmp::min(the_cluster_left_sector, buf_needed_sector);
            for s in 0..num_sector {
                self.buf_write(&buf[index..], s, &mut data);
                self.device
                    .write(&data, offset + s * BLOCK_SIZE, 1)
                    .unwrap();
                index += BLOCK_SIZE;
            }

            if buf_needed_sector > the_cluster_left_sector {
                return (true, index);
            }
        }

        (false, 0)
    }

    /// Update File Length
    fn update_length(&mut self, length: usize) {
        let fat = FAT::new(self.dir_cluster, Arc::clone(&self.device), self.bpb.fat1());
        let mut iter = DirIter::new(Arc::clone(&self.device), fat, self.bpb);
        iter.find(|d| {
            !d.is_deleted() && !d.is_lfn() && d.first_cluster() == self.detail.first_cluster()
        })
        .unwrap();

        self.detail.set_file_size(length);
        iter.previous();
        iter.update_item(&self.detail.sde_to_bytes_array().unwrap());
        iter.update();
    }

    /// Write Blank FAT
    //
    //  类似链表的插入
    fn write_blank_fat(&mut self, num_cluster: usize) {
        for n in 0..num_cluster {
            // 类似于创建一个空节点
            let bl1 = self.fat.blank_cluster();
            self.fat.write(bl1, END_INVALID_CLUSTER);

            let bl2 = self.fat.blank_cluster();
            if n != num_cluster - 1 {
                // 类似于 bl1.val = bl2
                self.fat.write(bl1, bl2);
            }
        }
    }

    /// Basic Write Function
    fn _write(&self, buf: &[u8], fat: &FAT) {
        let spc = self.bpb.sector_per_cluster_usize();
        let mut buf_write = [0; BLOCK_SIZE];
        let mut write_count = get_needed_sector(buf.len());
        let op = |start: usize, sectors: usize| -> &[u8] {
            &buf[start * BLOCK_SIZE..(start + sectors) * BLOCK_SIZE]
        };

        let mut w = 0;
        fat.clone()
            .map(|f| {
                let count = if write_count / spc > 0 {
                    write_count -= spc;
                    spc
                } else {
                    write_count
                };

                let offset = self.bpb.offset(f.current_cluster);
                if count == spc {
                    if (w + spc) * BLOCK_SIZE > buf.len() {
                        self.buf_write(&buf, w, &mut buf_write);
                        self.device.write(&buf_write, offset, 1).unwrap();
                    } else {
                        self.device.write(op(w, count), offset, count).unwrap();
                    }
                    w += count;
                } else {
                    self.device
                        .write(op(w, count - 1), offset, count - 1)
                        .unwrap();
                    w += count - 1;
                    self.buf_write(&buf, w, &mut buf_write);
                    self.device
                        .write(&buf_write, offset + (count - 1) * BLOCK_SIZE, 1)
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
        self.device.read(&mut self.buffer, offset, 1).unwrap();
        self.read_count += 1;

        Some(if self.read_count == self.need_count {
            (self.buffer, self.left_length)
        } else {
            self.left_length -= BLOCK_SIZE;
            (self.buffer, BLOCK_SIZE)
        })
    }
}
