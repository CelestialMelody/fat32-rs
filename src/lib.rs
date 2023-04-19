#![no_std]
pub mod block_cache;
pub mod block_device;
pub mod bpb;
pub mod dir;
pub mod entry;
pub mod fat;
pub mod file;

use crate::dir::DirError;
use crate::entry::NameType;
use crate::fat::FatError;
use crate::file::FileError;

use core::convert::TryInto;
use core::str;

extern crate alloc;

pub const LEAD_SIGNATURE: u32 = 0x41615252;
pub const STRUCT_SIGNATURE: u32 = 0x61417272;
pub const TRAIL_SIGNATURE: u32 = 0xAA550000;

pub const FREE_CLUSTER: u32 = 0x00000000;
pub const END_CLUSTER: u32 = 0x0FFFFFF8;
pub const BAD_CLUSTER: u32 = 0x0FFFFFF7;
/// EOC: End of Cluster Chain
/// note that we still USE this cluster and this clsuter id is not EOC,
/// but in FAT table, the value of this cluster is EOC
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

pub const BLOCK_CACHE_LIMIT: usize = 64;

// Charactor
pub const SPACE: u8 = 0x20;
pub const DOT: u8 = 0x2E;

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
/// Bit ClnShutBitMask -- If bit is 1, volume is “clean”. If bit is 0, volume is “dirty”.
/// Bit HrdErrBitMask  -- If this bit is 1, no disk read/write errors were encountered.
///                       If this bit is 0, the file system driver encountered a disk I/O error on the Volume
///                       the last time it was mounted, which is an indicator that some sectors may have gone bad on the volume.
pub const CLN_SHUT_BIT_MASK_FAT32: u32 = 0x08000000;
pub const HRD_ERR_BIT_MASK_FAT32: u32 = 0x04000000;

type Error = BlockDeviceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDeviceError {
    Fat(FatError),
    Dir(DirError),
    File(FileError),
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
