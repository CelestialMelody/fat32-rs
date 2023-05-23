use super::cache::Cache;
use super::vfs::VirFile;

use super::{BLOCK_SIZE, NEW_VIR_FILE_CLUSTER};

use super::cache::get_block_cache;
use super::get_needed_sector;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::clone::Clone;
use core::cmp::Ord;
use core::option::Option::{self, None, Some};
use core::result::Result::{self, Err, Ok};

pub trait File {
    fn read(&self, buf: &mut [u8]) -> Result<usize, FileError>;

    fn write(&self, buf: &[u8], write_type: WriteType) -> Result<usize, FileError>;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WriteType {
    OverWritten,
    Append,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FileError {
    BufTooSmall,
    WriteError,
    ReadOutOfBound,
    BadClusterChain,
}

impl File for VirFile {
    /// Read File To Buffer, Return File Length
    fn read(&self, buf: &mut [u8]) -> Result<usize, FileError> {
        let file_size = self.file_size();
        let spc = self.fs.read().sector_pre_cluster();
        let cluster_size = spc * BLOCK_SIZE;
        let mut block_cnt = spc;

        if buf.len() < file_size {
            return Err(FileError::BufTooSmall);
        }

        let clus_chain: crate::ClusterChain = self.cluster_chain.read().clone();

        assert_eq!(clus_chain.current_cluster, NEW_VIR_FILE_CLUSTER);

        let mut index = 0;
        clus_chain
            .map(|f| {
                let offset_in_disk = self.fs.read().bpb.offset(f.current_cluster);

                let end = if (file_size - index) < cluster_size {
                    // 读取长度在一个簇之内
                    let bytes_left = file_size % cluster_size;
                    block_cnt = get_needed_sector(bytes_left);
                    index + bytes_left
                } else {
                    // 读取长度超过一个簇的大小
                    index + cluster_size
                };

                for i in 0..block_cnt {
                    assert!(offset_in_disk % BLOCK_SIZE == 0);
                    let block_id = offset_in_disk / BLOCK_SIZE + i;
                    let option = get_block_cache(block_id, Arc::clone(&self.device));
                    if let Some(cache) = option {
                        let len = (BLOCK_SIZE).min(end - index);

                        let mut block_buffer = [0u8; BLOCK_SIZE];
                        cache.read().read(0, |buffer: &[u8; BLOCK_SIZE]| {
                            block_buffer.copy_from_slice(buffer);
                        });

                        let dst = &mut buf[index..index + len];
                        let src = &block_buffer[0..len];
                        dst.copy_from_slice(src);

                        index += len;
                    } else {
                        // TODO
                        // 使用更小/合适的缓存
                        let mut cluster_buffer = Vec::<u8>::with_capacity(cluster_size);
                        let len = end - index;
                        self.device
                            .read_blocks(
                                cluster_buffer.as_mut_slice(),
                                offset_in_disk + i * BLOCK_SIZE,
                                block_cnt - i,
                            )
                            .unwrap();

                        let dst = &mut buf[index..index + len];
                        let src = &cluster_buffer[i * BLOCK_SIZE..i * BLOCK_SIZE + len];
                        dst.copy_from_slice(src);

                        index += len;
                    }
                }
            })
            .last();

        Ok(file_size)
    }

    fn write(&self, buf: &[u8], write_type: WriteType) -> Result<usize, FileError> {
        let file_size = self.file_size();

        let new_size: usize;

        let write_size = match write_type {
            WriteType::OverWritten => {
                new_size = buf.len();
                self.write_at(0, buf)
            }
            WriteType::Append => {
                new_size = file_size + buf.len();
                self.write_at(file_size, buf)
            }
        };

        self.set_file_size(new_size);

        Ok(write_size)
    }
}
