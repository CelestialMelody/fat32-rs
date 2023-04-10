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
//! The following characters are not legal in any bytes of DIR_Name:
//! - Values less than 0x20 except for the special case of 0x05 in DIR_Name[0] described above.
//! - 0x22, 0x2A, 0x2B, 0x2C, 0x2E, 0x2F, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x5B, 0x5C, 0x5D,
//!   and 0x7C
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

use alloc::string::String;

#[derive(PartialEq, Debug, Clone, Copy)]
#[repr(u8)]
pub enum FATDiskInodeType {
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

/// FAT 32 Byte Directory Entry Structure
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
    dir_name: [u8; 8],
    /// Short Name Extension
    dir_extension: [u8; 3],
    /// Attributes
    ///
    /// size: 1 byte      offset: 11 Bytes (0xB)
    //
    //  描述文件的属性，该字段在短文件中不可取值 0x0F (标志是长文件)
    dir_attr: FATDiskInodeType,
    /// Reserved for Windows NT
    ///
    /// size: 1 byte      offset: 12 Bytes (0xC)    value: 0x00
    //
    //  这个位默认为 0,只有短文件名时才有用. 一般初始化为 0 后不再修改, 可能的用法为:
    //  当为 0x00 时为文件名全大写, 当为 0x08 时为文件名全小写;
    //  0x10 时扩展名全大写, 0x00 扩展名全小写; 当为 0x18 时为文件名全小写, 扩展名全大写
    dir_ntres: u8,
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
    dir_crt_time_tenth: u8,
    /// Time file was created
    /// The granularity of the seconds part of DIR_CrtTime is 2 seconds.
    ///
    /// size: 2 bytes     offset: 14 Bytes (0xE)
    //
    //  文件创建日期, 16bit 也划分为三个部分:
    //    0~4bit 为日, 有效值为 1~31
    //    5~8bit 为月, 有效值为 1~12
    //    9~15bit 为年, 有效值为 0~127，这是一个相对于 1980 年的年数值 (该值加上 1980 即为文件创建的日期值)
    dir_crt_time: u16,
    /// Date file was created
    ///
    /// size: 2 bytes     offset: 16 Bytes (0x10)
    dir_crt_date: u16,
    /// Last access date
    /// Note that there is no last access time, only a
    /// date. This is the date of last read or write. In the case of a write,
    /// this should be set to the same date as DIR_WrtDate.
    ///
    /// size: 2 bytes     offset: 18 Bytes (0x12)
    dir_lst_acc_date: u16,
    /// High word (16 bis) of this entry's first cluster number (always 0 on FAT12 and FAT16)
    ///
    /// size: 2 bytes     offset: 20 Bytes (0x14~0x15)
    dir_fst_clus_hi: u16,
    /// Time of last write
    /// Note that file creation is considered a write.
    ///
    /// size: 2 bytes     offset: 22 Bytes (0x16~0x17)
    dir_wrt_time: u16,
    /// Date of last write
    /// Note that file creation is considered a write.
    ///
    /// size: 2 bytes     offset: 24 Bytes (0x18~0x19)
    dir_wrt_date: u16,
    /// Cluster number of the first cluster
    /// Low word (16-bit) of this entry's first cluster number
    ///
    /// size: 2 bytes     offset: 26 Bytes (0x1A~0x1B)
    //
    //  文件内容起始簇号的低两个字节, 与 0x14~0x15 字节处的高两个字节组成文件内容起始簇号
    dir_fst_clus_lo: u16,
    /// File size in bytes
    /// 32-bit (DWORD) unsigned holding this file's size in bytes
    ///
    /// size: 4 bytes     offset: 28 Bytes (0x1C~0x1F)
    //
    //  文件内容大小字节数，只对文件有效，子目录的目录项此处全部设置为 0
    dir_file_size: u32,
}

impl ShortDirEntry {
    fn empty() -> Self {
        Self {
            dir_name: [0; 8],
            dir_extension: [0; 3],
            dir_attr: FATDiskInodeType::AttrArchive,
            dir_ntres: 0,
            dir_crt_time_tenth: 0,
            dir_crt_time: 0,
            dir_crt_date: 0,
            dir_lst_acc_date: 0,
            dir_fst_clus_hi: 0,
            dir_wrt_time: 0,
            dir_wrt_date: 0,
            dir_fst_clus_lo: 0,
            dir_file_size: 0,
        }
    }

    fn new(cluster: u32, name_str: &str, create_type: OpType) -> Self {
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

    pub fn is_valid(&self) -> bool {
        if self.dir_name[0] != 0xE5 {
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        if self.dir_name[0] == 0x00 {
            true
        } else {
            false
        }
    }

    pub fn is_dir(&self) -> bool {
        if 0 != (self.dir_attr as u8 & ATTR_DIRECTORY) {
            true
        } else {
            false
        }
    }

    pub fn is_long(&self) -> bool {
        if self.dir_attr as u8 == ATTR_LONG_NAME {
            true
        } else {
            false
        }
    }

    pub fn attr(&self) -> u8 {
        self.dir_attr as u8
    }

    pub fn file_size(&self) -> u32 {
        self.dir_file_size
    }

    pub fn set_file_size(&mut self, dir_file_size: u32) {
        self.dir_file_size = dir_file_size;
    }

    // Get the start cluster number of the file
    pub fn first_cluster(&self) -> u32 {
        ((self.dir_fst_clus_hi as u32) << 16) + (self.dir_fst_clus_lo as u32)
    }

    // Set the start cluster number of the file
    pub fn set_first_cluster(&mut self, cluster: u32) {
        self.dir_fst_clus_hi = ((cluster & 0xFFFF0000) >> 16) as u16;
        self.dir_fst_clus_lo = (cluster & 0x0000FFFF) as u16;
    }

    pub fn get_name_uppercase(&self) -> String {
        let mut name: String = String::new();
        for i in 0..8 {
            if self.dir_name[i] == 0x20 {
                break;
            } else {
                name.push(self.dir_name[i] as char);
            }
        }
        for i in 0..3 {
            if self.dir_extension[i] == 0x20 {
                break;
            } else {
                if i == 0 {
                    name.push('.');
                }
                name.push(self.dir_extension[i] as char);
            }
        }
        name
    }

    pub fn get_name_lowercase(&self) -> String {
        self.get_name_uppercase().to_ascii_lowercase()
    }

    pub fn set_case(&mut self, case: u8) {
        self.dir_ntres = case;
    }

    pub fn clear(&mut self) {
        self.dir_file_size = 0;
        self.set_first_cluster(0);
    }

    pub fn delete(&mut self) {
        self.clear();
        self.dir_name[0] = 0xE5;
    }
}
