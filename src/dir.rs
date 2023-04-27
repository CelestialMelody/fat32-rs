use super::entry::{LongDirEntry, ShortDirEntry};
use super::vfs::{DirEntryPos, VirFile};
use super::VirFileType;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use super::{generate_short_name, long_name_split, short_name_format, split_name_ext};

use super::{
    ATTR_DIRECTORY, ATTR_LONG_NAME, DIRENT_SIZE, DIR_ENTRY_UNUSED, LAST_LONG_ENTRY,
    NEW_VIR_FILE_CLUSTER, ORIGINAL,
};

#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum DirError {
    NoMatchDir,
    NoMatchFile,
    NoMatch,
    IllegalChar,
    DirHasExist,
    FileHasExist,
    NotDir,
    ListLFNIllegal,
    CreateFileError,
    MissingName,
}

pub trait Dir {
    fn find(&self, path: Vec<&str>) -> Result<VirFile, DirError>;

    fn create(&self, name: &str, file_type: VirFileType) -> Result<VirFile, DirError>;

    fn ls(&self) -> Result<Vec<String>, DirError>;

    fn remove(&self, path: Vec<&str>) -> Result<(), DirError>;
}

impl Dir for VirFile {
    /// 根据路径递归搜索文件
    // TODO 是否需要 Arc
    fn find(&self, path: Vec<&str>) -> Result<VirFile, DirError> {
        let len = path.len();
        if len == 0 {
            return Err(DirError::MissingName);
        }
        let mut current = self.clone();
        for i in 0..len {
            if path[i] == "" || path[i] == "." {
                continue;
            }
            if let Some(vfile) = current.find_by_name(path[i]) {
                current = vfile;
            } else {
                return Err(DirError::NoMatch);
            }
        }
        Ok(current)
    }

    fn remove(&self, path: Vec<&str>) -> Result<(), DirError> {
        match self.find(path) {
            Ok(file) => {
                file.clear();
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn ls(&self) -> Result<Vec<String>, DirError> {
        match self.ls_with_attr() {
            Ok(v) => {
                let mut name = Vec::new();
                for i in v {
                    name.push(i.0);
                }
                Ok(name)
            }
            Err(e) => Err(e),
        }
    }

    // Dir Functions
    fn create(&self, name: &str, file_type: VirFileType) -> Result<VirFile, DirError> {
        // 检测同名文件
        assert!(self.is_dir());
        // fix: add
        let option = self.find_by_name(name);
        if let Some(file) = option {
            if file.vir_file_type() == file_type {
                // TODO 改 result 处理
                return Err(DirError::FileHasExist);
            }
        }
        let (name_, ext_) = split_name_ext(name);
        // 搜索空处
        let mut entry_offset: usize;

        match self.empty_entry_index() {
            Ok(offset) => {
                entry_offset = offset;
            }
            Err(e) => {
                return Err(e);
            }
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
                // 按倒序填充长文件名目录项, 目的是为了避免名字混淆
                let mut order: u8 = (lfn_cnt - i) as u8;
                if i == 0 {
                    // 最后一个长文件名目录项, 将该目录项的序号与 0x40 进行或运算然后写入
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

        // 写短目录项(长文件名也是有短文件名目录项的)
        let wirte_size = self.write_at(entry_offset, sde.as_bytes());
        assert_eq!(wirte_size, DIRENT_SIZE);

        // 验证
        if let Some(file) = self.find_by_name(name) {
            // 如果是目录类型, 需要创建.和..
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
            Ok(file)
        } else {
            Err(DirError::CreateFileError)
        }
    }
}

impl VirFile {
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

        // println!("[find_by_lfn] name: {}", name);

        loop {
            // fix
            if index > dir_size {
                return None;
            }

            let mut read_size = self.read_at(index, lde.as_bytes_mut());
            // fix
            if read_size != DIRENT_SIZE {
                return None;
            }

            // 先匹配最后一个长文件名目录项, 即长文件名的最后一块
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
                // 如果长文件名目录项数量对不上, 则跳过继续搜索
                if order as usize != name_cnt {
                    index += DIRENT_SIZE;
                    continue;
                }
                // 如果order匹配通过, 开一个循环继续匹配长名目录项
                let mut is_match = true;
                for i in 1..order as usize {
                    read_size = self.read_at(index + i * DIRENT_SIZE, lde.as_bytes_mut());
                    if read_size != DIRENT_SIZE {
                        return None;
                    }
                    // 匹配前一个名字段, 如果失败就退出
                    if lde.name_utf16() != name_vec[name_cnt - 1 - i]
                        || lde.attr() != ATTR_LONG_NAME
                    {
                        is_match = false;
                        break;
                    }
                }
                if is_match {
                    // 如果成功, 读短目录项, 进行校验
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
                            // 存入长名目录项位置了, 第一个在栈顶
                            let lde_pos = self.dir_entry_pos(index + i * DIRENT_SIZE);
                            lde_pos_vec.push(lde_pos.unwrap());
                        }
                        let file_type = if sde.attr() == ATTR_DIRECTORY {
                            VirFileType::Dir
                        } else {
                            VirFileType::File
                        };

                        // fix clus
                        let clus_chain = self.file_cluster_chain(index);

                        return Some(VirFile::new(
                            String::from(name),
                            sde_pos,
                            lde_pos_vec,
                            Arc::clone(&self.fs),
                            Arc::clone(&self.device),
                            Arc::new(RwLock::new(clus_chain)),
                            file_type,
                        ));
                    }
                }
            }
            index += DIRENT_SIZE;
        }
    }

    fn find_by_sfn(&self, name: &str) -> Option<VirFile> {
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
            }

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

                // fix clus
                let clus_chain = self.file_cluster_chain(index);

                return Some(VirFile::new(
                    String::from(name),
                    sde_pos,
                    lde_pos_vec,
                    Arc::clone(&self.fs),
                    Arc::clone(&self.device),
                    Arc::new(RwLock::new(clus_chain)),
                    file_type,
                ));
            } else {
                index += DIRENT_SIZE;
                continue;
            }
        }
    }

    pub fn find_by_name(&self, name: &str) -> Option<VirFile> {
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

    // 查找可用目录项, 返回offset, 簇不够也会返回相应的offset
    fn empty_entry_index(&self) -> Result<usize, DirError> {
        if !self.is_dir() {
            return Err(DirError::NotDir);
        }
        let mut sde = ShortDirEntry::empty();
        let mut index = 0;
        loop {
            let read_size = self.read_at(index, sde.as_bytes_mut());
            // TODO 对于删除后的目录项移动管理: 建议实现 drop 时进行整理
            if read_size == 0 // 读到目录文件末尾 -> 超过 dir_size, 需要分配新簇 -> write_at 中处理 -> increase_size
            || sde.is_empty()
            {
                return Ok(index);
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

    // 返回二元组, 第一个是文件名, 第二个是文件属性(文件或者目录)
    // TODO 使用 dir_entry_info 方法
    pub fn ls_with_attr(&self) -> Result<Vec<(String, u8)>, DirError> {
        if !self.is_dir() {
            return Err(DirError::NotDir);
        }
        let mut list: Vec<(String, u8)> = Vec::new();
        let mut entry = LongDirEntry::empty();
        let mut offset = 0usize;
        loop {
            let read_size = self.read_at(offset, entry.as_bytes_mut());
            // 读取完了
            if read_size != DIRENT_SIZE || entry.is_empty() {
                return Ok(list);
            }
            // 文件被标记删除则跳过
            if entry.is_deleted() {
                offset += DIRENT_SIZE;
                continue;
            }
            // TODO 注意: Linux中文件创建都会创建一个长文件名目录项, 用于处理文件大小写问题
            if entry.attr() != ATTR_LONG_NAME {
                // 短文件名
                let sde: ShortDirEntry = unsafe { core::mem::transmute(entry) };
                list.push((sde.get_name_lowercase(), sde.attr()));
            } else {
                // 长文件名
                // 如果是长文件名目录项, 则必是长文件名最后的那一段
                let mut name = String::new();
                let order = entry.order() ^ LAST_LONG_ENTRY;
                for _ in 0..order {
                    name.insert_str(0, &entry.name().as_str());
                    offset += DIRENT_SIZE;
                    let read_size = self.read_at(offset, entry.as_bytes_mut());
                    if read_size != DIRENT_SIZE || entry.is_empty() {
                        return Err(DirError::ListLFNIllegal);
                    }
                }
                list.push((name.clone(), entry.attr()));
            }
            offset += DIRENT_SIZE;
        }
    }
}
