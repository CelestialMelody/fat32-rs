use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::clone::Clone;
use core::cmp::Ord;
use core::convert::AsMut;
use core::convert::AsRef;
use core::convert::From;
use core::ops::FnOnce;
use core::option::Option::{self, None, Some};
use core::{assert, assert_eq, assert_ne, todo};
use spin::RwLock;

use super::cache::get_block_cache;

use super::cache::{BlockCache, Cache};
use super::device::BlockDevice;
use super::entry::{LongDirEntry, ShortDirEntry};
use super::fat::ClusterChain;
use super::fs::FileSystem;
use super::{
    ATTR_ARCHIVE, ATTR_DIRECTORY, ATTR_LONG_NAME, BLOCK_SIZE, DIRENT_SIZE, END_OF_CLUSTER,
    NEW_VIR_FILE_CLUSTER, ROOT_DIR_ENTRY_CLUSTER, STRAT_CLUSTER_IN_FAT,
};

#[derive(Clone)]
pub struct VirFile {
    pub(crate) name: String,
    pub(crate) sde_pos: DirEntryPos,
    pub(crate) lde_pos: Vec<DirEntryPos>,
    pub(crate) fs: Arc<RwLock<FileSystem>>,
    pub(crate) device: Arc<dyn BlockDevice>,
    pub(crate) cluster_chain: Arc<RwLock<ClusterChain>>,
    pub(crate) attr: VirFileType,
}

pub fn root(fs: Arc<RwLock<FileSystem>>) -> VirFile {
    let fs = Arc::clone(&fs);
    let device = Arc::clone(&fs.read().device);

    // fix
    let root_dir_cluster = fs.read().bpb.root_cluster();

    let cluster_chain = Arc::new(RwLock::new(ClusterChain::new(
        root_dir_cluster as u32,
        Arc::clone(&device),
        fs.read().bpb.fat1_offset(),
    )));

    // Set root next cluster
    fs.write()
        .fat
        .write()
        .set_next_cluster(root_dir_cluster as u32, END_OF_CLUSTER);

    let root_dir = VirFile::new(
        String::from("/"),
        DirEntryPos {
            start_cluster: ROOT_DIR_ENTRY_CLUSTER,
            offset_in_cluster: 0,
        },
        Vec::new(),
        fs,
        device,
        cluster_chain,
        VirFileType::Dir,
    );

    // init file_size
    let spc = root_dir.fs.read().bpb.sectors_per_cluster();
    let mut entry = LongDirEntry::empty();
    let mut index = 0usize;
    let mut file_size = 0usize;
    let mut left = 0usize;

    let mut curr_cluster = root_dir_cluster as u32;
    let mut clus_chain = root_dir.cluster_chain.read().clone().next().unwrap();

    loop {
        let cluster_offset_in_disk = root_dir.fs.read().bpb.offset(curr_cluster);
        let start_block_id = cluster_offset_in_disk / BLOCK_SIZE;
        for block_id in start_block_id..start_block_id + spc {
            while index >= left {
                let buf = entry.as_bytes_mut();
                let offset_in_block = index - left;
                let len = buf.len();

                let option = get_block_cache(block_id, Arc::clone(&root_dir.device));
                if let Some(block) = option {
                    block.read().read(0, |cache: &[u8; BLOCK_SIZE]| {
                        let dst = &mut buf[..len];
                        let src = &cache[offset_in_block..offset_in_block + len];
                        dst.copy_from_slice(src);
                    });
                } else {
                    let mut cache = [0u8; BLOCK_SIZE];
                    root_dir
                        .device
                        .read_blocks(&mut cache, block_id * BLOCK_SIZE, 1)
                        .unwrap();
                    let dst = &mut buf[..len];
                    let src = &cache[offset_in_block..offset_in_block + len];
                    dst.copy_from_slice(src);
                }

                if entry.is_empty() {
                    break;
                }

                index += len;
                file_size += len;
            }
            left += BLOCK_SIZE;
        }

        if entry.is_empty() {
            break;
        }

        clus_chain = clus_chain.next().unwrap();
        curr_cluster = clus_chain.current_cluster;
    }

    root_dir.set_file_size(file_size);

    root_dir
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VirFileType {
    Dir = ATTR_DIRECTORY,
    File = ATTR_ARCHIVE,
}

#[derive(Clone, Copy, Debug)]
pub struct DirEntryPos {
    pub(crate) start_cluster: u32,
    pub(crate) offset_in_cluster: usize,
}

impl DirEntryPos {
    fn new(start_cluster: u32, offset_in_cluster: usize) -> Self {
        Self {
            start_cluster,
            offset_in_cluster,
        }
    }
}

impl VirFile {
    pub fn new(
        name: String,
        sde_pos: DirEntryPos,
        lde_pos: Vec<DirEntryPos>,
        fs: Arc<RwLock<FileSystem>>,
        device: Arc<dyn BlockDevice>,
        cluster_chain: Arc<RwLock<ClusterChain>>,
        attr: VirFileType,
    ) -> Self {
        Self {
            name,
            sde_pos,
            lde_pos,
            fs,
            device,
            cluster_chain,
            attr,
        }
    }

    // Dir Func
    pub fn file_cluster_chain(&self, sde_pos: usize) -> ClusterChain {
        let fat_offset = self.fs.read().bpb.fat1_offset();
        let (block_id, offset_in_block) = self.offset_block_pos(sde_pos).unwrap();
        let mut start_cluster: u32 = NEW_VIR_FILE_CLUSTER;

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(block) = option {
            block.read().read(offset_in_block, |sde: &ShortDirEntry| {
                start_cluster = sde.first_cluster();
            })
        } else {
            let mut buf = [0u8; BLOCK_SIZE];
            self.device
                .read_blocks(buf.as_mut(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            let mut sde = ShortDirEntry::empty();
            let src = &buf[offset_in_block..offset_in_block + DIRENT_SIZE];
            let dst = sde.as_bytes_mut();
            dst.copy_from_slice(src);
            assert_ne!(sde.first_cluster(), 0);
            start_cluster = sde.first_cluster();
        }

        ClusterChain::new(start_cluster, Arc::clone(&self.device), fat_offset)
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn sde_pos(&self) -> (usize, usize) {
        assert!(self.sde_pos.start_cluster != END_OF_CLUSTER);
        let cluster_id = self.sde_pos.start_cluster;
        let cluster_offset = self.fs.read().bpb.offset(cluster_id);
        let offset = self.sde_pos.offset_in_cluster + cluster_offset;
        let offset_in_block = offset % BLOCK_SIZE;
        let block_id = offset / BLOCK_SIZE;

        (block_id, offset_in_block)
    }

    pub fn lde_pos(&self, index: usize) -> (usize, usize) {
        assert!(self.lde_pos[index].start_cluster != END_OF_CLUSTER);
        let cluster_id = self.lde_pos[index].start_cluster;
        let cluster_offset = self.fs.read().bpb.offset(cluster_id);
        let offset = self.lde_pos[index].offset_in_cluster + cluster_offset;
        let offset_in_block = offset % BLOCK_SIZE;
        let block_id = offset / BLOCK_SIZE;

        (block_id, offset_in_block)
    }

    pub fn read_sde<V>(&self, f: impl FnOnce(&ShortDirEntry) -> V) -> V {
        // fix
        if self.sde_pos.start_cluster == ROOT_DIR_ENTRY_CLUSTER {
            let root_dir_entry = self.fs.read().root_dir_entry();
            let root_dir_entry_read = root_dir_entry.read();
            return f(&root_dir_entry_read);
        }

        let (block_id, offset_in_block) = self.sde_pos();

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(block) = option {
            block.read().read(offset_in_block, f)
        } else {
            let mut buf = [0u8; BLOCK_SIZE];
            self.device
                .read_blocks(buf.as_mut(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            let block = BlockCache::new(block_id, Arc::clone(&self.device));
            block.read(offset_in_block, f)
        }
    }

    pub fn modify_sde<V>(&self, f: impl FnOnce(&mut ShortDirEntry) -> V) -> V {
        // fix
        if self.sde_pos.start_cluster == ROOT_DIR_ENTRY_CLUSTER {
            let root_dir_entry = self.fs.read().root_dir_entry();
            let mut root_dir_entry_write = root_dir_entry.write();
            return f(&mut root_dir_entry_write);
        }

        let (block_id, offset_in_block) = self.sde_pos();

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(block) = option {
            block.write().modify(offset_in_block, f)
        } else {
            let mut buf = [0u8; BLOCK_SIZE];
            self.device
                .read_blocks(buf.as_mut(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            let mut block = BlockCache::new(block_id, Arc::clone(&self.device));
            let res = block.modify(offset_in_block, f);
            self.device
                .write_blocks(buf.as_ref(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            res
        }
    }

    pub fn read_lde<V>(&self, index: usize, f: impl FnOnce(&LongDirEntry) -> V) -> V {
        let (block_id, offset_in_block) = self.lde_pos(index);

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(block) = option {
            block.read().read(offset_in_block, f)
        } else {
            let mut buf = [0u8; BLOCK_SIZE];
            self.device
                .read_blocks(buf.as_mut(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            let block = BlockCache::new(block_id, Arc::clone(&self.device));
            block.read(offset_in_block, f)
        }
    }

    pub fn modify_lde<V>(&self, index: usize, f: impl FnOnce(&mut LongDirEntry) -> V) -> V {
        let (block_id, offset_in_block) = self.lde_pos(index);

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(block) = option {
            block.write().modify(offset_in_block, f)
        } else {
            let mut buf = [0u8; BLOCK_SIZE];
            self.device
                .read_blocks(buf.as_mut(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            let mut block = BlockCache::new(block_id, Arc::clone(&self.device));
            let res = block.modify(offset_in_block, f);
            self.device
                .write_blocks(buf.as_ref(), block_id * BLOCK_SIZE, 1)
                .unwrap();
            res
        }
    }

    pub fn file_size(&self) -> usize {
        self.read_sde(|sde| sde.file_size() as usize)
    }

    pub fn is_dir(&self) -> bool {
        self.attr == VirFileType::Dir
    }

    pub fn is_file(&self) -> bool {
        self.attr == VirFileType::File
    }

    pub fn offset_block_pos(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.file_size() {
            return None;
        }

        let cluster_size = self.fs.read().cluster_size();
        let cluster_index = offset / cluster_size;
        let offset_in_cluster = offset % cluster_size;

        let start_cluster = self.first_cluster();
        let cluster = self
            .fs
            .read()
            .fat
            .read()
            .get_cluster_at(start_cluster as u32, cluster_index as u32)
            .unwrap(); // assert offset < file_size()
        let offset_in_disk = self.fs.read().bpb.offset(cluster);

        let block_id = offset_in_disk / BLOCK_SIZE + offset_in_cluster / BLOCK_SIZE;
        assert!(offset_in_disk % BLOCK_SIZE == 0);
        let offset_in_block = offset_in_cluster % BLOCK_SIZE;

        Some((block_id, offset_in_block))
    }

    pub fn dir_entry_pos(&self, offset: usize) -> Option<DirEntryPos> {
        if offset > self.file_size() {
            return None;
        }
        let cluster_size = self.fs.read().cluster_size();
        let cluster_index = offset / cluster_size;
        let offset_in_cluster = offset % cluster_size;

        let start_cluster = self.first_cluster();
        let cluster = self
            .fs
            .read()
            .fat
            .read()
            .get_cluster_at(start_cluster as u32, cluster_index as u32)
            .unwrap();

        Some(DirEntryPos::new(cluster, offset_in_cluster))
    }

    pub fn set_first_cluster(&self, cluster: usize) {
        self.modify_sde(|sde| sde.set_first_cluster(cluster as u32));
    }

    pub fn set_file_size(&self, size: usize) {
        self.modify_sde(|sde| sde.set_file_size(size as u32));
    }

    pub fn first_cluster(&self) -> usize {
        self.read_sde(|sde| sde.first_cluster() as usize)
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let spc = self.fs.read().bpb.sectors_per_cluster();
        let cluster_size = self.fs.read().cluster_size();

        let mut index = offset;

        // fix
        let file_size = self.file_size();

        let end = (offset + buf.len()).min(file_size);
        // let end = offset + buf.len();

        // > not >=
        if offset > file_size || buf.len() == 0 {
            // if buf.len() == 0 {
            return 0;
        }
        let pre_cluster_cnt = offset / cluster_size;
        let mut curr_cluster = self.first_cluster() as u32;

        let mut clus_chain = self.cluster_chain.read().clone();

        // assert_eq!(clus_chain.current_cluster, NEW_VIR_FILE_CLUSTER);
        assert_ne!(clus_chain.start_cluster, 0);

        clus_chain = clus_chain.next().unwrap();

        for _ in 0..pre_cluster_cnt {
            curr_cluster = self
                .fs
                .read()
                .fat
                .read()
                .get_next_cluster(curr_cluster)
                .unwrap();

            clus_chain = clus_chain.next().unwrap();
            assert_eq!(curr_cluster, clus_chain.current_cluster);
            // curr_cluster = clus_chain.current_cluster;
        }

        let mut left = pre_cluster_cnt * cluster_size;
        let mut right = left + BLOCK_SIZE;
        let mut already_read = 0;

        while index < end {
            let cluster_offset_in_disk = self.fs.read().bpb.offset(curr_cluster);

            let start_block_id = cluster_offset_in_disk / BLOCK_SIZE;

            for block_id in start_block_id..start_block_id + spc {
                if index >= left && index < right && index < end {
                    let offset_in_block = index - left;
                    let len = (BLOCK_SIZE - offset_in_block).min(end - index);

                    let option = get_block_cache(block_id, Arc::clone(&self.device));
                    if let Some(block) = option {
                        block.read().read(0, |cache: &[u8; BLOCK_SIZE]| {
                            let dst = &mut buf[already_read..already_read + len];
                            let src = &cache[offset_in_block..offset_in_block + len];
                            dst.copy_from_slice(src);
                        });
                    } else {
                        let mut cache = [0u8; BLOCK_SIZE];
                        self.device
                            .read_blocks(&mut cache, block_id * BLOCK_SIZE, 1)
                            .unwrap();
                        let dst = &mut buf[already_read..already_read + len];
                        let src = &cache[offset_in_block..offset_in_block + len];
                        dst.copy_from_slice(src);
                    }

                    index += len;
                    already_read += len;

                    if index >= end {
                        break;
                    }
                }

                left += BLOCK_SIZE;
                right += BLOCK_SIZE;
            }

            if index >= end {
                break;
            }

            curr_cluster = self
                .fs
                .read()
                .fat
                .read()
                .get_cluster_at(curr_cluster, 1)
                .unwrap();

            // clus_chain = clus_chain.next().unwrap();
            // // assert_eq!(curr_cluster, clus_chain.current_cluster);
            // curr_cluster = clus_chain.current_cluster;
        }

        already_read
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let spc = self.fs.read().bpb.sectors_per_cluster();
        let cluster_size = self.fs.read().cluster_size();

        if buf.len() == 0 {
            return 0;
        }

        let mut index = offset;
        let end = offset + buf.len();

        let new_size = offset + buf.len();

        // TODO
        // self.modify_size(new_size);
        self.incerase_size(new_size);

        let pre_cluster_cnt = offset / cluster_size;

        let mut clus_chain = self.cluster_chain.read().clone().next().unwrap();

        let mut curr_cluster = self.first_cluster() as u32;
        for _ in 0..pre_cluster_cnt {
            curr_cluster = self
                .fs
                .read()
                .fat
                .read()
                .get_next_cluster(curr_cluster)
                .unwrap();

            clus_chain = clus_chain.next().unwrap();
            assert_eq!(curr_cluster, clus_chain.current_cluster);
            // curr_cluster = clus_chain.current_cluster;
        }

        let mut left = pre_cluster_cnt * cluster_size;
        let mut right = left + BLOCK_SIZE;
        let mut already_write = 0;

        while index < end {
            let cluster_offset_in_disk = self.fs.read().bpb.offset(curr_cluster);
            let start_block_id = cluster_offset_in_disk / BLOCK_SIZE;

            for block_id in start_block_id..start_block_id + spc {
                if index >= left && index < right && index < end {
                    let offset_in_block = index - left;
                    let len = (BLOCK_SIZE - offset_in_block).min(end - index);
                    let option = get_block_cache(block_id, Arc::clone(&self.device));
                    if let Some(block) = option {
                        block.write().modify(0, |cache: &mut [u8; BLOCK_SIZE]| {
                            let src = &buf[already_write..already_write + len];
                            let dst = &mut cache[offset_in_block..offset_in_block + len];
                            dst.copy_from_slice(src);
                        });
                    } else {
                        let mut cache = [0u8; BLOCK_SIZE];
                        self.device
                            .read_blocks(&mut cache, block_id * BLOCK_SIZE, 1)
                            .unwrap();
                        let src = &buf[already_write..already_write + len];
                        let dst = &mut cache[offset_in_block..offset_in_block + len];
                        dst.copy_from_slice(src);
                        self.device
                            .write_blocks(&cache, block_id * BLOCK_SIZE, 1)
                            .unwrap();
                    }
                    index += len;
                    already_write += len;

                    if index >= end {
                        break;
                    }
                }

                left += BLOCK_SIZE;
                right += BLOCK_SIZE;
            }

            if index >= end {
                break;
            }

            // curr_cluster = self
            //     .fs
            //     .read()
            //     .fat
            //     .read()
            //     .get_cluster_at(curr_cluster, 1)
            //     .unwrap();

            clus_chain = clus_chain.next().unwrap();
            // assert_eq!(curr_cluster, clus_chain.current_cluster);
            curr_cluster = clus_chain.current_cluster;
        }

        already_write
    }

    fn incerase_size(&self, new_size: usize) {
        let first_cluster = self.first_cluster() as u32;
        let old_size = self.file_size();
        if new_size <= old_size {
            return;
        }

        let need_cluster_cnt = self
            .fs
            .read()
            .count_needed_clusters(new_size, first_cluster);

        if need_cluster_cnt == 0 {
            self.modify_sde(|sde| {
                sde.set_file_size(new_size as u32);
            });
            return;
        }

        let option = self
            .fs
            .write()
            .alloc_cluster(need_cluster_cnt, first_cluster);

        if let Some(start_cluster) = option {
            if first_cluster == NEW_VIR_FILE_CLUSTER || first_cluster == ROOT_DIR_ENTRY_CLUSTER {
                self.cluster_chain.write().refresh(start_cluster);

                self.modify_sde(|sde| {
                    sde.set_first_cluster(start_cluster);
                });
            } else {
                let last_cluster = self.fs.read().fat.read().cluster_chain_tail(first_cluster);
                assert_ne!(last_cluster, NEW_VIR_FILE_CLUSTER);
                self.fs
                    .write()
                    .fat
                    .write()
                    .set_next_cluster(last_cluster, start_cluster);
            }

            self.modify_sde(|sde| {
                sde.set_file_size(new_size as u32);
            });
        } else {
            panic!("Alloc Cluster Failed! Out of Space!");
        }
    }

    #[allow(unused)]
    fn modify_size(&self, new_size: usize) {
        let first_cluster = self.first_cluster() as u32;
        let old_size = self.file_size();
        let cluster_size = self.fs.read().cluster_size();

        if new_size >= old_size {
            self.incerase_size(new_size);
        } else {
            let left = (new_size + cluster_size - 1) / cluster_size;
            let right = (old_size + cluster_size - 1) / cluster_size;
            let mut release_clsuter_vec = Vec::<u32>::new();
            for i in left..right {
                let cluster = self
                    .fs
                    .read()
                    .fat
                    .read()
                    .get_cluster_at(first_cluster, i as u32);
                assert!(cluster.is_some());
                let cluster = cluster.unwrap();
                release_clsuter_vec.push(cluster);
            }

            self.fs.write().dealloc_cluster(release_clsuter_vec);
            self.modify_sde(|sde| {
                sde.set_file_size(new_size as u32);
            });

            let last_clus = self
                .fs
                .read()
                .fat
                .read()
                .get_cluster_at(first_cluster, left as u32 - 1)
                .unwrap();
            assert!(last_clus >= 2);
            self.fs
                .write()
                .fat
                .write()
                .set_next_cluster(last_clus, END_OF_CLUSTER);
        }
    }

    // 删除自身
    pub fn clear(&self) -> usize {
        let first_cluster = self.first_cluster() as u32;
        for i in 0..self.lde_pos.len() {
            self.modify_lde(i, |lde: &mut LongDirEntry| {
                lde.delete();
            });
        }
        self.modify_sde(|sde: &mut ShortDirEntry| {
            sde.delete();
        });
        let all_clusters = self.fs.read().fat.read().get_all_cluster_id(first_cluster);
        let cluster_cnt = all_clusters.len();
        self.fs.write().dealloc_cluster(all_clusters);
        cluster_cnt
    }

    /// 返回: (st_size, st_blksize, st_blocks, is_dir, time)
    /// TODO 时间等
    pub fn stat(&self) -> (usize, usize, usize, bool, usize) {
        self.read_sde(|sde: &ShortDirEntry| {
            let first_cluster = sde.first_cluster();
            let mut file_size = sde.file_size() as usize;
            let spc = self.fs.read().sector_pre_cluster();
            let cluster_size = self.fs.read().cluster_size();
            let cluster_cnt = self.fs.read().fat.read().cluster_chain_len(first_cluster) as usize;

            let block_cnt = cluster_cnt * spc;
            if self.is_dir() {
                // 目录文件的 dir_file_size 字段为 0
                file_size = cluster_cnt * cluster_size;
            }
            (file_size, BLOCK_SIZE, block_cnt, self.is_dir(), 0)
        })
    }

    // 返回 (d_name, d_off, d_type)
    pub fn dir_info(&self, offset: usize) -> Option<(String, usize, usize, usize)> {
        if !self.is_dir() {
            return None;
        }
        let mut entry = LongDirEntry::empty();
        let mut index = offset;
        let mut name = String::new();
        let mut is_long = false;
        loop {
            let read_size = self.read_at(index, entry.as_bytes_mut());
            if read_size != DIRENT_SIZE || entry.is_empty() {
                return None;
            }
            if entry.is_deleted() {
                index += DIRENT_SIZE;
                name.clear();
                is_long = false;
                continue;
            }
            // 名称拼接
            if entry.attr() != ATTR_LONG_NAME {
                let sde: ShortDirEntry = unsafe { core::mem::transmute(entry) };
                if !is_long {
                    name = sde.get_name_lowercase();
                }
                let attribute = sde.attr();
                let first_cluster = sde.first_cluster();
                index += DIRENT_SIZE;
                return Some((name, index, first_cluster as usize, attribute as usize));
            } else {
                is_long = true;
                name.insert_str(0, &entry.name().as_str());
            }
            index += DIRENT_SIZE;
        }
    }

    pub fn set_time(&self, sec: u64, nsec: u64) {
        todo!("set_time");
    }
}
