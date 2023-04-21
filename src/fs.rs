use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::{self, Debug, Formatter};
use spin::RwLock;

use crate::bpb::{BIOSParameterBlock, FSInfo};
use crate::cache::{get_block_cache, Cache};
use crate::device::BlockDevice;
use crate::entry::{Entry, EntryType, ShortDirEntry};
use crate::fat::FATManager;
use crate::VirFileType;
use crate::{BLOCK_SIZE, FREE_CLUSTER, ROOT, STRAT_CLUSTER_IN_FAT};

pub struct FileSystem {
    device: Arc<dyn BlockDevice>,
    free_cluster_cnt: Arc<RwLock<usize>>, // TODO Arc needed?
    bpb: BIOSParameterBlock,              // read only
    fat: Arc<RwLock<FATManager>>,
}

impl FileSystem {
    pub fn sector_pre_cluster(&self) -> usize {
        self.bpb.sector_per_cluster()
    }

    pub fn sector_size(&self) -> usize {
        self.bpb.bytes_per_sector()
    }

    pub fn cluster_size(&self) -> usize {
        self.bpb.bytes_per_sector() * self.bpb.sector_per_cluster()
    }

    pub fn first_data_sector(&self) -> usize {
        self.bpb.first_data_sector()
    }

    pub fn free_cluster_cnt(&self) -> usize {
        *self.free_cluster_cnt.read()
    }

    pub fn set_free_clusters(&self, cnt: usize) {
        let option = get_block_cache(self.bpb.fat_info_sector(), Arc::clone(&self.device));
        if let Some(block) = option {
            block.write().modify(0, |fsinfo: &mut FSInfo| {
                fsinfo.set_free_clusters(cnt as u32)
            });
        }
        *self.free_cluster_cnt.write() = cnt;
    }

    pub fn first_sector_of_cluster(&self, cluster: u32) -> usize {
        self.bpb.first_sector_of_cluster(cluster)
    }

    pub fn cluster_offset(&self, cluster: u32) -> usize {
        self.bpb.offset(cluster)
    }

    pub fn root_sector_id(&self) -> usize {
        self.first_data_sector()
    }

    pub fn root_entry(&self) -> Entry {
        let mut name_bytes = [0x20u8; 11];
        name_bytes[0] = ROOT;
        let root =
            ShortDirEntry::new_from_name_bytes(STRAT_CLUSTER_IN_FAT, &name_bytes, VirFileType::Dir);
        Entry {
            item_type: EntryType::Dir,
            sde: Some(root),
            lde: None,
        }
    }

    pub fn open(device: Arc<dyn BlockDevice>) -> Arc<RwLock<Self>> {
        let bpb = get_block_cache(0, Arc::clone(&device))
            .unwrap()
            .read()
            .read(0, |bpb: &BIOSParameterBlock| *bpb);

        let free_cluster_cnt = get_block_cache(bpb.fat_info_sector(), Arc::clone(&device))
            .unwrap()
            .read()
            .read(0, |fsinfo: &FSInfo| {
                assert!(
                    fsinfo.check_signature(),
                    "Error loading fat32! Illegal signature"
                );
                fsinfo.free_cluster_cnt() as usize
            });

        let fat = FATManager::new(bpb.fat1_offset(), Arc::clone(&device));

        Arc::new(RwLock::new(Self {
            device,
            free_cluster_cnt: Arc::new(RwLock::new(free_cluster_cnt)),
            bpb,
            fat: Arc::new(RwLock::new(fat)),
        }))
    }

    fn clear_cluster(&self, cluster: u32) {
        let block_id = self.first_sector_of_cluster(cluster);
        for i in 0..self.sector_pre_cluster() {
            let option = get_block_cache(block_id + i, Arc::clone(&self.device));
            if let Some(block) = option {
                block.write().modify(0, |cache: &mut [u8; BLOCK_SIZE]| {
                    cache.copy_from_slice(&[0u8; BLOCK_SIZE])
                })
            } else {
                // TODO
                self.device
                    .write_blocks(&[0u8; BLOCK_SIZE], (block_id + i) * BLOCK_SIZE, 1)
                    .unwrap();
            }
        }
    }

    // 成功返回第一个簇号，失败返回None
    pub fn alloc_cluster(&self, num: usize) -> Option<usize> {
        let free_cluster_cnt = self.free_cluster_cnt();
        if free_cluster_cnt < num {
            return None;
        }

        let first_cluster_id = self.fat.write().blank_cluster();
        self.clear_cluster(first_cluster_id);
        assert!(first_cluster_id >= 2);

        let mut curr_cluster_id = first_cluster_id;
        for _ in 1..num {
            let cluster_id = self.fat.write().blank_cluster();
            self.clear_cluster(cluster_id);
            assert!(cluster_id >= 2);

            self.fat
                .write()
                .set_next_cluster(curr_cluster_id, cluster_id);

            curr_cluster_id = cluster_id;
        }

        // TODO 是否维护 fsinfo next_free_cluster
        self.clear_cluster(curr_cluster_id);
        self.fat
            .write()
            .set_next_cluster(curr_cluster_id, 0x0FFFFFFF);
        self.set_free_clusters(free_cluster_cnt - num);
        Some(first_cluster_id as usize)
    }

    pub fn dealloc_cluster(&self, clusters: Vec<u32>) {
        let num = clusters.len();
        if num == 0 {
            return;
        }
        let free_cluster_cnt = self.free_cluster_cnt();
        for i in 0..num {
            self.fat.write().set_next_cluster(clusters[i], FREE_CLUSTER);
        }
        self.set_free_clusters(free_cluster_cnt + num);
    }

    pub fn count_needed_clusters(&self, new_size: usize, start_cluster: u32) -> usize {
        let old_cluster_cnt = self.fat.read().cluster_chain_len(start_cluster) as usize;
        let cluster_cnt = (new_size + self.cluster_size() - 1) / self.cluster_size();
        if cluster_cnt > old_cluster_cnt {
            cluster_cnt - old_cluster_cnt
        } else {
            0
        }
    }
}
