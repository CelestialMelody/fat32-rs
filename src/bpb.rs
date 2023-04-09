//! BIOS Parameter Block (BPB) and Boot Sector
//!
//! The first important data structure on a FAT volume is called the BPB (BIOS Parameter Block), which
//! is located in the first sector of the volume in the Reserved Region. This sector is sometimes called the
//! "boot sector" or the "reserved sector" or the "0th sector", but the important fact is simply that it is the
//! first sector of the volume.
//!
//! [`BIOSParameterBlock`] is the main structure of this module. It contains the [`BPB`] and [`BPB32`] fields.
//!
//! FAT Data Structure
//!
//! The next data structure that is important is the FAT itself. What this data structure does is define a
//! singly linked list of the “extents” (clusters) of a file. Note at this point that a FAT directory or file
//! container is nothing but a regular file that has a special attribute indicating it is a directory. The only
//! other special thing about a directory is that the data or contents of the “file” is a series of 32=byte FAT
//! directory entries (see discussion below). In all other respects, a directory is just like a file. The FAT
//! maps the data region of the volume by cluster number. The first data cluster is cluster 2.
//!
//! Functions implemented for [`BIOSParameterBlock`] are:
//! - [`BIOSParameterBlock::offset()`]: Get the first sector offset bytes of the cluster from the cluster number
//! - [`BIOSParameterBlock::first_data_sector`]: The first data sector beyond the root directory
//! - [`BIOSParameterBlock::first_sector_of_cluster`]: The first sector of the cluster
//! - [`BIOSParameterBlock::root_dir_sector_cnt`]: The first sector of the root directory
//! - [`BIOSParameterBlock::data_sector_cnt`]: The number of sectors in the data region
//! - [`BIOSParameterBlock::data_cluster_cnt`]: The number of clusters in the data region
//!
//! Note that the CountofClusters value is exactly that -- the count of data clusters starting at cluster 2.
//! The maximum valid cluster number for the volume is CountofClusters + 1, and the "count of clusters
//! including the two reserved clusters" is CountofClusters + 2.
//!
//! We intend to realize fat32, so we don't need to care about fat12 and fat16.
//! But we still reserve the fields of fat12 and fat16 for future maybe. See the [`BPB12_16`] and [`FatType`].
//!
//! FAT type Definitions
//!
//! See the realizetion in [`bpb::BIOSParameterBlock::fat_type()`]

#![allow(unused)]

use crate::{MAX_CLUSTER_FAT12, MAX_CLUSTER_FAT16};

/// BIOS Parameters
/// *On-disk* data structure for partition information.
#[derive(Debug, Copy, Clone)]
// repr(packed) 表示使用紧凑的表示方式来表示一个结构体或枚举, 编译器不会在字段间填充字节
// 使用 #[repr(packed)] 属性可能会导致访问未对齐的内存，这可能会导致不可预测的结果，例如内存访问异常、程序崩溃等
#[repr(packed)]
pub struct BIOSParameterBlock {
    pub(crate) bpb: BPB,
    pub(crate) bpb32: BPB32,
}

/// We intend to realize fat32, so we don't need to care about fat12 and fat16.
/// But we still reserve the fields of fat12 and fat16 for future maybe.
pub enum FatType {
    FAT32,
    FAT16,
    FAT12,
}

// TODO: u32 or usize?
impl BIOSParameterBlock {
    #[inline(always)]
    /// Get the first sector offset bytes of the cluster from the cluster number
    pub fn offset(&self, cluster: u32) -> usize {
        // Q: why cluster - 2?
        // A: The first two clusters are reserved for the root directory
        //    and he first data cluster is 2
        ((self.bpb.rsvd_sec_cnt as usize)
            + (self.bpb.num_fats as usize) * (self.bpb32.fat_sz32 as usize)
            + (cluster as usize - 2) * (self.bpb.sec_per_clus as usize))
            * (self.bpb.byts_per_sec as usize)
    }

    #[inline(always)]
    /// The first data sector beyond the root directory
    ///
    /// The start of the data region, the first sector of cluster 2, is computed as follows:
    pub fn first_data_sector(&self) -> usize {
        // let mut fat_sz: usize = 0;
        // if self.bpb.fat_sz16 != 0 {
        //     fat_sz = self.bpb.fat_sz16 as usize;
        // } else {
        //     fat_sz = self.bpb32.fat_sz32 as usize;
        // }
        // (self.bpb.rsvd_sec_cnt as usize)
        //     + (self.bpb.num_fats as usize) * fat_sz
        //     + self.root_dir_sector_cnt()

        (self.bpb.rsvd_sec_cnt as usize)
            + (self.bpb.num_fats as usize) * self.bpb32.fat_sz32 as usize
            + self.root_dir_sector_cnt()
    }

    #[inline(always)]
    /// Given any valid data cluster number N, the sector number of the first sector of that cluster
    /// (again relative to sector 0 of the FAT volume) is computed as follows.
    pub fn first_sector_of_cluster(&self, cluster: u32) -> usize {
        self.first_data_sector() + (cluster as usize - 2) * self.bpb.sec_per_clus as usize
    }

    #[inline(always)]
    /// Get FAT1 Offset
    pub fn fat1(&self) -> usize {
        (self.bpb.rsvd_sec_cnt as usize) * (self.bpb.byts_per_sec as usize)
    }

    /// Get sector_per_cluster_usize as usize value
    pub fn sector_per_cluster_usize(&self) -> usize {
        self.bpb.sec_per_clus as usize
    }

    #[inline(always)]
    /// Sectors occupied by the root directory
    ///
    /// Note that on a FAT32 volume, the BPB_RootEntCnt value is always 0; so on a FAT32 volume,
    /// RootDirSectors is always 0.
    /// The 32 in the above is the size of one FAT directory entry in bytes.
    /// Note also that this computation rounds up
    pub fn root_dir_sector_cnt(&self) -> usize {
        ((self.bpb.root_ent_cnt * 32) as usize + (self.bpb.byts_per_sec - 1) as usize)
            / self.bpb.byts_per_sec as usize
    }

    #[inline(always)]
    /// Total sectors of the data region
    pub fn data_sector_cnt(&self) -> usize {
        // let mut fat_sz: usize = 0;
        // if self.bpb.fat_sz16 != 0 {
        //     fat_sz = self.bpb.fat_sz16 as usize;
        // } else {
        //     fat_sz = self.bpb32.fat_sz32 as usize;
        // }
        // let mut tot_sec: usize = 0;
        // if self.bpb.tot_sec16 != 0 {
        //     tot_sec = self.bpb.tot_sec16 as usize;
        // } else {
        //     tot_sec = self.bpb.tot_sec32 as usize;
        // }
        // tot_sec
        //     - (self.bpb.rsvd_sec_cnt as usize)
        //     - (self.bpb.num_fats as usize) * fat_sz
        //     - self.root_dir_sector_cnt()

        self.bpb.tot_sec32 as usize
            - (self.bpb.rsvd_sec_cnt as usize)
            - (self.bpb.num_fats as usize) * (self.bpb32.fat_sz32 as usize)
            - self.root_dir_sector_cnt()
    }

    /// The count of (data) clusters
    ///
    /// This function should round DOWN.
    #[inline(always)]
    pub fn data_cluster_cnt(&self) -> usize {
        self.data_sector_cnt() / (self.bpb.sec_per_clus as usize)
    }

    pub fn is_valid(&self) -> bool {
        self.bpb.root_ent_cnt == 0
            && self.bpb.tot_sec16 == 0
            && self.bpb.tot_sec32 != 0
            && self.bpb.fat_sz16 == 0
            && self.bpb32.fat_sz32 != 0
    }

    #[inline(always)]
    pub fn cluster_size(&self) -> usize {
        self.bpb.sec_per_clus as usize * self.bpb.byts_per_sec as usize
    }

    pub fn fat_type(&self) -> FatType {
        if self.data_cluster_cnt() < MAX_CLUSTER_FAT12 {
            FatType::FAT12
        } else if self.data_cluster_cnt() < MAX_CLUSTER_FAT16 {
            FatType::FAT16
        } else {
            FatType::FAT32
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(packed)]
/// Boot Sector and BPB Structure For FAT12/16/32
pub struct BPB {
    //  0x00~0x02 3个字节: 跳转指令与空值指令.
    //
    /// x86 assembly to jump instruction to boot code.
    //
    //  跳转指令与空值指令    大小: 3字节    值: 0xEB(跳转指令) ??(跳转地址) 0x90(空指令)    偏移: 0x00
    pub(crate) bs_jmp_boot: [u8; 3],

    //  0x03~0x0A 8个字节: OEM名称
    //
    /// It is only a name string.
    //
    //  OEM名称    大小: 8字节    值: ???    偏移: 0x03
    pub(crate) bs_oem_name: [u8; 8],

    //  从 0x0B 开始的79个字节的数据叫做 BPB (BIOS Paramter Block)
    //
    /// Bytes per sector, This value may take on only the
    /// following values: 512, 1024, 2048 or 4096. 512 for SD card
    //
    // 	每扇区字数    大小: 2字节    值: 512 (0x200)    偏移: 0x0B
    pub(crate) byts_per_sec: u16,
    /// Sector per cluster. Number of sectors per allocation unit. This value
    /// must be a power of 2 that is greater than 0. The legal values are
    /// 1, 2, 4, 8, 16, 32, 64, and 128.Note however, that a value should
    /// never be used that results in a “bytes per cluster” value
    /// (BPB_BytsPerSec * BPB_SecPerClus) greater than 32K (32 * 1024).
    /// Usually 8 for SD card.
    //
    //  每簇扇区数    大小: 1字节    值: 8 (0x08)    偏移: 0x0D
    pub(crate) sec_per_clus: u8,
    /// Sector number of the reserved area.
    /// Number of reserved sectors in the Reserved region of the volume
    /// starting at the first sector of the volume.
    /// For FAT32 volumes, this value is typically 32.
    //
    //  保留扇区数    大小: 2字节    值: 32 (0x20)    偏移: 0x0E
    pub(crate) rsvd_sec_cnt: u16,
    /// Number of FATs
    /// This field should always contain the value 2 for any FAT
    /// volume of any type.
    //
    //  FAT表数      大小: 1字节    值: 2 (0x02)    偏移: 0x10
    pub(crate) num_fats: u8,
    /// For FAT32 volumes, this field must be set to 0.
    //
    //  根目录最大文件数 (最大目录项个数) (FAT32 已经突破限制, 无效)    大小: 2字节    值: 0 (0x00)   偏移: 0x11
    pub(crate) root_ent_cnt: u16,
    /// For FAT32 volumes, this field must be 0.
    /// If it is 0, then BPB_TotSec32 must be non-zero.
    //
    //  扇区总数 (给FAT12/16使用)    大小: 2字节    值: 0 (0x00)   偏移: 0x13
    pub(crate) tot_sec16: u16,
    /// Used to denote the media type. This is a legacy field that is no longer
    /// in use. 0xF8 is the standard value for “fixed” (non-removable) media.
    /// For removable media, 0xF0 is frequently used. The legal values for this
    /// field are: 0xF0, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, and 0xFF.
    /// The only other important point is that whatever value is put in here must
    /// also be put in the low byte of the FAT[0] entry.
    //
    //  媒体描述符    大小: 1字节    值: 0xF8 (0xF8)   偏移: 0x15
    pub(crate) media: u8,
    /// On FAT32 volumes this field must be 0, and fat_sz32 contains the FAT size count.
    //
    //  每个FAT扇区数 (给FAT12/16使用)    大小: 2字节    值: 0 (0x00)    偏移: 0x16
    pub(crate) fat_sz16: u16,
    /// Sector per track used by interrupt 0x13.
    /// Not needed by SD card.
    //
    // 每磁道扇区数    大小: 2字节    值: 0 (0x00)    偏移: 0x18
    pub(crate) sec_per_trk: u16,
    /// Number of heads for interrupt 0x13.
    /// This field is relevant as discussed earlier for BPB_SecPerTrk.
    /// This field contains the one based “count of heads”.
    /// Not needed by SD card.
    //
    //  磁头数      大小: 2字节    值: 0 (0x00)   偏移: 0x1A
    pub(crate) num_heads: u16,
    /// Count of hidden sectors preceding the partition that contains this
    /// FAT volume. This field is generally only relevant for media visible
    /// on interrupt 0x13. This field should always be zero on media that
    /// are not partitioned. Exactly what value is appropriate is operating
    /// system specific.
    //
    //  隐藏扇区数    大小: 4字节    值: 0 (0x00)   偏移: 0x1C
    pub(crate) hidd_sec: u32,
    /// This field is the new 32-bit total count of sectors on the volume.
    /// This count includes the count of all sectors in all four regions of the
    /// volume. For FAT32 volumes, this field must be non-zero.
    //
    //  扇区总数 (给FAT32使用)    大小: 4字节    值: non-zero   偏移: 0x20
    pub(crate) tot_sec32: u32,
}

impl BPB {
    pub(crate) fn bytes_per_sector(&self) -> u32 {
        self.byts_per_sec as u32
    }
    pub(crate) fn sector_per_cluster(&self) -> u32 {
        self.sec_per_clus as u32
    }
    pub(crate) fn fat_cnt(&self) -> u32 {
        self.num_fats as u32
    }
    pub(crate) fn reserved_sector_cnt(&self) -> u32 {
        self.rsvd_sec_cnt as u32
    }
    pub(crate) fn sector_pre_fat32(&self) -> u32 {
        self.tot_sec32
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(packed)]
/// Boot Sector and BPB Structure For FAT32.
/// FAT32 Structure Starting at Offset 36B (0x24B)
pub struct BPB32 {
    // 从 0x24B 开始的余下的 54 个字节对于 FAT32 来说的 BPB
    //
    /// This field is the FAT32 32-bit count of sectors occupied by
    /// ONE FAT. BPB_FATSz16 must be 0.
    //
    // 每个FAT扇区数 (给FAT32使用)    大小: 4字节    值: non-zero  偏移: 0x24
    fat_sz32: u32,
    /// This field is only defined for FAT32 media and does not exist on
    /// FAT12 and FAT16 media.
    /// Bits 0-3    -- Zero-based number of active FAT. Only valid if mirroring
    ///                is disabled.
    /// Bits 4-6    -- Reserved.
    /// Bit 7       -- 0 means the FAT is mirrored at runtime into all FATs.
    ///             -- 1 means only one FAT is active; it is the one referenced
    ///                in bits 0-3.
    /// Bits 8-15   -- Reserved.
    //
    // 扩展标志    大小: 2字节    值: ???    偏移: 0x28
    ext_flags: u16,
    /// This is the version number of the FAT32 volume.
    /// This field is only defined for FAT32 media. High byte is major
    /// revision number. Low byte is minor revision number.
    /// Disk utilities should respect this field and not operate on
    /// volumes with a higher major or minor version number than that for
    /// which they were designed. FAT32 file system drivers must check
    /// this field and not mount the volume if it does not contain a version
    /// number that was defined at the time the driver was written.
    //
    //  文件系统版本 (通常为零)    大小: 2字节    值: 0x0000 (0x0000)   偏移: 0x2A
    fs_ver: u16,
    /// s is set to the cluster number of the first cluster of the root
    /// directory, usually 2 but not required to be 2.
    //
    //  根目录起始簇号    大小: 4字节    值: 2 (0x02)    偏移: 0x2C
    root_clus: u32,
    /// Sector number of FSINFO structure in the reserved area of
    /// the FAT32 volume. Usually 1.
    //
    //  FSINFO 扇区号 (Boot占用扇区数)    大小: 2字节    值: 1 (0x01)   偏移: 0x30
    fs_info: u16,
    /// The sector number in the reserved area of the volume of
    /// a copy of the boot record. Usually 6.
    //
    //  备份引导扇区扇区号    大小: 2字节    值: 6 (0x06)   偏移: 0x32
    bk_boot_sec: u16,
    /// Reserved for future expansion. Code that formats FAT32 volumes
    /// should always set all of the bytes of this field to 0.
    //
    //  保留区    大小: 12字节    值: 0 (0x00)    偏移: 0x34
    reserved: [u8; 12],
    //  以下 26B 与 FAT12/16 的 BPB 完全相同, 但是偏移不同
    //
    /// This field is the physical drive number for the INT 13h.
    /// This field has the same definition as it does for FAT12 and FAT16
    /// media. The only difference for FAT32 media is that the field is at a
    /// different offset in the boot sector.
    //
    //  物理驱动器号    大小: 1字节    值: 0x80    偏移: 0x40
    bs_drv_num: u8,
    /// This field is no longer used and should always be set to 0.
    //
    //  保留区    大小: 1字节    值: 0 (0x00)    偏移: 0x41
    bs_reserved1: u8,
    /// This field is the extended boot signature. This field is set to 0x29.
    /// This is a signature byte that indicates that the following three fields
    /// in the boot sector are present. (BS_VolID, BS_VolLab, BS_FilSysType)
    //
    //  扩展引导标记    大小: 1字节    值: 0x29 (0x29)   偏移: 0x42
    bs_boot_sig: u8,
    /// Volume serial number. This field, together with BS_VolLab,
    /// supports volume tracking on removable media. These values allow
    /// FAT file system drivers to detect that the wrong disk is inserted in a
    /// removable drive. This ID is usually generated by simply combining
    /// the current date and time into a 32-bit value.
    //
    //  卷序列号    大小: 4字节    值: ???    偏移: 0x43
    bs_vol_id: u32,
    /// Volume label. This field matches the 11-byte volume label recorded in
    /// the root directory.
    /// NOTE: FAT file system drivers should make sure that they update
    /// this field when the volume label file in the root directory has its
    /// name changed or created. The setting for this field when there is no
    /// volume label is the string "NO NAME    ".
    //
    //  卷标    大小: 11字节    值: ???    偏移: 0x47
    bs_vol_lab: [u8; 11],
    /// File system type.
    /// This string is informational only and is not used by Microsoft
    /// file system drivers to determine FAT typ,e because it is frequently
    /// not set correctly or is not present.
    //
    //  文件系统类型    大小: 8字节    值: "FAT32   "    偏移: 0x52
    bs_fil_sys_type: [u8; 8],
}

impl BPB32 {
    // Number of sectors occupied by FAT
    pub(crate) fn sector_per_fat(&self) -> u32 {
        self.fat_sz32
    }

    // Get the sector number of FSInfo
    pub(crate) fn fat_info_sector(&self) -> u32 {
        self.fs_info as u32
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(packed)]
#[allow(dead_code)]
/// Boot Sector and BPB Structure For FAT32.
/// FAT12/16 Structure Starting at Offset 36B (0x24B)
pub struct BPB12_16 {
    bs_drv_num: u8,
    bs_reserved1: u8,
    bs_boot_sig: u8,
    bs_vol_id: u32,
    bs_vol_lab: [u8; 11],
    bs_fil_sys_type: [u8; 8],
}
