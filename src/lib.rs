// #![no_std]
pub mod bpb;
pub mod cache;
pub mod device;
pub mod dir;
pub mod entry;
pub mod fat;
pub mod file;
pub mod fs;
pub mod vfs;

use crate::dir::DirError;
use crate::entry::NameType;
use crate::fat::ClusterChainErr;
use crate::file::FileError;

use core::convert::TryInto;
use core::str;

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub const LEAD_SIGNATURE: u32 = 0x41615252;
pub const STRUCT_SIGNATURE: u32 = 0x61417272;
pub const TRAIL_SIGNATURE: u32 = 0xAA550000;

pub const FREE_CLUSTER: u32 = 0x00000000;
pub const END_CLUSTER: u32 = 0x0FFFFFF8;
pub const BAD_CLUSTER: u32 = 0x0FFFFFF7;
/// EOC: End of Cluster Chain
/// note that we still USE this cluster and this clsuter id is not EOC,
/// but in FAT table, the value of this cluster is EOC
///
/// A FAT32 FAT entry is actually only a 28-bit entry. The high 4 bits of a FAT32 FAT entry are reserved.
//
//  在创建新簇时将其在 FAT 表中的值设置为 EOC
//  这样在 next() 中也判断是否为 EOC
pub const END_OF_CLUSTER: u32 = 0x0FFFFFFF;

pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

pub const DIRENT_SIZE: usize = 32;
pub const LONG_NAME_LEN: u32 = 13;
pub const STRAT_CLUSTER_IN_FAT: u32 = 2;
pub const NEW_VIR_FILE_CLUSTER: u32 = 0;
// 持久化根目录的不得已行为; TODO 实际上只要能够知道根目录大小就行
pub const ROOT_DIR_ENTRY_CLUSTER: u32 = 3;
pub const BLOCK_CACHE_LIMIT: usize = 64;

// Name Status for Short Directory Entry
pub const ALL_UPPER_CASE: u8 = 0x00;
pub const ALL_LOWER_CASE: u8 = 0x08;
pub const ORIGINAL: u8 = 0x0F;

// Charactor
pub const SPACE: u8 = 0x20;
pub const DOT: u8 = 0x2E;
pub const ROOT: u8 = 0x2F;

// Just for test
pub const BLOCK_NUM: u32 = 0x4000;

/// BPB Bytes Per Sector
pub const BLOCK_SIZE: usize = 512;
pub const CACHE_SIZE: usize = 512;
pub const FAT_BUFFER_SIZE: usize = 512;
pub const DIR_BUFFER_SIZE: usize = 512;
pub const FILE_BUFFER_SIZE: usize = 512;

pub const LONG_DIR_ENT_NAME_CAPACITY: usize = 13;
pub const SHORT_DIR_ENT_NAME_CAPACITY: usize = 11;

/// For Short Directory Entry Name[0] and Long Directory Entry Ord
///
/// Deleted
pub const DIR_ENTRY_UNUSED: u8 = 0xE5;
/// For Short Directory Entry Name[0]
pub const DIR_ENTRY_LAST_AND_UNUSED: u8 = 0x00;
/// For Long Directory Entry Ord as the last entry mask
///
/// Q: The default maximum number of lde does not exceed 0x40?
///    But the maximum number of files within a directory of a FAT
///    file system is 65,536. So, how to deal with lfn.ord?
///
/// A: DO NOT misunderstand the meaning of this mask.
///    This mask should be for ord in the same file. The long
///    file name of a long directory entry only has 13 unicode
///    characters. When the file name exceeds 13 characters,
///    multiple long directory entries are required.
pub const LAST_LONG_ENTRY: u8 = 0x40;

pub const MAX_CLUSTER_FAT12: usize = 4085;
pub const MAX_CLUSTER_FAT16: usize = 65525;
pub const MAX_CLUSTER_FAT32: usize = 268435445;

/// The two reserved clusters at the start of the FAT, and FAT[1] high bit mask as follows:
/// Bit ClnShutBitMask -- If bit is 1, volume is "clean". If bit is 0, volume is "dirty".
/// Bit HrdErrBitMask  -- If this bit is 1, no disk read/write errors were encountered.
///                       If this bit is 0, the file system driver encountered a disk I/O error on the Volume
///                       the last time it was mounted, which is an indicator that some sectors may have gone bad on the volume.
pub const CLN_SHUT_BIT_MASK_FAT32: u32 = 0x08000000;
pub const HRD_ERR_BIT_MASK_FAT32: u32 = 0x04000000;

type Error = BlockDeviceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDeviceError {
    ClusterChain(ClusterChainErr),
    Dir(DirError),
    File(FileError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VirFileType {
    Dir = ATTR_DIRECTORY,
    File = ATTR_ARCHIVE,
}

pub(crate) fn read_le_u16(input: &[u8]) -> u16 {
    let (int_bytes, _) = input.split_at(core::mem::size_of::<u16>());
    u16::from_le_bytes(int_bytes.try_into().unwrap())
}

pub(crate) fn read_le_u32(input: &[u8]) -> u32 {
    let (int_bytes, _) = input.split_at(core::mem::size_of::<u32>());
    u32::from_le_bytes(int_bytes.try_into().unwrap())
}

pub(crate) fn is_illegal(chs: &str) -> bool {
    let illegal_char = "\\/:*?\"<>|";
    for ch in illegal_char.chars() {
        if chs.contains(ch) {
            return true;
        }
    }
    false
}

pub(crate) fn sfn_or_lfn(name: &str) -> NameType {
    let (name, extension) = match name.find('.') {
        Some(i) => (&name[0..i], &name[i + 1..]),
        None => (&name[0..], ""),
    };

    if name.is_ascii()
        && !name.contains(|ch: char| ch.is_ascii_uppercase())
        && !name.contains(' ')
        && !name.contains('.')
        && !extension.contains('.')
        && name.len() <= 8
        && extension.len() <= 3
    {
        NameType::SFN
    } else {
        NameType::LFN
    }
}

/// 根据文件名, 返回需要的长目录项数目
pub(crate) fn get_lde_cnt(value_str: &str) -> usize {
    // eg. value = "hello, 你好!" -> value.chars().count() = 10
    let num_char = value_str.chars().count();
    // 向上取整
    if num_char % 13 == 0 {
        num_char / 13
    } else {
        num_char / 13 + 1
    }
}

/// 根据文件名, 获取对应的第 count 个长目录项的名字对应于文件名的下标
pub(crate) fn get_lfn_index(value_str: &str, count: usize) -> usize {
    let end = 13 * (count - 1);
    let mut len = 0;
    for (index, ch) in value_str.chars().enumerate() {
        if (0..end).contains(&index) {
            len += ch.len_utf8();
        }
    }
    len
}

pub(crate) fn generate_checksum(value: &[u8]) -> u8 {
    let mut checksum = 0;
    for &i in value {
        checksum = (if checksum & 1 == 1 { 0x80 } else { 0 } + (checksum >> 1) + i as u32) & 0xFF;
    }
    checksum as u8
}

pub(crate) fn get_needed_sector(value: usize) -> usize {
    if value % BLOCK_SIZE != 0 {
        value / BLOCK_SIZE + 1
    } else {
        value / BLOCK_SIZE
    }
}

/// 将长文件名拆分, 返回字符串数组
pub fn long_name_split(name: &str) -> Vec<[u16; 13]> {
    let mut name: Vec<u16> = name.encode_utf16().collect();
    let len = name.len() as u32; // 注意: 要有 \0

    // 计算需要几个目录项, 向上取整
    // 以 13个字符为单位进行切割, 每一组占据一个目录项
    let lfn_cnt = (len + LONG_NAME_LEN - 1) / LONG_NAME_LEN;
    if len < lfn_cnt * LONG_NAME_LEN {
        name.push(0x00);
        while name.len() < (lfn_cnt * LONG_NAME_LEN) as usize {
            name.push(0xFF);
        }
    }
    name.chunks(LONG_NAME_LEN as usize)
        .map(|x| {
            let mut arr = [0u16; 13];
            arr.copy_from_slice(x);
            arr
        })
        .collect()
}

/// 拆分文件名和后缀
pub fn split_name_ext(name: &str) -> (&str, &str) {
    match name {
        "." => return (".", ""),
        ".." => return ("..", ""),
        _ => {
            let mut name_and_ext: Vec<&str> = name.split(".").collect(); // 按 . 进行分割
            if name_and_ext.len() == 1 {
                // 如果没有后缀名则推入一个空值
                name_and_ext.push("");
            }
            (name_and_ext[0], name_and_ext[1])
        }
    }
}

/// 将短文件名格式化为目录项存储的内容
pub fn short_name_format(name: &str) -> ([u8; 8], [u8; 3]) {
    let (name, ext) = split_name_ext(name);
    let name_bytes = name.as_bytes();
    let ext_bytes = ext.as_bytes();
    let mut f_name = [0u8; 8];
    let mut f_ext = [0u8; 3];
    for i in 0..8 {
        if i >= name_bytes.len() {
            f_name[i] = 0x20; // 不足的用0x20进行填充
        } else {
            f_name[i] = (name_bytes[i] as char).to_ascii_uppercase() as u8;
        }
    }
    for i in 0..3 {
        if i >= ext_bytes.len() {
            f_ext[i] = 0x20; // 不足的用0x20进行填充
        } else {
            f_ext[i] = (ext_bytes[i] as char).to_ascii_uppercase() as u8;
        }
    }
    (f_name, f_ext)
}

// 由长文件名生成短文件名
pub fn generate_short_name(long_name: &str) -> String {
    let (name_, ext_) = split_name_ext(long_name);
    let name = name_.as_bytes();
    let extension = ext_.as_bytes();
    let mut short_name = String::new();
    // 取长文件名的前6个字符加上"~1"形成短文件名, 扩展名不变,
    // 目前不支持重名, 即"~2""~3"; 支持重名与在目录下查找文件的方法绑定
    for i in 0..6 {
        short_name.push((name[i] as char).to_ascii_uppercase())
    }
    short_name.push('~');
    short_name.push('1');
    let ext_len = extension.len();
    for i in 0..3 {
        // fill extension
        if i >= ext_len {
            short_name.push(0x20 as char); // 不足的用0x20进行填充
        } else {
            short_name.push((extension[i] as char).to_ascii_uppercase());
        }
    }
    // 返回一个长度为 11 的string数组
    short_name
}

// TODO
// 1. change name
// 2. time 处理
// 3. 长短名转化(~n)(目前只有~1)
// 4. 删除文件后, 目录下的目录项的物理位置上的移动
