use crate::block_cache::get_block_cache;
use crate::block_cache::Cache;
use crate::block_device::BlockDevice;
use crate::bpb::BIOSParameterBlock;
use crate::entry::Entry;
use crate::entry::NameType;
use crate::fat::ClusterChain;
use crate::file::File;
use crate::DIR_ENTRY_UNUSED;
use crate::DOT;
use crate::END_OF_CLUSTER;
use crate::SPACE;

use crate::BLOCK_SIZE;

use alloc::sync::Arc;
use core::fmt::Debug;

use crate::{generate_checksum, get_lde_cnt, get_lfn_index, is_illegal, sfn_or_lfn};

/// Define DirError
#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum DirError {
    NoMatchDir,
    NoMatchFile,
    IllegalChar,
    DirHasExist,
    FileHasExist,
}

/// Define Operation Type
#[derive(Clone, Copy)]
pub enum OpType {
    Dir,
    File,
}

#[derive(Clone)]
pub struct Dir<'a> {
    pub(crate) device: Arc<dyn BlockDevice>,
    pub(crate) bpb: &'a BIOSParameterBlock,
    pub(crate) sde: Entry,
    pub(crate) fat: ClusterChain,
}

impl<'a> Dir<'a> {
    /// Delete Dir
    pub fn delete_dir(&mut self, dir: &str) -> Result<(), DirError> {
        self.delete(dir, OpType::Dir)
    }

    /// Delete File
    pub fn delete_file(&mut self, file: &str) -> Result<(), DirError> {
        self.delete(file, OpType::File)
    }

    /// Create Dir
    pub fn create_dir(&mut self, dir: &str) -> Result<(), DirError> {
        self.create(dir, OpType::Dir)
    }

    /// Create File
    pub fn create_file(&mut self, file: &str) -> Result<(), DirError> {
        self.create(file, OpType::File)
    }

    /// Open File, Return File Type
    pub fn open_file(&self, file: &str) -> Result<File<'a>, DirError> {
        if is_illegal(file) {
            return Err(DirError::IllegalChar);
        }
        match self.exist(file) {
            None => Err(DirError::NoMatchFile),
            Some(dir_entry) => {
                if dir_entry.is_file() {
                    let fat = ClusterChain::new(
                        dir_entry.first_cluster(),
                        Arc::clone(&self.device),
                        self.bpb.fat1(),
                    );
                    Ok(File {
                        device: Arc::clone(&self.device),
                        bpb: self.bpb,
                        dir_cluster: self.sde.first_cluster(),
                        sde: dir_entry,
                        fat,
                    })
                } else {
                    Err(DirError::NoMatchFile)
                }
            }
        }
    }

    /// Cd Dir, Return Dir Type
    pub fn cd(&self, dir: &str) -> Result<Dir<'a>, DirError> {
        if is_illegal(dir) {
            return Err(DirError::IllegalChar);
        }
        match self.exist(dir) {
            None => Err(DirError::NoMatchDir),
            Some(di) => {
                if di.is_dir() {
                    let fat = ClusterChain::new(
                        di.first_cluster(),
                        Arc::clone(&self.device),
                        self.bpb.fat1(),
                    );
                    Ok(Self {
                        device: Arc::clone(&self.device),
                        bpb: self.bpb,
                        sde: di,
                        fat,
                    })
                } else {
                    Err(DirError::NoMatchDir)
                }
            }
        }
    }

    /// Check if file or dir is exist or not, Return Option Type
    pub fn exist(&self, value: &str) -> Option<Entry> {
        let mut iter = DirIter::new(Arc::clone(&self.device), self.fat.clone(), self.bpb);

        match sfn_or_lfn(value) {
            NameType::SFN => iter.find(|d| d.sfn_equal(value)),
            NameType::LFN => self.find_lfn(&mut iter, value),
        }
    }

    /// Check if file or dir is exist or not through DirIter, Return Option Type
    pub fn exist_iter(&self, iter: &mut DirIter, value: &str) -> Option<Entry> {
        match sfn_or_lfn(value) {
            NameType::SFN => iter.find(|d| d.sfn_equal(value)),
            NameType::LFN => self.find_lfn(iter, value),
        }
    }

    /// Find Long File Name Item, Return Option Type
    fn find_lfn(&self, iter: &mut DirIter, value: &str) -> Option<Entry> {
        let count = get_lde_cnt(value);
        // 最后一个 lde 对应于 value 的 index
        let mut index = get_lfn_index(value, count);
        let mut has_match = true;

        // TODO 倒着找? -> 倒着创建?

        // TODO 直接last?
        let result = iter.find(|d| {
            if d.is_lfn()
                && d.lde_order().unwrap() == count
                && d.is_lde_end().unwrap()
                && d.lfn_equal(&value[index..])
            {
                true
            } else {
                false
            }
        });

        if let Some(_) = result {
            for c in (1..count).rev() {
                let value = &value[0..index];
                index = get_lfn_index(value, c);

                let next = iter.next().unwrap();
                if next.lfn_equal(&value[index..]) {
                    continue;
                } else {
                    has_match = false;
                    break;
                }
            }
        }

        if has_match {
            iter.next()
        } else {
            None
        }
    }

    /// Basic Create Function
    fn create(&mut self, value: &str, create_type: OpType) -> Result<(), DirError> {
        if is_illegal(value) {
            return Err(DirError::IllegalChar);
        }
        if let Some(_) = self.exist(value) {
            return match create_type {
                OpType::Dir => Err(DirError::DirHasExist),
                OpType::File => Err(DirError::FileHasExist),
            };
        }

        let blank_cluster = self.fat.blank_cluster();
        self.fat.write(blank_cluster, END_OF_CLUSTER);

        match sfn_or_lfn(value) {
            NameType::SFN => {
                let di = Entry::new_sfn_str(blank_cluster, value, create_type);
                self.write_directory_item(di, NameType::SFN);
            }
            NameType::LFN => {
                // TODO 长文件名转短文件名
                let sfn = "unsupported".as_bytes();
                let check_sum = generate_checksum(sfn);
                let count = get_lde_cnt(value);
                // 最后一个 lde 对应于 value 的 index
                let mut lfn_index = get_lfn_index(value, count);

                let di =
                    Entry::new_lfn_str((count as u8) | (1 << 6), check_sum, &value[lfn_index..]);

                self.write_directory_item(di, NameType::LFN);

                // TODO 为什么倒着创建?
                for c in (1..count).rev() {
                    let value = &value[0..lfn_index];
                    lfn_index = get_lfn_index(value, c);
                    let di = Entry::new_lfn_str(c as u8, check_sum, &value[lfn_index..]);
                    self.write_directory_item(di, NameType::LFN);
                }

                let di = Entry::new_sfn_bytes(blank_cluster, sfn, create_type);
                self.write_directory_item(di, NameType::SFN);
            }
        }

        if let OpType::Dir = create_type {
            self.clean_cluster_data(blank_cluster);
            self.add_dot_item(blank_cluster);
        }
        Ok(())
    }

    /// Basic Delete Function
    fn delete(&mut self, name: &str, delete_type: OpType) -> Result<(), DirError> {
        if is_illegal(name) {
            return Err(DirError::IllegalChar);
        }
        let mut iter = DirIter::new(Arc::clone(&self.device), self.fat.clone(), self.bpb);

        match self.exist_iter(&mut iter, name) {
            None => {
                return match delete_type {
                    OpType::Dir => Err(DirError::NoMatchDir),
                    OpType::File => Err(DirError::NoMatchFile),
                }
            }
            Some(di) => {
                match delete_type {
                    OpType::Dir if di.is_file() => return Err(DirError::NoMatchDir),
                    OpType::File if di.is_dir() => return Err(DirError::NoMatchFile),
                    OpType::Dir => self.delete_in_dir(di.first_cluster()),
                    OpType::File => (),
                }
                self.fat.write(di.first_cluster(), 0);
            }
        }

        match sfn_or_lfn(name) {
            NameType::SFN => {
                iter.to_previous();
                iter.set_deleted();
                iter.update_in_disk();
            }
            NameType::LFN => {
                let count = get_lde_cnt(name);
                for _ in 0..=count {
                    iter.to_previous();
                    iter.set_deleted();
                    iter.update_in_disk();
                }
            }
        }
        Ok(())
    }

    /// Delete ALL File And Dir Which Included Deleted Dir
    fn delete_in_dir(&self, cluster: u32) {
        let fat_offset = self.bpb.fat1();
        let fat = ClusterChain::new(cluster, Arc::clone(&self.device), fat_offset);
        let mut iter = DirIter::new(Arc::clone(&self.device), fat, self.bpb);
        loop {
            if let Some(d) = iter.next() {
                if d.is_dir() {
                    self.delete_in_dir(d.first_cluster());
                }
                if d.is_deleted() {
                    continue;
                }
                iter.to_previous();
                iter.set_deleted();
                iter.update_in_disk();
                iter.next();
            } else {
                break;
            }
        }
    }

    /// Write Directory Item
    fn write_directory_item(&self, di: Entry, name_type: NameType) {
        let mut iter = DirIter::new(Arc::clone(&self.device), self.fat.clone(), self.bpb);
        iter.find(|_| false);
        let mut di_bytes: [u8; 32] = [0; 32];
        match name_type {
            NameType::SFN => {
                di_bytes.copy_from_slice(&di.sde_to_bytes_array().unwrap());
            }
            NameType::LFN => {
                di_bytes.copy_from_slice(&di.lde_to_bytes_array().unwrap());
            }
        }
        iter.update_item(&di_bytes);
        iter.update_in_disk();
    }

    /// Clean Sectors In Cluster, To Avoid Dirty Data
    fn clean_cluster_data(&self, cluster: u32) {
        let spc = self.bpb.sector_per_cluster_usize();
        for i in 0..spc {
            let offset = self.bpb.offset(cluster) + i * BLOCK_SIZE;
            let block_id = offset / BLOCK_SIZE;
            assert!(offset % BLOCK_SIZE == 0);

            let option = get_block_cache(block_id, Arc::clone(&self.device));
            if let Some(cache) = option {
                cache.write().modify(0, |cache: &mut [u8; BLOCK_SIZE]| {
                    cache.copy_from_slice(&[0; BLOCK_SIZE])
                })
            } else {
                self.device
                    .write_blocks(&[0; BLOCK_SIZE], offset, 1)
                    .unwrap();
            }
        }
    }

    /// Add '.' AND '..' Item
    fn add_dot_item(&self, cluster: u32) {
        let mut buffer = [0; BLOCK_SIZE];

        let mut value = [SPACE; 11];
        value[0] = b'.';
        let mut di = Entry::new_sfn_bytes(cluster, &value, OpType::Dir);
        buffer[0..32].copy_from_slice(&di.sde_to_bytes_array().unwrap());
        value[1] = b'.';
        di = Entry::new_sfn_bytes(self.sde.first_cluster(), &value, OpType::Dir);
        buffer[32..64].copy_from_slice(&di.sde_to_bytes_array().unwrap());

        let offset = self.bpb.offset(cluster);
        let block_id = offset / BLOCK_SIZE;
        assert!(offset % BLOCK_SIZE == 0);

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(cache) = option {
            cache.write().modify(0, |cache: &mut [u8; BLOCK_SIZE]| {
                cache.copy_from_slice(&buffer);
            })
        } else {
            self.device.write_blocks(&buffer, offset, 1).unwrap();
        }
    }
}

/// To Iterate Dir
#[derive(Clone)]
pub struct DirIter<'a> {
    device: Arc<dyn BlockDevice>,
    fat: ClusterChain,
    bpb: &'a BIOSParameterBlock,
    cluster_offset: usize,
    sector_id_in_cluster: usize,
    index_in_buf: usize,
    buffer: [u8; BLOCK_SIZE],
}

impl<'a> DirIter<'a> {
    pub(crate) fn new(
        device: Arc<dyn BlockDevice>,
        fat: ClusterChain,
        bpb: &BIOSParameterBlock,
    ) -> DirIter {
        // TODO 为什么要 next? 难道是 fat.current_cluster = 0? 是否需要 assert?
        let mut fat = fat;
        fat.next();

        DirIter {
            device: Arc::clone(&device),
            fat: fat.clone(),
            bpb,
            cluster_offset: bpb.offset(fat.current_cluster),
            sector_id_in_cluster: 0,
            index_in_buf: 0,
            buffer: [0; BLOCK_SIZE],
        }
    }

    fn sector_offset(&self) -> usize {
        self.cluster_offset + self.sector_id_in_cluster * BLOCK_SIZE
    }

    // next() 时更新 iter 的值
    fn val_next(&mut self) {
        let spc = self.bpb.sector_per_cluster_usize();

        self.index_in_buf += 32;
        if self.index_in_buf % BLOCK_SIZE == 0 {
            self.sector_id_in_cluster += 1;
            self.index_in_buf = 0;
        }

        if self.sector_id_in_cluster % spc == 0 && self.sector_id_in_cluster != 0 {
            if self.fat.next_is_none() {
                self.sector_id_in_cluster = spc;
            } else {
                self.fat.next();
                self.cluster_offset = self.bpb.offset(self.fat.current_cluster);
                self.sector_id_in_cluster = 0;
            }
        }
    }

    fn is_end_sector(&self) -> bool {
        let spc = self.bpb.sector_per_cluster_usize();
        self.sector_id_in_cluster == spc
    }

    fn is_end(&self) -> bool {
        self.is_end_sector() || self.buffer[self.index_in_buf] == 0x00
    }

    fn is_special_item(&self) -> bool {
        // '.' or '..'
        (self.buffer[self.index_in_buf] == DOT && self.buffer[self.index_in_buf + 1] == SPACE)
            || (self.buffer[self.index_in_buf] == DOT
                && self.buffer[self.index_in_buf + 1] == DOT
                && self.buffer[self.index_in_buf + 2] == SPACE)
    }

    fn get_part_buf(&mut self) -> &[u8] {
        &self.buffer[self.index_in_buf..self.index_in_buf + 32]
    }

    fn set_deleted(&mut self) {
        self.buffer[self.index_in_buf] = DIR_ENTRY_UNUSED;
    }

    pub(crate) fn update_item(&mut self, buf: &[u8]) {
        // append cluster if is dir end
        if self.is_end_sector() {
            let blank_cluster = self.fat.blank_cluster();
            self.clean_new_cluster_data(blank_cluster);
            // fat.current_cluster != 0
            self.fat.write(blank_cluster, END_OF_CLUSTER);
            self.fat.write(self.fat.current_cluster, blank_cluster);

            // TODO 错误处理
            assert_ne!(self.fat.current_cluster, 0);
            // 两次 next() 修复 fat.previous_cluster
            let _ = self.fat.to_previous();
            self.fat.next();
            self.fat.next();

            self.cluster_offset = self.bpb.offset(blank_cluster);
            self.index_in_buf = 0;
            self.sector_id_in_cluster = 0;
            self.update_buffer();
        }
        self.buffer[self.index_in_buf..self.index_in_buf + 32].copy_from_slice(buf);
    }

    // TODO 可以确保一定有 previous 吗?
    pub(crate) fn to_previous(&mut self) {
        if self.index_in_buf == 0 && self.sector_id_in_cluster != 0 {
            self.index_in_buf = BLOCK_SIZE - 32;
            self.sector_id_in_cluster -= 1;
            self.update_buffer();
        } else if self.index_in_buf != 0 {
            self.index_in_buf -= 32;
        } else {
            // TODO 可以确保一定有 previous 吗?
            // self.sector_id_in_cluster == 0

            // TODO 错误处理 (断言有 previous)
            assert_ne!(self.fat.current_cluster, 0);

            let spc = self.bpb.sector_per_cluster_usize();
            self.sector_id_in_cluster = spc - 1;
            self.index_in_buf = BLOCK_SIZE - 32;

            // assert_ne!(self.fat.current_cluster, 0);

            let _ = self.fat.to_previous();
            self.update_buffer();
        }
    }

    pub(crate) fn update_buffer(&mut self) {
        let offset = self.sector_offset();
        let block_id = offset / BLOCK_SIZE;
        assert!(offset % BLOCK_SIZE == 0);

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(cache) = option {
            cache.read().read(0, |cache: &[u8; BLOCK_SIZE]| {
                self.buffer.copy_from_slice(cache);
            })
        } else {
            self.device
                .read_blocks(&mut self.buffer, offset, 1)
                .unwrap();
        }
    }

    pub(crate) fn update_in_disk(&self) {
        let block_id = self.sector_offset() / BLOCK_SIZE;
        assert!(self.sector_offset() % BLOCK_SIZE == 0);

        let option = get_block_cache(block_id, Arc::clone(&self.device));
        if let Some(cache) = option {
            cache.write().modify(0, |cache: &mut [u8; BLOCK_SIZE]| {
                cache.copy_from_slice(&self.buffer);
            })
        } else {
            self.device
                .write_blocks(&self.buffer, self.sector_offset(), 1)
                .unwrap();
        }
    }

    fn clean_new_cluster_data(&self, cluster: u32) {
        let spc = self.bpb.sector_per_cluster_usize();
        for i in 0..spc {
            let offset = self.bpb.offset(cluster) + i * BLOCK_SIZE;
            let block_id = offset / BLOCK_SIZE;
            assert!(offset % BLOCK_SIZE == 0);

            let option = get_block_cache(block_id, Arc::clone(&self.device));
            if let Some(cache) = option {
                cache.write().modify(0, |cache: &mut [u8; BLOCK_SIZE]| {
                    cache.copy_from_slice(&[0; BLOCK_SIZE]);
                })
            } else {
                self.device
                    .write_blocks(&[0; BLOCK_SIZE], offset, 1)
                    .unwrap();
            }
        }
    }
}

/// Implement Iterator For DirIter
impl<'a> Iterator for DirIter<'a> {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index_in_buf == 0 {
            // index_in_buf == 0 仅在 new() , is_end_sector(), val_next() 时
            self.update_buffer();
        }

        if self.is_end() {
            return None;
        };

        if self.is_special_item() {
            self.val_next();
            self.next()
        } else {
            let buf = self.get_part_buf();
            let di = Entry::from_buf(buf);
            self.val_next();
            Some(di)
        }
    }
}

// TODO 长文件名转短文件名
// TODO 短文件名转长文件名
// TODO 修改文件名
