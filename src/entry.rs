//! FAT32 Directory Structure
//!
//! For FAT32, the root directory can be of variable size and is a cluster chain, just like any other
//! directory is. The first cluster of the root directory on a FAT32 volume is stored in BPB_RootClus.
//! Unlike other directories, the root directory itself on any FAT type does not have any date or time
//! stamps, does not have a file name (other than the implied file name "\"), and does not contain "." and
//! ".." files as the first two directory entries in the directory. The only other special aspect of the root
//! directory is that it is the only directory on the FAT volume for which it is valid to have a file that has
//! only the ATTR_VOLUME_ID attribute bit set.

//! Dir_Name[0]
//!
//! DIR_Name[0]
//! Special notes about the first byte (DIR_Name[0]) of a FAT directory entry:
//! - If DIR_Name[0] == 0xE5, then the directory entry is free (there is no file or directory name in this
//!   entry).
//! - If DIR_Name[0] == 0x00, then the directory entry is free (same as for 0xE5), and there are no
//!   allocated directory entries after this one (all of the DIR_Name[0] bytes in all of the entries after
//!   this one are also set to 0).
//!   The special 0 value, rather than the 0xE5 value, indicates to FAT file system driver code that the
//!   rest of the entries in this directory do not need to be examined because they are all free.
//! - If DIR_Name[0] == 0x05, then the actual file name character for this byte is 0xE5. 0xE5 is
//!   actually a valid KANJI lead byte value for the character set used in Japan. The special 0x05 value
//!   is used so that this special file name case for Japan can be handled properly and not cause FAT file
//!   system code to think that the entry is free.
//!
//! The DIR_Name field is actually broken into two parts+ the 8-character main part of the name, and the
//! 3-character extension. These two parts are “trailing space padded” with bytes of 0x20.
//!
//! DIR_Name[0] may not equal 0x20. There is an implied ‘.’ character between the main part of the
//! name and the extension part of the name that is not present in DIR_Name. Lower case characters are
//! not allowed in DIR_Name (what these characters are is country specific).
//!
//! DIR_Name[..]
//! The following characters are not legal in any bytes of DIR_Name:
//! - Values less than 0x20 except for the special case of 0x05 in DIR_Name[0] described above.
//! - 0x22, 0x2A, 0x2B, 0x2C, 0x2E, 0x2F, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x5B, 0x5C, 0x5D,
//!   and 0x7C
//! See [`ShortDirEntry::is_valid()`].
//!
//! FAT file system on disk data structure is all "little endian".
//! This is important if your machine is a “big endian” machine, because you will have to translate
//! between big and little endian as you move data to and from the disk.

#![allow(unused)]

use crate::dir::OpType;
use crate::{
    ATTR_ARCHIVE, ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_LONG_NAME, ATTR_READ_ONLY, ATTR_SYSTEM,
    ATTR_VOLUME_ID,
};

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{convert::TryInto, fmt::Debug, mem};

#[derive(PartialEq, Debug, Clone, Copy)]
#[repr(u8)]
pub enum FATAttr {
    /// Indicates that writes to the file should fail.
    AttrReadOnly = ATTR_READ_ONLY, // 只读
    /// Indicates that normal directory listings should not show this file.
    AttrHidden = ATTR_HIDDEN, // 隐藏
    /// Indicates that this is an operating system file.
    AttrSystem = ATTR_SYSTEM, // 系统
    /// Root Dir
    /// There should only be one “file” on the volume that has this attribute
    /// set, and that file must be in the root directory. This name of this file is
    /// actually the label for the volume. DIR_FstClusHI and
    /// DIR_FstClusLO must always be 0 for the volume label (no data
    /// clusters are allocated to the volume label file).
    AttrVolumeID = ATTR_VOLUME_ID, // 根目录/卷标
    /// Indicates that this file is actually a container for other files.
    AttrDirectory = ATTR_DIRECTORY, // 子目录
    /// This attribute supports backup utilities. This bit is set by the FAT file
    /// system driver when a file is created, renamed, or written to. Backup
    /// utilities may use this attribute to indicate which files on the volume
    /// have been modified since the last time that a backup was performed.
    AttrArchive = ATTR_ARCHIVE, // 归档
    /// Idicates that the “file” is actually part of the long name entry for some other file.
    AttrLongName = ATTR_LONG_NAME, // 长文件名
}

#[derive(PartialEq, Debug, Clone, Copy, PartialOrd)]
pub enum EntryType {
    Dir,
    File,
    LFN,
    Deleted,
}

/// FAT 32 Byte Directory Entry Structure
///
/// TODO: Realize Time and Date
//
// 11+1+1+1+2+2+2+2+2+2+2+4 = 32
#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct ShortDirEntry {
    /// Short Name
    ///
    /// size: (8+3) bytes    offset: 0 (0x0)
    //
    //  文件名, 如果该目录项正在使用中 0x0 位置的值为文件名或子目录名的第一个字符, 如果该目录项未被使用
    //  name[0] 位置的值为 0x0, 如果该目录项曾经被使用过但是现在已经被删除则 name[0] 位置的值为 0xE5
    name: [u8; 8],
    /// Short Name Extension
    extension: [u8; 3],
    /// Attributes
    ///
    /// size: 1 byte      offset: 11 Bytes (0xB)
    //
    //  描述文件的属性，该字段在短文件中不可取值 0x0F (标志是长文件)
    attr: u8,
    // attr: FATAttr,
    /// Reserved for Windows NT
    ///
    /// size: 1 byte      offset: 12 Bytes (0xC)    value: 0x00
    //
    //  这个位默认为 0,只有短文件名时才有用. 一般初始化为 0 后不再修改, 可能的用法为:
    //  当为 0x00 时为文件名全大写, 当为 0x08 时为文件名全小写;
    //  0x10 时扩展名全大写, 0x00 扩展名全小写; 当为 0x18 时为文件名全小写, 扩展名全大写
    nt_res: u8,
    /// Millisecond stamp at file creation time. This field actually
    /// contains a count of tenths of a second. The granularity of the
    /// seconds part of DIR_CrtTime is 2 seconds so this field is a
    /// count of tenths of a second and its valid value range is 0-199
    /// inclusive.
    ///
    /// size: 1 byte      offset: 13 Bytes (0xD)    value range: 0-199
    //
    //  文件创建的时间: 时-分-秒，16bit 被划分为 3个部分:
    //    0~4bit 为秒, 以 2秒为单位，有效值为 0~29，可以表示的时刻为 0~58
    //    5~10bit 为分, 有效值为 0~59
    //    11~15bit 为时, 有效值为 0~23
    crt_time_tenth: u8,
    /// Time file was created
    /// The granularity of the seconds part of DIR_CrtTime is 2 seconds.
    ///
    /// size: 2 bytes     offset: 14 Bytes (0xE)
    //
    //  文件创建日期, 16bit 也划分为三个部分:
    //    0~4bit 为日, 有效值为 1~31
    //    5~8bit 为月, 有效值为 1~12
    //    9~15bit 为年, 有效值为 0~127，这是一个相对于 1980 年的年数值 (该值加上 1980 即为文件创建的日期值)
    crt_time: u16,
    /// Date file was created
    ///
    /// size: 2 bytes     offset: 16 Bytes (0x10)
    crt_date: u16,
    /// Last access date
    /// Note that there is no last access time, only a
    /// date. This is the date of last read or write. In the case of a write,
    /// this should be set to the same date as DIR_WrtDate.
    ///
    /// size: 2 bytes     offset: 18 Bytes (0x12)
    lst_acc_date: u16,
    /// High word (16 bis) of this entry's first cluster number (always 0 on FAT12 and FAT16)
    ///
    /// size: 2 bytes     offset: 20 Bytes (0x14~0x15)
    fst_clus_hi: u16,
    /// Time of last write
    /// Note that file creation is considered a write.
    ///
    /// size: 2 bytes     offset: 22 Bytes (0x16~0x17)
    wrt_time: u16,
    /// Date of last write
    /// Note that file creation is considered a write.
    ///
    /// size: 2 bytes     offset: 24 Bytes (0x18~0x19)
    wrt_date: u16,
    /// Cluster number of the first cluster
    /// Low word (16-bit) of this entry's first cluster number
    ///
    /// size: 2 bytes     offset: 26 Bytes (0x1A~0x1B)
    //
    //  文件内容起始簇号的低两个字节, 与 0x14~0x15 字节处的高两个字节组成文件内容起始簇号
    fst_clus_lo: u16,
    /// File size in bytes
    /// 32-bit (DWORD) unsigned holding this file's size in bytes
    ///
    /// size: 4 bytes     offset: 28 Bytes (0x1C~0x1F)
    //
    //  文件内容大小字节数，只对文件有效，子目录的目录项此处全部设置为 0
    file_size: u32,
}

impl ShortDirEntry {
    pub fn new(cluster: u32, name: &[u8], extension: &[u8], create_type: OpType) -> Self {
        let mut item = Self::empty();
        let mut name_: [u8; 8] = [0x20; 8];
        let mut extension_: [u8; 3] = [0x20; 3];
        name_[0..name.len()].copy_from_slice(name);
        extension_[0..extension.len()].copy_from_slice(extension);
        item.name = name_;
        item.extension = extension_;
        match create_type {
            OpType::File => item.attr = ATTR_ARCHIVE,
            OpType::Dir => item.attr = ATTR_DIRECTORY,
        }
        item.set_first_cluster(cluster);
        item
    }

    pub fn new_str(cluster: u32, name_str: &str, create_type: OpType) -> Self {
        let (name, extension) = match name_str.find('.') {
            Some(i) => (&name_str[0..i], &name_str[i + 1..]),
            None => (&name_str[0..], ""),
        };

        let mut item = [0; 32];
        let _item = [0x20; 11]; // invalid name

        // 初始化为 0x20, 0x20 为 ASCII 码中的空格字符; 0x00..0x0B = 0..11
        item[0x00..0x0B].copy_from_slice(&_item);
        // name 的长度可能不足 8 个字节; 0..name.len()
        item[0x00..0x00 + name.len()].copy_from_slice(name.as_bytes());
        // ext 的长度可能不足 3 个字节; 8..extension.len()
        item[0x08..0x08 + extension.len()].copy_from_slice(extension.as_bytes());

        // 将 name 和 ext 部分转换为大写
        // item[0x00..0x00 + name.len()].make_ascii_uppercase();
        // item[0x08..0x08 + extension.len()].make_ascii_uppercase();

        // Q: 采用小端还是大端序存储数据?
        // A: 采用小端序存储数据, 与 FAT32 文件系统的存储方式一致
        //
        // FAT file system on disk data structure is all "little endian".
        //
        // to_le_bytes() 方法将 u32 类型的数据转换为 小端序 的字节数组
        // eg. 0x12345678 -> [0x78, 0x56, 0x34, 0x12]
        let mut cluster: [u8; 4] = cluster.to_le_bytes();

        // 0x1A~0x1B 字节为文件内容起始簇号的低两个字节, 与 0x14~0x15 字节处的高两个字节组成文件内容起始簇号
        item[0x14..0x16].copy_from_slice(&cluster[2..4]);
        item[0x1A..0x1C].copy_from_slice(&cluster[0..2]);

        match create_type {
            OpType::Dir => item[0x0B] = ATTR_DIRECTORY,
            OpType::File => item[0x10] = ATTR_ARCHIVE,
        }

        unsafe { *(item.as_ptr() as *const ShortDirEntry) }
    }

    pub fn new_bytes(cluster: u32, name_bytes: &[u8], create_type: OpType) -> Self {
        let mut item = [0; 32];
        item[0x00..0x0B].copy_from_slice(name_bytes);

        let mut cluster: [u8; 4] = cluster.to_be_bytes();
        cluster.reverse();

        item[0x14..0x16].copy_from_slice(&cluster[2..4]);
        item[0x1A..0x1C].copy_from_slice(&cluster[0..2]);

        match create_type {
            OpType::Dir => item[0x0B] = ATTR_DIRECTORY,
            OpType::File => item[0x10] = ATTR_ARCHIVE,
        }

        unsafe { *(item.as_ptr() as *const ShortDirEntry) }
    }

    pub fn check_sum(&self) -> u8 {
        let mut name_: [u8; 11] = [0u8; 11];
        let mut sum: u8 = 0;
        for i in 0..8 {
            name_[i] = self.name[i];
        }
        for i in 0..3 {
            name_[i + 8] = self.extension[i];
        }
        // for i in 0..11 {
        //     if (sum & 1) != 0 {
        //         sum = 0x80 + (sum >> 1) + name_[i];
        //     } else {
        //         sum = (sum >> 1) + name_[i];
        //     }
        // }
        for i in 0..11 {
            sum = ((sum & 1) << 7) + (sum >> 1) + self.name[i];
        }
        sum
    }

    pub fn name(&self) -> String {
        let name_len = self.name.iter().position(|&x| x == 0x20).unwrap_or(8);
        let ext_len = self.extension.iter().position(|&x| x == 0x20).unwrap_or(3);
        macro_rules! as_u8str {
            ($a:expr) => {
                core::str::from_utf8(&$a).unwrap_or("")
            };
        }
        {
            if ext_len != 0 {
                [
                    as_u8str!(self.name[..name_len]),
                    as_u8str!(['.' as u8][..]),
                    as_u8str!(self.extension[..ext_len]),
                ]
                .join("")
            } else {
                as_u8str!(self.name[0..name_len]).to_string()
            }
        }
    }

    pub fn full_name_bytes_array(&self) -> ([u8; 12], usize) {
        let mut len = 0;
        let mut full_name = [0; 12];

        for &i in self.name.iter() {
            if i != 0x20 {
                full_name[len] = i;
                len += 1;
            }
        }

        if self.extension[0] != 0x20 {
            full_name[len] = b'.';
            len += 1;
        }

        for &i in self.extension.iter() {
            if i != 0x20 {
                full_name[len] = i;
                len += 1;
            }
        }

        (full_name, len)
    }
}

impl ShortDirEntry {
    fn empty() -> Self {
        Self {
            name: [0; 8],
            extension: [0; 3],
            attr: ATTR_ARCHIVE,
            nt_res: 0,
            crt_time_tenth: 0,
            crt_time: 0,
            crt_date: 0,
            lst_acc_date: 0,
            fst_clus_hi: 0,
            wrt_time: 0,
            wrt_date: 0,
            fst_clus_lo: 0,
            file_size: 0,
        }
    }

    fn root_dir(cluster: u32) -> Self {
        let mut item = Self::empty();
        item.set_first_cluster(cluster);
        item.attr = ATTR_DIRECTORY;
        item
    }

    // Get the start cluster number of the file
    pub fn first_cluster(&self) -> u32 {
        ((self.fst_clus_hi as u32) << 16) + (self.fst_clus_lo as u32)
    }

    // Set the start cluster number of the file
    pub fn set_first_cluster(&mut self, cluster: u32) {
        self.fst_clus_hi = ((cluster & 0xFFFF0000) >> 16) as u16;
        self.fst_clus_lo = (cluster & 0x0000FFFF) as u16;
    }

    /// directory entry is free
    pub fn is_free(&self) -> bool {
        self.name[0] == 0xE5 || self.name[0] == 0x00 || self.name[0] == 0x05
    }

    pub fn is_valid_name(&self) -> bool {
        if self.name[0] < 0x20 {
            return self.name[0] == 0x05;
        } else {
            for i in 0..8 {
                if (i < 3) {
                    if self.extension[i] < 0x20 {
                        return false;
                    }
                    if self.extension[i] == 0x22
                        || self.extension[i] == 0x2A
                        || self.extension[i] == 0x2E
                        || self.extension[i] == 0x2F
                        || self.extension[i] == 0x3A
                        || self.extension[i] == 0x3C
                        || self.extension[i] == 0x3E
                        || self.extension[i] == 0x3F
                        || self.extension[i] == 0x5B
                        || self.extension[i] == 0x5C
                        || self.extension[i] == 0x5D
                        || self.extension[i] == 0x7C
                    {
                        return false;
                    }
                }
                if self.name[i] < 0x20 {
                    return false;
                }
                if self.name[i] == 0x22
                    || self.name[i] == 0x2A
                    || self.name[i] == 0x2E
                    || self.name[i] == 0x2F
                    || self.name[i] == 0x3A
                    || self.name[i] == 0x3C
                    || self.name[i] == 0x3E
                    || self.name[i] == 0x3F
                    || self.name[i] == 0x5B
                    || self.name[i] == 0x5C
                    || self.name[i] == 0x5D
                    || self.name[i] == 0x7C
                {
                    return false;
                }
            }
            return true;
        }
    }

    pub fn is_dir(&self) -> bool {
        self.attr == ATTR_DIRECTORY
    }

    pub fn is_long(&self) -> bool {
        self.attr as u8 == ATTR_LONG_NAME
    }

    pub fn is_file(&self) -> bool {
        self.attr == ATTR_ARCHIVE
            || self.attr == ATTR_HIDDEN
            || self.attr == ATTR_SYSTEM
            || self.attr == ATTR_READ_ONLY
    }

    pub fn attr(&self) -> u8 {
        self.attr as u8
    }

    pub fn file_size(&self) -> u32 {
        self.file_size
    }

    pub fn set_file_size(&mut self, file_size: u32) {
        self.file_size = file_size;
    }

    pub fn get_name_uppercase(&self) -> String {
        let mut name: String = String::new();
        for i in 0..8 {
            if self.name[i] == 0x20 {
                break;
            } else {
                name.push(self.name[i] as char);
            }
        }
        for i in 0..3 {
            if self.extension[i] == 0x20 {
                break;
            } else {
                if i == 0 {
                    name.push('.');
                }
                name.push(self.extension[i] as char);
            }
        }
        name
    }

    pub fn get_name_lowercase(&self) -> String {
        self.get_name_uppercase().to_ascii_lowercase()
    }

    pub fn clear(&mut self) {
        self.file_size = 0;
        self.set_first_cluster(0);
    }

    pub fn delete(&mut self) {
        self.clear();
        self.name[0] = 0xE5;
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut ShortDirEntry as *mut u8, 32) }
    }

    pub fn as_bytes_array_mut(&mut self) -> [u8; 32] {
        let mut buf = [0; 32];
        let len = core::mem::size_of::<ShortDirEntry>();
        unsafe {
            core::ptr::copy_nonoverlapping(
                self as *mut ShortDirEntry as *mut u8,
                buf.as_mut_ptr(),
                len,
            )
        }
        buf
    }

    pub fn from_buf(buf: &[u8]) -> Self {
        unsafe { *(buf.as_ptr() as *const ShortDirEntry) }
    }
}
