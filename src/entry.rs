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
//! 3-character extension. These two parts are "trailing space padded" with bytes of 0x20.
//!
//! DIR_Name[0] may not equal 0x20. There is an implied '.' character between the main part of the
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
//! This is important if your machine is a "big endian" machine, because you will have to translate
//! between big and little endian as you move data to and from the disk.

//! When a directory is created, a file with the ATTR_DIRECTORY bit set in its DIR_Attr field, you set
//! its DIR_FileSize to 0. DIR_FileSize is not used and is always 0 on a file with the
//! ATTR_DIRECTORY attribute (directories are sized by simply following their cluster chains to the
//! EOC mark). One cluster is allocated to the directory (unless it is the root directory on a FAT16/FAT12
//! volume), and you set DIR_FstClusLO and DIR_FstClusHI to that cluster number and place an EOC
//! mark in that clusters entry in the FAT. Next, you initialize all bytes of that cluster to 0. If the directory
//! is the root directory, you are done (there are no dot or dotdot entries in the root directory). If the
//! directory is not the root directory, you need to create two special entries in the first two 32-byte
//! directory entries of the directory (the first two 32 byte entries in the data region of the cluster you
//! just allocated): ".       " and "..      ".
//!
//! These are called the dot and dotdot entries. The DIR_FileSize field on both entries is set to 0, and all
//! of the date and time fields in both of these entries are set to the same values as they were in the
//! directory entry for the directory that you just created. You now set DIR_FstClusLO and
//! DIR_FstClusHI for the dot entry (the first entry) to the same values you put in those fields for the
//! directories directory entry (the cluster number of the cluster that contains the dot and dotdot entries).
//!
//! Finally, you set DIR_FstClusLO and DIR_FstClusHI for the dotdot entry (the second entry) to the
//! first cluster number of the directory in which you just created the directory (value is 0 if this directory
//! is the root directory even for FAT32 volumes).
//!
//! Here is the summary for the dot and dotdot entries:
//! - The dot entry is a directory that points to itself.
//! - The dotdot entry points to the starting cluster of the parent of this directory (which is 0 if this
//!   directories parent is the root directory).

//! Organization and Association of Short & Long Directory Entries
//!
//! A set of long entries is always associated with a short entry that they always immediately precede.
//! Long entries always immediately precede and are physically contiguous with, the short entry they are
//! associated with. The file system makes a few other checks to ensure that a set of long entries is
//! actually associated with a short entry.
//!
//! First, every member of a set of long entries is uniquely numbered and the last member of the set is or'd
//! with a flag indicating that it is, in fact, the last member of the set. The LDIR_Ord field is used to
//! make this determination. The first member of a set has an LDIR_Ord value of one. The nth long
//! member of the set has a value of (n OR LAST_LONG_ENTRY). Note that the LDIR_Ord field
//! cannot have values of 0xE5 or 0x00. These values have always been used by the file system to
//! indicate a "free" directory entry, or the "last" directory entry in a cluster. Values for LDIR_Ord do not
//! take on these two values over their range. Values for LDIR_Ord must run from 1 to (n OR
//! LAST_LONG_ENTRY). If they do not, the long entries are "damaged" and are treated as orphans by
//! the file system.
//!
//! Second, an 8-bit checksum is computed on the name contained in the short directory entry at the time
//! the short and long directory entries are created. All 11 characters of the name in the short entry are
//! used in the checksum calculation. The check sum is placed in every long entry. If any of the check
//! sums in the set of long entries do not agree with the computed checksum of the name contained in the
//! short entry, then the long entries are treated as orphans.
//!
//! Sum = 0;
//! for (FcbNameLen=11; FcbNameLen!=0; FcbNameLen--) {
//!     Sum = ((Sum & 1) ? 0x80 : 0) + (Sum >> 1) + *pFcbName++;
//! }
//!
//! As a consequence of this pairing, the short directory entry serves as the structure that contains fields
//! like: last access date, creation time, creation date, first cluster, and size.
//!  long directory entries are free to contain new
//! information and need not replicate information already available in the short entry. Principally, the
//! long entries contain the long name of a file. The name contained in a short entry which is associated
//! with a set of long entries is termed the alias name, or simply alias, of the file.

//!Storage of a Long-Name Within Long Directory Entries
//!
//! A long name can consist of more characters than can fit in a single long directory entry. When this
//! occurs the name is stored in more than one long entry. In any event, the name fields themselves
//! within the long entries are disjoint. The following example is provided to illustrate how a long name
//! is stored across several long directory entries. Names are also NUL terminated and padded with
//! 0xFFFF characters in order to detect corruption of long name fields by errant disk utilities. A name
//! that fits exactly in a n long directory entries (i.e. is an integer multiple of 13) is not NUL terminated
//! and not padded with 0xFFFFs.

//! Short Directory Entries
//!
//! [`ShortDirEntry`]
//!
//! Short names are limited to 8 characters followed by an optional period (.) and extension of up to 3
//! characters. The total path length of a short name cannot exceed 80 characters (64 char path + 3 drive
//! letter + 12 for 8.3 name + NUL) including the trailing NUL. The characters may be any combination
//! of letters, digits, or characters with code point values greater than 127. The following special
//! characters are also allowed: $  % ' - _ @ ~ ` ! ( ) { } ^ # &
//!
//! Names are stored in a short directory entry in the OEM code page that the system is configured for at
//! the time the directory entry is created.
//!
//! Short names passed to the file system are always converted to upper case and their original case value
//! is lost. One problem that is generally true of most OEM code pages is that they map lower to upper
//! case extended characters in a non-unique fashion. That is, they map multiple extended characters to a
//! single upper case character. This creates problems because it does not preserve the information that
//! the extended character provides. This mapping also prevents the creation of some file names that
//! would normally differ, but because of the mapping to upper case they become the same file name.
//!

//! Long Directory Entries
//!
//! [`LongDirEntry`]
//!
//! Long names are limited to 255 characters, not including the trailing NUL. The total path length of a
//! long name cannot exceed 260 characters, including the trailing NUL. The characters may be any
//! combination of those defined for short names with the addition of the period (.) character used
//! multiple times within the long name. A space is also a valid character in a long name as it always has
//! been for a short name. However, in short names it typically is not used. The following six special
//! characters are now allowed in a long name. They are not legal in a short name: + , ; = [ ]
//!
//! Embedded spaces within a long name are allowed. Leading and trailing spaces in a long name are
//! ignored.
//!
//! Leading and embedded periods are allowed in a name and are stored in the long name. Trailing
//! periods are ignored.
//!
//! Long names are stored in long directory entries in UNICODE. UNICODE characters are 16-bit
//! characters. It is not be possible to store UNICODE in short directory entries since the names stored
//! there are 8-bit characters or DBCS characters.
//!
//! Long names passed to the file system are not converted to upper case and their original case value is
//! preserved. UNICODE solves the case mapping problem prevalent in some OEM code pages by
//! always providing a translation for lower case characters to a single, unique upper case character.

//! Name Matching In Short & Long Names
//!
//! The names contained in the set of all short directory entries are termed the "short name space". The
//! names contained in the set of all long directory entries are termed the "long name space". Together,
//! they form a single unified name space in which no duplicate names can exist. That is: any name
//! within a specific directory, whether it is a short name or a long name, can occur only once in the name
//! space. Furthermore, although the case of a name is preserved in a long name, no two names can have
//! the same name although the names on the media actually differ by case. That is names like "foobar"
//! cannot be created if there is already a short entry with a name of "FOOBAR" or a long name with a
//! name of "FooBar".
//!
//! All types of search operations within the file system (i.e. find, open, create, delete, rename) are case-
//! insensitive. An open of "FOOBAR" will open either "FooBar" or "foobar" if one or the other exists.
//! A find using "FOOBAR" as a pattern will find the same files mentioned. The same rules are also true
//! for extended characters that are accented.
//!
//! A short name search operation checks only the names of the short directory entries for a match. A
//! long name search operation checks both the long and short directory entries. As the file system
//! traverses a directory, it caches the long-name sub-components contained in long directory entries. As
//! soon as a short directory entry is encountered that is associated with the cached long name, the long
//! name search operation will check the cached long name first and then the short name for a match.
//!
//! When a character on the media, whether it is stored in the OEM character set or in UNICODE, cannot
//! be translated into the appropriate character in the OEM or ANSI code page, it is always "translated" to
//! the "_" (underscore) character as it is returned to the user – it is NOT modified on the disk. This
//! character is the same in all OEM code pages and ANSI.

//! FAT Long Directory Entries
//!

// #![allow(unused)]

use super::vfs::VirFileType;
use super::{
    ATTR_ARCHIVE, ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_LONG_NAME, ATTR_READ_ONLY, ATTR_SYSTEM,
    ATTR_VOLUME_ID, DIR_ENTRY_LAST_AND_UNUSED, DIR_ENTRY_UNUSED, LAST_LONG_ENTRY,
    LONG_NAME_LEN_CAP, SPACE,
};

use alloc::string::{String, ToString};
use core::fmt::Debug;
use core::str;

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
    /// There should only be one "file" on the volume that has this attribute
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
    /// Idicates that the "file" is actually part of the long name entry for some other file.
    AttrLongName = ATTR_LONG_NAME, // 长文件名
}

/// FAT 32 Byte Directory Entry Structure
///
// 9 + 3 + 1 + 1 + 1 + 1 + 2 + 2 + 2 + 4 + 4 = 32 bytes
#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct ShortDirEntry {
    /// Short Name
    ///
    /// size: (8+3) bytes    offset: 0 (0x0~0xA)
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
    //  描述文件的属性, 该字段在短文件中不可取值 0x0F (标志是长文件)
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
    crt_time_tenth: u8,
    /// Time file was created
    /// The granularity of the seconds part of DIR_CrtTime is 2 seconds.
    ///
    /// size: 2 bytes     offset: 14 Bytes (0xE ~ 0xF)
    //
    //  文件创建的时间: 时-分-秒, 16bit 被划分为 3个部分:
    //    0~4bit 为秒, 以 2秒为单位, 有效值为 0~29, 可以表示的时刻为 0~58
    //    5~10bit 为分, 有效值为 0~59
    //    11~15bit 为时, 有效值为 0~23
    crt_time: u16,
    /// Date file was created
    ///
    /// size: 2 bytes     offset: 16 Bytes (0x10~0x11)
    //
    //  文件创建日期, 16bit 也划分为三个部分:
    //    0~4bit 为日, 有效值为 1~31
    //    5~8bit 为月, 有效值为 1~12
    //    9~15bit 为年, 有效值为 0~127, 这是一个相对于 1980 年的年数值 (该值加上 1980 即为文件创建的日期值 (1980–2107))
    crt_date: u16,
    /// Last access date
    /// Note that there is no last access time, only a
    /// date. This is the date of last read or write. In the case of a write,
    /// this should be set to the same date as DIR_WrtDate.
    ///
    /// size: 2 bytes     offset: 18 Bytes (0x12~0x13)
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
    /// DIR_FileSize is not used and is always 0 on a file with the ATTR_DIRECTORY attribute
    /// (directories are sized by simply following their cluster chains to the EOC mark).
    /// size: 4 bytes     offset: 28 Bytes (0x1C~0x1F)
    file_size: u32,
}

impl Default for ShortDirEntry {
    fn default() -> Self {
        Self::empty()
    }
}

impl ShortDirEntry {
    // All names must check if they have existed in the directory
    pub fn new(cluster: u32, name: &[u8], extension: &[u8], create_type: VirFileType) -> Self {
        let mut item = Self::empty();
        let mut name_: [u8; 8] = [SPACE; 8];
        let mut extension_: [u8; 3] = [SPACE; 3];
        name_[0..name.len()].copy_from_slice(name);
        extension_[0..extension.len()].copy_from_slice(extension);

        name_[..].make_ascii_uppercase();
        extension_[..].make_ascii_uppercase();

        item.name = name_;
        item.extension = extension_;
        match create_type {
            VirFileType::File => item.attr = ATTR_ARCHIVE,
            VirFileType::Dir => item.attr = ATTR_DIRECTORY,
        }
        item.set_first_cluster(cluster);
        item
    }

    // All names must check if they have existed in the directory
    pub fn new_form_name_str(cluster: u32, name_str: &str, create_type: VirFileType) -> Self {
        let (name, extension) = match name_str.find('.') {
            Some(i) => (&name_str[0..i], &name_str[i + 1..]),
            None => (&name_str[0..], ""),
        };

        let mut item = [0; 32];
        let _item = [SPACE; 11]; // invalid name

        // 初始化为 0x20, 0x20 为 ASCII 码中的空格字符; 0x00..0x0B = 0..11
        item[0x00..0x0B].copy_from_slice(&_item);
        // name 的长度可能不足 8 个字节; 0..name.len()
        item[0x00..0x00 + name.len()].copy_from_slice(name.as_bytes());
        // ext 的长度可能不足 3 个字节; 8..extension.len()
        item[0x08..0x08 + extension.len()].copy_from_slice(extension.as_bytes());

        // 将 name 和 ext 部分转换为大写
        //
        // "Short names passed to the file system are always converted to upper case and their original case value is lost"
        //
        item[0x00..0x00 + name.len()].make_ascii_uppercase();
        item[0x08..0x08 + extension.len()].make_ascii_uppercase();

        // Q: 采用小端还是大端序存储数据?
        // A: 采用小端序存储数据, 与 FAT32 文件系统的存储方式一致
        //
        // FAT file system on disk data structure is all "little endian".
        //
        // to_le_bytes() 方法将 u32 类型的数据转换为 小端序 的字节数组
        // eg. 0x12345678 -> [0x78, 0x56, 0x34, 0x12]
        let cluster: [u8; 4] = cluster.to_le_bytes();

        // 0x1A~0x1B 字节为文件内容起始簇号的低两个字节, 与 0x14~0x15 字节处的高两个字节组成文件内容起始簇号
        item[0x14..0x16].copy_from_slice(&cluster[2..4]);
        item[0x1A..0x1C].copy_from_slice(&cluster[0..2]);

        match create_type {
            VirFileType::Dir => item[0x0B] = ATTR_DIRECTORY,
            VirFileType::File => item[0x10] = ATTR_ARCHIVE,
        }

        unsafe { *(item.as_ptr() as *const ShortDirEntry) }
    }

    // All names must check if they have existed in the directory
    pub fn new_from_name_bytes(cluster: u32, name_bytes: &[u8], create_type: VirFileType) -> Self {
        let mut item = [0; 32];
        item[0x00..0x0B].copy_from_slice(name_bytes);

        item[0x00..0x00 + name_bytes.len()].make_ascii_uppercase();

        let mut cluster: [u8; 4] = cluster.to_be_bytes();
        cluster.reverse();

        item[0x14..0x16].copy_from_slice(&cluster[2..4]);
        item[0x1A..0x1C].copy_from_slice(&cluster[0..2]);

        match create_type {
            VirFileType::Dir => item[0x0B] = ATTR_DIRECTORY,
            VirFileType::File => item[0x10] = ATTR_ARCHIVE,
        }

        unsafe { *(item.as_ptr() as *const ShortDirEntry) }
    }

    // All names must check if they have existed in the directory
    pub fn set_name(&mut self, name: &[u8], extension: &[u8]) {
        let mut name_: [u8; 8] = [SPACE; 8];
        name_[0..name.len()].make_ascii_uppercase();
        name_[0..name.len()].copy_from_slice(name);

        let mut extension_: [u8; 3] = [SPACE; 3];
        extension_[0..extension.len()].make_ascii_uppercase();
        extension_[0..extension.len()].copy_from_slice(extension);
        self.name = name_;
    }

    pub fn gen_check_sum(&self) -> u8 {
        let mut name_: [u8; 11] = [0u8; 11];
        let mut sum: u8 = 0;
        for i in 0..8 {
            name_[i] = self.name[i];
        }
        for i in 0..3 {
            name_[i + 8] = self.extension[i];
        }

        for i in 0..11 {
            sum = ((sum & 1) << 7) + (sum >> 1) + name_[i];
        }
        sum
    }

    pub fn name(&self) -> String {
        let name_len = self.name.iter().position(|&x| x == SPACE).unwrap_or(8);
        let ext_len = self.extension.iter().position(|&x| x == SPACE).unwrap_or(3);
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

    pub fn name_bytes_array_with_dot(&self) -> ([u8; 12], usize) {
        let mut len = 0;
        let mut full_name = [0; 12];

        for &i in self.name.iter() {
            if i != SPACE {
                full_name[len] = i;
                len += 1;
            }
        }

        if self.extension[0] != SPACE {
            full_name[len] = b'.';
            len += 1;
        }

        for &i in self.extension.iter() {
            if i != SPACE {
                full_name[len] = i;
                len += 1;
            }
        }

        (full_name, len)
    }

    pub fn name_bytes_array(&self) -> [u8; 11] {
        let mut full_name = [0; 11];
        let mut len = 0;

        for &i in self.name.iter() {
            if i != SPACE {
                full_name[len] = i;
                len += 1;
            }
        }

        for &i in self.extension.iter() {
            if i != SPACE {
                full_name[len] = i;
                len += 1;
            }
        }

        full_name
    }
}

impl ShortDirEntry {
    pub fn empty() -> Self {
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

    pub fn root_dir(cluster: u32) -> Self {
        let mut item = Self::empty();
        item.set_first_cluster(cluster);
        item.attr = ATTR_DIRECTORY;
        item
    }

    pub fn set_name_case(&mut self, state: u8) {
        self.nt_res = state;
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
        self.name[0] == DIR_ENTRY_UNUSED
            || self.name[0] == DIR_ENTRY_LAST_AND_UNUSED
            || self.name[0] == 0x05
    }

    pub fn is_deleted(&self) -> bool {
        self.name[0] == DIR_ENTRY_UNUSED
    }

    pub fn is_empty(&self) -> bool {
        self.name[0] == DIR_ENTRY_LAST_AND_UNUSED
    }

    pub fn is_valid_name(&self) -> bool {
        if self.name[0] < 0x20 {
            return self.name[0] == 0x05;
        } else {
            for i in 0..8 {
                if i < 3 {
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

    pub fn set_attr(&mut self, attr: u8) {
        self.attr = attr;
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
            if self.name[i] == SPACE {
                break;
            } else {
                name.push(self.name[i] as char);
            }
        }
        for i in 0..3 {
            if self.extension[i] == SPACE {
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

    pub fn delete(&mut self) {
        self.file_size = 0;
        self.set_first_cluster(0);
        self.name[0] = DIR_ENTRY_UNUSED;
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut ShortDirEntry as *mut u8, 32) }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self as *const ShortDirEntry as *const u8, 32) }
    }

    pub fn as_bytes_array_mut(&mut self) -> &mut [u8; 32] {
        unsafe { &mut *(self as *mut ShortDirEntry as *mut [u8; 32]) }
    }

    pub fn to_bytes_array(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(self.as_bytes());
        bytes
    }

    pub fn as_bytes_array(&self) -> &[u8; 32] {
        unsafe { &*(self as *const ShortDirEntry as *const [u8; 32]) }
    }

    pub fn new_from_bytes(buf: &[u8]) -> Self {
        unsafe { *(buf.as_ptr() as *const ShortDirEntry) }
    }
}

impl ShortDirEntry {
    pub fn set_create_time(&mut self, time: u16) {
        self.crt_time = time;
    }

    pub fn set_create_date(&mut self, date: u16) {
        self.crt_date = date;
    }

    pub fn set_last_access_date(&mut self, date: u16) {
        self.lst_acc_date = date;
    }

    pub fn set_last_write_time(&mut self, time: u16) {
        self.wrt_time = time;
    }

    pub fn set_last_write_date(&mut self, date: u16) {
        self.wrt_date = date;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(packed)]
/// Long Directory Entry
///
/// 1 + 2*5 + 1 + 1 + 2 + 2*6 + 2 + 2*2 = 32 bytes
//
//  TODO: Name charactor check
pub struct LongDirEntry {
    /// The order of this entry in the sequence of long dir entries.
    /// It is associated with the short dir entry at the end of the long dir set,
    /// and masked with 0x40 (`LAST_LONG_ENTRY`),
    /// which indicates that the entry is the last long dir entry in a set of long dir entries.
    /// All valid sets of long dir entries must begin with an entry having this mask.
    ///
    /// DO NOT misunderstand the meaning of the mask(0x40).
    /// This mask should be for ord in the same file. The long
    /// file name of a long directory entry only has 13 unicode
    /// characters. When the file name exceeds 13 characters,
    /// multiple long directory entries are required.
    ///
    /// Long Dir Entry Order   size: 1 byte    offset: 0 (0x00)
    //
    //  长文件名目录项的序列号, 一个文件的第一个目录项序列号为 1, 然后依次递增. 如果是该文件的
    //  最后一个长文件名目录项, 则将该目录项的序号与 0x40 进行 "或 (OR) 运算"的结果写入该位置.
    //  如果该长文件名目录项对应的文件或子目录被删除, 则将该字节设置成删除标志0xE5.
    //
    //  Mask(0x40)针对同一个文件中的 ord, 一个长目录项的长文件名仅有 13 个 unicode字符,
    //  当文件名超过13个字符时, 需要多个长目录项
    ord: u8,
    /// Characters 1-5 of the long-name sub-component in this dir entry.
    /// CharSet: Unicode. Codeing: UTF-16LE
    ///
    /// Long Dir Entry Name 1  size: 10 bytes  offset: 1 (0x01~0x0A)
    //
    //  长文件名的第 1~5 个字符. 长文件名使用 Unicode 码, 每个字符需要两个字节的空间.
    //  如果文件名结束但还有未使用的字节, 则会在文件名后先填充两个字节的 "00", 然后开始使用 0xFF 填充
    name1: [u16; 5],
    /// Attributes - must be ATTR_LONG_NAME
    ///
    /// Long Dir Entry Attributes   size: 1 byte    offset: 11 (0x0B)
    //
    //  长目录项的属性标志, 一定是 0x0F
    attr: u8,
    /// If zero, indicates a directory entry that is a sub-component of a long name.
    /// Other values reserved for future extensions.
    /// Non-zero implies other dirent types.
    ///
    /// Long Dir Entry Type    size: 1 byte    offset: 12 (0x0C)   value: 0 (sub-component of long name)
    ldir_type: u8,
    /// Checksum of name in the short dir entry at the end of the long dir set.
    ///
    /// Checksum      size: 1 byte    offset: 13 (0x0D)
    //
    //  校验和. 如果一个文件的长文件名需要几个长文件名目录项进行存储, 则这些长文件名目录项具有相同的校验和.
    chk_sum: u8,
    /// Characters 6-11 of the long-name sub-component in this dir entry.
    /// CharSet: Unicode. Codeing: UTF-16LE
    ///
    /// Long Dir Entry Name 2  size: 12 bytes  offset: 14 (0x0E~0x19)
    ///
    //  文件名的第 6~11 个字符, 未使用的字节用 0xFF 填充
    name2: [u16; 6],
    /// Must be ZERO.
    /// This is an artifact of the FAT "first cluster",
    /// and must be zero for compatibility with existing disk utilities.
    /// It's meaningless in the context of a long dir entry.
    ///
    /// Long Dir Entry First Cluster Low   size: 2 bytes   offset: 26 (Ox1A~0x1B)     value: 0
    fst_clus_lo: u16,
    /// Characters 12-13 of the long-name sub-component in this dir entry.
    /// CharSet: Unicode. Codeing: UTF-16LE
    ///
    /// Long Dir Entry Name 3  size: 4 bytes   offset: 28 (0x1C~0x1F)
    //
    //  文件名的第 12~13 个字符, 未使用的字节用 0xFF 填充
    name3: [u16; 2],
}

impl LongDirEntry {
    pub fn new_form_name_slice(order: u8, name_array: [u16; 13], check_sum: u8) -> Self {
        let mut lde = Self::empty();

        unsafe {
            core::ptr::addr_of_mut!(lde.name1)
                // try_into() 被用来尝试将 partial_name[..5] 转换成一个大小为 5 的固定大小数组
                .write_unaligned(name_array[..5].try_into().expect("Failed to cast!"));
            core::ptr::addr_of_mut!(lde.name2)
                .write_unaligned(name_array[5..11].try_into().expect("Failed to cast!"));
            core::ptr::addr_of_mut!(lde.name3)
                .write_unaligned(name_array[11..].try_into().expect("Failed to cast!"));
        }

        lde.ord = order;
        lde.chk_sum = check_sum;

        lde
    }

    pub fn set_name(&mut self, name_array: [u16; 13]) {
        unsafe {
            core::ptr::addr_of_mut!(self.name1)
                // try_into() 被用来尝试将 partial_name[..5] 转换成一个大小为 5 的固定大小数组
                .write_unaligned(name_array[..5].try_into().expect("Failed to cast!"));
            core::ptr::addr_of_mut!(self.name2)
                .write_unaligned(name_array[5..11].try_into().expect("Failed to cast!"));
            core::ptr::addr_of_mut!(self.name3)
                .write_unaligned(name_array[11..].try_into().expect("Failed to cast!"));
        }
    }

    pub fn new(order: u8, check_sum: u8, name_str: &str) -> Self {
        let mut buf = [0; 32];
        buf[0x00] = order;
        buf[0x0B] = ATTR_LONG_NAME;
        buf[0x0D] = check_sum;
        Self::write_unicode(name_str, &mut buf);
        Self::new_form_bytes(&buf)
    }

    pub fn name(&self) -> String {
        let name_all = self.name_utf16();
        let len = (0..name_all.len())
            .find(|i| name_all[*i] == 0)
            .unwrap_or(name_all.len());

        // 从 UTF-16 编码的字节数组中解码出字符串
        String::from_utf16_lossy(&name_all[..len])
    }

    pub fn name_utf16(&self) -> [u16; LONG_NAME_LEN_CAP] {
        let mut name_all: [u16; LONG_NAME_LEN_CAP] = [0u16; LONG_NAME_LEN_CAP];

        name_all[..5].copy_from_slice(unsafe { &core::ptr::addr_of!(self.name1).read_unaligned() });
        name_all[5..11]
            .copy_from_slice(unsafe { &core::ptr::addr_of!(self.name2).read_unaligned() });
        name_all[11..]
            .copy_from_slice(unsafe { &core::ptr::addr_of!(self.name3).read_unaligned() });

        name_all
    }
}

impl LongDirEntry {
    pub fn empty() -> Self {
        Self {
            ord: 0u8,
            name1: [0u16; 5],
            attr: ATTR_LONG_NAME,
            ldir_type: 0u8,
            chk_sum: 0u8,
            name2: [0u16; 6],
            fst_clus_lo: 0u16,
            name3: [0u16; 2],
        }
    }

    pub fn new_form_bytes(buf: &[u8]) -> Self {
        unsafe { *(buf.as_ptr() as *const Self) }
    }

    pub fn attr(&self) -> u8 {
        self.attr
    }

    pub fn order(&self) -> u8 {
        self.ord
    }

    pub fn check_sum(&self) -> u8 {
        self.chk_sum
    }

    pub fn is_free(&self) -> bool {
        self.ord == DIR_ENTRY_LAST_AND_UNUSED || self.ord == DIR_ENTRY_UNUSED
    }

    pub fn is_empty(&self) -> bool {
        self.ord == DIR_ENTRY_LAST_AND_UNUSED
    }

    pub fn is_valid(&self) -> bool {
        self.ord != DIR_ENTRY_UNUSED
    }

    pub fn is_deleted(&self) -> bool {
        self.ord == DIR_ENTRY_UNUSED
    }

    pub fn delete(&mut self) {
        self.ord = DIR_ENTRY_UNUSED;
    }

    fn write_unicode(value: &str, buf: &mut [u8]) {
        let mut temp = [0xFF; 26];
        let mut index = 0;

        for i in value.encode_utf16() {
            // u16 低 8 位
            let part1 = (i & 0xFF) as u8;
            // u16 高 8 位
            let part2 = ((i & 0xFF00) >> 8) as u8;
            temp[index] = part1;
            temp[index + 1] = part2;
            index += 2;
        }

        //  如果文件名结束但还有未使用的字节, 则会在文件名后先填充两个字节的 "00", 然后开始使用 0xFF 填充
        if index != 26 {
            temp[index] = 0;
            temp[index + 1] = 0;
        }

        index = 0;

        let mut op = |start: usize, end: usize| {
            for i in (start..end).step_by(2) {
                buf[i] = temp[index];
                buf[i + 1] = temp[index + 1];
                index += 2;
            }
        };

        op(0x01, 0x0A);
        op(0x0E, 0x19);
        op(0x1C, 0x1F);
    }

    fn name_to_utf8(&self) -> ([u8; 13 * 3], usize) {
        let (mut utf8, mut len) = ([0; 13 * 3], 0);

        let mut op = |parts: &[u16]| {
            for i in 0..parts.len() {
                let unicode: u16 = parts[i];
                if unicode == 0 || unicode == 0xFFFF {
                    break;
                }

                // UTF-16 转 UTF-8 编码
                // UTF-8 编码的规则:
                // 如果代码点在 0x80 以下 (即 ASCII 字符), 则使用 1 个字节的编码表示, 即 0xxxxxxx (其中 x 表示可用的位)
                // 如果代码点在 0x80 到 0x7FF 之间, 则使用 2 个字节的编码表示, 即 110xxxxx 10xxxxxx.
                // 如果代码点在 0x800 到 0xFFFF 之间, 则使用 3 个字节的编码表示, 即 1110xxxx 10xxxxxx 10xxxxxx
                // 如果代码点在 0x10000 到 0x10FFFF 之间, 则使用 4 个字节的编码表示, 即 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
                if unicode <= 0x007F {
                    utf8[len] = unicode as u8;
                    len += 1;
                } else if unicode >= 0x0080 && unicode <= 0x07FF {
                    let part1 = (0b11000000 | (0b00011111 & (unicode >> 6))) as u8;
                    let part2 = (0b10000000 | (0b00111111) & unicode) as u8;

                    utf8[len] = part1;
                    utf8[len + 1] = part2;
                    len += 2;
                } else if unicode >= 0x0800 {
                    let part1 = (0b11100000 | (0b00011111 & (unicode >> 12))) as u8;
                    let part2 = (0b10000000 | (0b00111111) & (unicode >> 6)) as u8;
                    let part3 = (0b10000000 | (0b00111111) & unicode) as u8;

                    utf8[len] = part1;
                    utf8[len + 1] = part2;
                    utf8[len + 2] = part3;
                    len += 3;
                }
            }
        };

        unsafe {
            op(&core::ptr::addr_of!(self.name1).read_unaligned());
            op(&core::ptr::addr_of!(self.name2).read_unaligned());
            op(&core::ptr::addr_of!(self.name3).read_unaligned());
        }

        (utf8, len)
    }

    // The mask should be for ord in the same file. The long
    // file name of a long directory entry only has 13 unicode
    // characters. When the file name exceeds 13 characters,
    // multiple long directory entries are required.
    pub fn lde_order(&self) -> usize {
        (self.ord & (LAST_LONG_ENTRY - 1)) as usize
    }

    pub fn is_lde_end(&self) -> bool {
        (self.ord & LAST_LONG_ENTRY) == LAST_LONG_ENTRY
    }

    pub fn as_bytes_array(&self) -> [u8; 32] {
        unsafe { core::ptr::read_unaligned(self as *const Self as *const [u8; 32]) }
    }

    pub fn as_bytes_array_mut(&mut self) -> &mut [u8; 32] {
        unsafe { &mut *(self as *mut Self as *mut [u8; 32]) }
    }

    pub fn to_bytes_array(&self) -> [u8; 32] {
        let mut buf = [0; 32];
        buf.copy_from_slice(self.as_bytes());
        buf
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, 32) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut Self as *mut u8, 32) }
    }
}

pub(crate) enum NameType {
    SFN,
    LFN,
}
