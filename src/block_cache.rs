use crate::{block_device::BlockDevice, BLOCK_CACHE_LIMIT};

use alloc::sync::Arc;
// use core::num::NonZeroUsize;
use lazy_static::*;
use lru::LruCache;
use spin::RwLock;

use crate::BLOCK_SIZE;

pub trait Cache {
    /// The read-only mapper to the block cache
    ///
    /// - `offset`: offset in cache
    /// - `f`: a closure to read
    fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V;
    /// The mutable mapper to the block cache
    ///
    /// - `offset`: offset in cache
    /// - `f`: a closure to write
    fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V;
    /// Tell cache to write back
    ///
    /// - `block_ids`: block ids in this cache
    /// - `block_device`: The pointer to the block_device.
    fn sync(&mut self);
}
pub struct BlockCache {
    cache: [u8; BLOCK_SIZE],
    // the block id in the disk not in the cluster
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    modified: bool,
}

impl BlockCache {
    // load a block from the disk
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8; BLOCK_SIZE];
        block_device
            .read_blocks(&mut cache, block_id * BLOCK_SIZE, 1)
            .unwrap();
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
    }

    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SIZE);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SIZE);
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }
}

impl Cache for BlockCache {
    fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    // write the content back to disk
    fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device
                .write_blocks(&self.cache, self.block_id * BLOCK_SIZE, 1)
                .unwrap();
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}

pub struct BlockCacheManager {
    // TODO
    // 是否需要添加一个字段 物理起始块号
    lru: LruCache<usize, Arc<RwLock<BlockCache>>>,
}

impl BlockCacheManager {
    pub fn new() -> Self {
        Self {
            /// Creates a new LRU Cache that never automatically evicts items.
            //
            // 创建一个不会自动清理的lru_cache
            lru: LruCache::unbounded(),
        }
    }

    // get a block cache by block id
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Option<Arc<RwLock<BlockCache>>> {
        // if the block is already in lru_cache, just return the copy
        if let Some(pair) = self.lru.get(&block_id) {
            Some(Arc::clone(pair))
        } else {
            // 如果不在lru_cache中，就创建一个新的block_cache
            // 如果lru_cache已经满了，就把最久没有使用的block_cache写回磁盘(不过只有引用计数为 0 的时候才会 drop 写回磁盘)
            // TODO
            if self.lru.len() == BLOCK_CACHE_LIMIT {
                let (_, block_cache) = self.lru.peek_lru().unwrap();
                if Arc::strong_count(block_cache) == 1 {
                    self.lru.pop_lru();
                } else {
                    // 否则就返回 None, 让上层直接从磁盘读取
                    return None;
                }
            }
            // create a new block cache
            let block_cache = Arc::new(RwLock::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            // Add to the end of lru_cache and return
            self.lru.put(block_id, Arc::clone(&block_cache));
            // TODO 是不是不用 clone
            Some(block_cache)
        }
    }

    pub fn clear(&mut self) {
        // TODO
        // 是否需要考虑引用计数
        self.lru.clear();
    }
}

// create a block cache manager with 64 blocks
lazy_static! {
    pub static ref BLOCK_CACHE_MANAGER: RwLock<BlockCacheManager> =
        RwLock::new(BlockCacheManager::new());
}

// used for external modules
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Option<Arc<RwLock<BlockCache>>> {
    // TODO
    // 是否需要添加一个字段 物理起始块号 phy_blk_id = start_sec + block_id
    BLOCK_CACHE_MANAGER
        // TODO 区分 BLOCK_CACHE_MANAGER 的读写锁
        .write()
        .get_block_cache(block_id, block_device)
}

pub fn sync_all() {
    BLOCK_CACHE_MANAGER.write().clear();
}
