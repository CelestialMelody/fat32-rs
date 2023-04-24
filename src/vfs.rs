use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use super::cache::{get_block_cache, BlockCache, Cache};
use super::device::BlockDevice;
use super::entry::{LongDirEntry, ShortDirEntry};
use super::fat::ClusterChain;
use super::fs::FileSystem;
use super::{
    generate_short_name, long_name_split, short_name_format, split_name_ext, VirFileType,
    ATTR_DIRECTORY, ATTR_LONG_NAME, BLOCK_SIZE, DIRENT_SIZE, DIR_ENTRY_UNUSED, END_OF_CLUSTER,
    LAST_LONG_ENTRY, NEW_VIR_FILE_CLUSTER, ORIGINAL, ROOT_DIR_ENTRY_CLUSTER, STRAT_CLUSTER_IN_FAT,
};

#[derive(Clone)]
pub struct VirFile {
    name: String,
    sde_pos: DirEntryPos,
    lde_pos: Vec<DirEntryPos>,
    fs: Arc<RwLock<FileSystem>>,
    device: Arc<dyn BlockDevice>,
    cluster_chain: Arc<RwLock<ClusterChain>>,
    attr: VirFileType,
}

pub fn root(fs: Arc<RwLock<FileSystem>>, device: Arc<dyn BlockDevice>) -> VirFile {
    let fs = Arc::clone(&fs);
    let device = Arc::clone(&device);

    let cluster_chain = Arc::new(RwLock::new(ClusterChain::new(
        STRAT_CLUSTER_IN_FAT,
        Arc::clone(&device),
        fs.read().bpb.fat1_offset(),
    )));

    // fix: set root next cluster
    fs.write()
        .fat
        .write()
        .set_next_cluster(STRAT_CLUSTER_IN_FAT, END_OF_CLUSTER);

    VirFile::new(
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
    )
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
            .unwrap(); // assert offset < file_size()

        // let offset_in_disk = self.fs.read().bpb.offset(cluster);
        // let cluster_id = offset_in_disk / cluster_size;
        // Some((cluster_id, offset_in_cluster))

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

        let file_size = self.file_size();

        let mut index = offset;
        let end = (offset + buf.len()).min(file_size);
        // fix: > not >= (new file offset == file_size == 0)
        if offset > file_size || buf.len() == 0 {
            return 0;
        }
        let pre_cluster_cnt = offset / cluster_size;
        let mut curr_cluster = self.first_cluster() as u32;

        for _ in 0..pre_cluster_cnt {
            curr_cluster = self
                .fs
                .read()
                .fat
                .read()
                .get_next_cluster(curr_cluster)
                .unwrap();
        }

        let mut left = pre_cluster_cnt * cluster_size;
        let mut right = left + BLOCK_SIZE;
        let mut already_read = 0;

        while index < end {
            let cluster_offset_in_disk = self.fs.read().bpb.offset(curr_cluster);
            let start_block_id = cluster_offset_in_disk / BLOCK_SIZE;

            // fix: code pos of offset_in_block and len (may overflow)
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
                        // fix: else pos
                        let mut cache = [0u8; BLOCK_SIZE];
                        self.device
                            .read_blocks(&mut cache, block_id * BLOCK_SIZE, 1)
                            .unwrap();
                        let dst = &mut buf[already_read..already_read + len];
                        let src = &cache[offset_in_block..offset_in_block + len];
                        dst.copy_from_slice(src);
                    }

                    // fix: add
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
        }

        already_read
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let spc = self.fs.read().bpb.sectors_per_cluster();
        let cluster_size = self.fs.read().cluster_size();

        // fix
        if buf.len() == 0 {
            return 0;
        }

        let mut index = offset;
        // fix: end
        let end = offset + buf.len();

        let new_size = offset + buf.len();

        // TODO
        // self.modify_size(new_size);
        self.incerase_size(new_size);

        let cluster_len = self
            .fs
            .read()
            .fat
            .read()
            .cluster_chain_len(self.first_cluster() as u32);

        let pre_cluster_cnt = offset / cluster_size;

        let mut curr_cluster = self.first_cluster() as u32;
        for _ in 0..pre_cluster_cnt {
            curr_cluster = self
                .fs
                .read()
                .fat
                .read()
                .get_next_cluster(curr_cluster)
                .unwrap();
        }

        let mut left = pre_cluster_cnt * cluster_size;
        let mut right = left + BLOCK_SIZE;
        let mut already_write = 0;

        while index < end {
            let cluster_offset_in_disk = self.fs.read().bpb.offset(curr_cluster);
            let start_block_id = cluster_offset_in_disk / BLOCK_SIZE;

            for block_id in start_block_id..start_block_id + spc {
                if index >= left && index < right && index < end {
                    // fix: pos of offset_in_block and len (may overflow)
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

                    // fix: add
                    if index >= end {
                        break;
                    }
                }

                // fix: pos of index and already_write
                left += BLOCK_SIZE;
                right += BLOCK_SIZE;
            }

            // fix: add
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

        // fix: dead lock (if put this in expr 'if let Some' directly)
        let option = self
            .fs
            .write()
            .alloc_cluster(need_cluster_cnt, first_cluster);

        if let Some(start_cluster) = option {
            if first_cluster == NEW_VIR_FILE_CLUSTER || first_cluster == ROOT_DIR_ENTRY_CLUSTER {
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
        }
    }

    // Dir Functions
    fn find_by_lfn(&self, name: &str) -> Option<VirFile> {
        let name_vec = long_name_split(name);
        let name_cnt = name_vec.len();
        //  在目录文件中的偏移
        let mut index = 0;
        let mut lde = LongDirEntry::empty();
        let mut lde_pos_vec: Vec<DirEntryPos> = Vec::new();
        let name_last = name_vec[name_cnt - 1].clone();
        let dir_size = self.file_size();

        loop {
            if (index + DIRENT_SIZE) > dir_size {
                return None;
            }
            let mut read_size = self.read_at(index, lde.as_bytes_mut());
            if read_size != DIRENT_SIZE || lde.is_free() {
                return None;
            }

            // 先匹配最后一个长文件名目录项，即长文件名的最后一块
            if lde.attr() == ATTR_LONG_NAME // 防止为短文件名
            && lde.name_utf16() == name_last
            {
                let mut order = lde.order();
                if order & LAST_LONG_ENTRY == 0 || order == DIR_ENTRY_UNUSED {
                    index += DIRENT_SIZE;
                    continue;
                }
                // 恢复 order为正确的次序值
                order = order ^ LAST_LONG_ENTRY;
                // 如果长文件名目录项数量对不上，则跳过继续搜索
                if order as usize != name_cnt {
                    index += DIRENT_SIZE;
                    continue;
                }
                // 如果order匹配通过，开一个循环继续匹配长名目录项
                let mut is_match = true;
                for i in 1..order as usize {
                    read_size = self.read_at(index + i * DIRENT_SIZE, lde.as_bytes_mut());
                    if read_size != DIRENT_SIZE {
                        return None;
                    }
                    // 匹配前一个名字段，如果失败就退出
                    if lde.name_utf16() != name_vec[name_cnt - 1 - i]
                        || lde.attr() != ATTR_LONG_NAME
                    {
                        is_match = false;
                        break;
                    }
                }
                if is_match {
                    // 如果成功，读短目录项，进行校验
                    let checksum = lde.check_sum();
                    let mut sde = ShortDirEntry::empty();
                    let sde_offset = index + name_cnt * DIRENT_SIZE;
                    read_size = self.read_at(sde_offset, sde.as_bytes_mut());
                    if read_size != DIRENT_SIZE {
                        return None;
                    }
                    if !sde.is_deleted() && checksum == sde.gen_check_sum() {
                        assert!(sde_offset <= self.file_size());
                        let sde_pos = self.dir_entry_pos(sde_offset).unwrap();
                        for i in 0..order as usize {
                            // 存入长名目录项位置了，第一个在栈顶
                            let lde_pos = self.dir_entry_pos(index + i * DIRENT_SIZE);
                            lde_pos_vec.push(lde_pos.unwrap());
                        }
                        let file_type = if sde.attr() == ATTR_DIRECTORY {
                            VirFileType::Dir
                        } else {
                            VirFileType::File
                        };
                        return Some(VirFile::new(
                            String::from(name),
                            sde_pos,
                            lde_pos_vec,
                            Arc::clone(&self.fs),
                            Arc::clone(&self.device),
                            Arc::clone(&self.cluster_chain),
                            file_type,
                        ));
                    }
                }
            }
            index += DIRENT_SIZE;
        }
    }

    pub fn find_by_sfn(&self, name: &str) -> Option<VirFile> {
        let name = name.to_ascii_uppercase();

        let mut sde = ShortDirEntry::empty();
        let mut index = 0;
        let dir_size = self.file_size();

        loop {
            // fix
            if index > dir_size {
                return None;
            }

            let read_size = self.read_at(index, sde.as_bytes_mut());

            // fix: do not sde.is_free() of sde.is_deleted()
            if read_size != DIRENT_SIZE {
                return None;
            } else {
                // 判断名字是否一样
                if !sde.is_deleted() && name == sde.get_name_uppercase() {
                    assert!(index <= self.file_size());
                    let sde_pos = self.dir_entry_pos(index).unwrap();
                    let lde_pos_vec: Vec<DirEntryPos> = Vec::new();
                    let file_type = if sde.attr() == ATTR_DIRECTORY {
                        VirFileType::Dir
                    } else {
                        VirFileType::File
                    };
                    return Some(VirFile::new(
                        String::from(name),
                        sde_pos,
                        lde_pos_vec,
                        Arc::clone(&self.fs),
                        Arc::clone(&self.device),
                        Arc::clone(&self.cluster_chain),
                        file_type,
                    ));
                } else {
                    index += DIRENT_SIZE;
                    continue;
                }
            }
        }
    }

    fn find_by_name(&self, name: &str) -> Option<VirFile> {
        // 不是目录则退出
        assert!(self.is_dir());
        let (name_, ext_) = split_name_ext(name);
        // TODO self 为父级目录
        if name_.len() > 8 || ext_.len() > 3 {
            //长文件名
            return self.find_by_lfn(name);
        } else {
            // 短文件名
            return self.find_by_sfn(name);
        }
    }

    /// 根据路径递归搜索文件
    // TODO 是否需要 Arc
    pub fn find_by_path(&self, path: Vec<&str>) -> Option<VirFile> {
        let len = path.len();
        if len == 0 {
            return None;
        }
        let mut current = self.clone();
        for i in 0..len {
            if path[i] == "" || path[i] == "." {
                continue;
            }
            if let Some(vfile) = current.find_by_name(path[i]) {
                current = vfile;
            } else {
                return None;
            }
        }
        Some(current)
    }

    // 查找可用目录项，返回offset，簇不够也会返回相应的offset，caller需要及时分配
    // TODO
    fn empty_entry_index(&self) -> Option<usize> {
        if !self.is_dir() {
            return None;
        }
        let mut sde = ShortDirEntry::empty();
        let mut index = 0;
        loop {
            let read_size = self.read_at(index, sde.as_bytes_mut());
            // TODO 对于删除后的目录项移动管理：建议实现 drop 时进行整理
            if read_size == 0 // 读到目录文件末尾 -> 超过 dir_size, 需要分配新簇 -> write_at 中处理 -> increase_size
            || sde.is_empty()
            {
                return Some(index);
            } else {
                index += DIRENT_SIZE;
            }
        }
    }

    pub fn vir_file_type(&self) -> VirFileType {
        if self.is_dir() {
            VirFileType::Dir
        } else {
            VirFileType::File
        }
    }

    // Dir Functions
    pub fn create(&self, name: &str, file_type: VirFileType) -> Option<VirFile> {
        // 检测同名文件
        assert!(self.is_dir());
        // fix: add
        let option = self.find_by_name(name);
        if let Some(file) = option {
            if file.vir_file_type() == file_type {
                // 改 result 处理
                return None;
            }
        }
        let (name_, ext_) = split_name_ext(name);
        // 搜索空处
        let mut entry_offset: usize;
        if let Some(offset) = self.empty_entry_index() {
            entry_offset = offset;
        } else {
            return None;
        }

        // low -> high
        // lfn(n) -> lfn(n-1) -> .. -> lfn(1) -> sfn
        let mut sde: ShortDirEntry;
        if name_.len() > 8 || ext_.len() > 3 {
            // 长文件名
            // 生成短文件名及对应目录项
            let short_name = generate_short_name(name);
            let (_name, _ext) = short_name_format(short_name.as_str());
            sde = ShortDirEntry::new(NEW_VIR_FILE_CLUSTER, &_name, &_ext, file_type);

            // 长文件名拆分
            let mut lfn_vec = long_name_split(name);
            // 需要创建的长文件名目录项个数
            let lfn_cnt = lfn_vec.len();

            // 逐个写入长名目录项
            for i in 0..lfn_cnt {
                // 按倒序填充长文件名目录项，目的是为了避免名字混淆
                let mut order: u8 = (lfn_cnt - i) as u8;
                if i == 0 {
                    // 最后一个长文件名目录项，将该目录项的序号与 0x40 进行或运算然后写入
                    order |= 0x40;
                }
                // 初始化长文件名目录项
                let lde = LongDirEntry::new_form_name_slice(
                    order,
                    lfn_vec.pop().unwrap(),
                    // TODO 统一 generate_checksum
                    sde.gen_check_sum(),
                );
                // 写入长文件名目录项
                let write_size = self.write_at(entry_offset, lde.as_bytes());
                assert_eq!(write_size, DIRENT_SIZE);
                // 更新写入位置
                entry_offset += DIRENT_SIZE;
            }
        } else {
            // 短文件名
            let (_name, _ext) = short_name_format(name);
            sde = ShortDirEntry::new(NEW_VIR_FILE_CLUSTER, &_name, &_ext, file_type);
            sde.set_name_case(ORIGINAL);
        }

        // 写短目录项（长文件名也是有短文件名目录项的）
        let wirte_size = self.write_at(entry_offset, sde.as_bytes());
        assert_eq!(wirte_size, DIRENT_SIZE);

        // 验证
        if let Some(file) = self.find_by_name(name) {
            // 如果是目录类型，需要创建.和..
            if file_type == VirFileType::Dir {
                // 先写入 .. 使得目录获取第一个簇
                let (_name, _ext) = short_name_format("..");
                let mut parent_sde = ShortDirEntry::new(
                    self.first_cluster() as u32,
                    &_name,
                    &_ext,
                    VirFileType::Dir,
                );

                // fix: 注意文件大小的更新, 否则返回上级目录没法读
                let parent_file_size = self.file_size();
                parent_sde.set_file_size(parent_file_size as u32);
                file.write_at(DIRENT_SIZE, parent_sde.as_bytes_mut());

                let (_name, _ext) = short_name_format(".");
                let mut self_sde = ShortDirEntry::new(
                    file.first_cluster() as u32, // 先写入 .. 使得目录获取第一个簇, 否则 first_cluster 为 0
                    &_name,
                    &_ext,
                    VirFileType::Dir,
                );
                file.write_at(0, self_sde.as_bytes_mut());
            }
            return Some(file);
        } else {
            None
        }
    }

    // 返回二元组，第一个是文件名，第二个是文件属性（文件或者目录）
    // TODO 使用 dir_entry_info 方法
    pub fn ls(&self) -> Option<Vec<(String, u8)>> {
        if !self.is_dir() {
            return None;
        }
        let mut list: Vec<(String, u8)> = Vec::new();
        let mut entry = LongDirEntry::empty();
        let mut offset = 0usize;
        loop {
            let read_size = self.read_at(offset, entry.as_bytes_mut());
            // 读取完了
            if read_size != DIRENT_SIZE || entry.is_empty() {
                return Some(list);
            }
            // 文件被标记删除则跳过
            if entry.is_deleted() {
                offset += DIRENT_SIZE;
                continue;
            }
            // TODO 注意：Linux中文件创建都会创建一个长文件名目录项，用于处理文件大小写问题
            if entry.attr() != ATTR_LONG_NAME {
                // 短文件名
                let sde: ShortDirEntry = unsafe { core::mem::transmute(entry) };
                list.push((sde.get_name_lowercase(), sde.attr()));
            } else {
                // 长文件名
                // 如果是长文件名目录项，则必是长文件名最后的那一段
                let mut name = String::new();
                let order = entry.order() ^ LAST_LONG_ENTRY;
                for _ in 0..order {
                    name.insert_str(0, &entry.name().as_str());
                    offset += DIRENT_SIZE;
                    let read_size = self.read_at(offset, entry.as_bytes_mut());
                    if read_size != DIRENT_SIZE || entry.is_empty() {
                        panic!("ls read long name entry error!");
                    }
                }
                list.push((name.clone(), entry.attr()));
            }
            offset += DIRENT_SIZE;
        }
    }

    // 删除自身
    pub fn clear(&self) {
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
        self.fs.write().dealloc_cluster(all_clusters);
    }

    pub fn delete_by_path(&self, path: Vec<&str>) {
        if let Some(file) = self.find_by_path(path) {
            file.clear();
        } else {
            panic!("delete_by_path error!");
        }
    }

    /// 返回：(st_size, st_blksize, st_blocks, is_dir, time)
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
}
