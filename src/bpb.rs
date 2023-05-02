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
//! singly linked list of the "extents" (clusters) of a file. Note at this point that a FAT directory or file
//! container is nothing but a regular file that has a special attribute indicating it is a directory. The only
//! other special thing about a directory is that the data or contents of the "file" is a series of 32=byte FAT
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
//! Given any valid data cluster number N, the sector number of the first sector of that cluster (again
//! relative to sector 0 of the FAT volume) is computed as follows:
//!     FirstSectorofCluster = ((N – 2) * BPB_SecPerClus) + FirstDataSector
//!
//! We intend to realize fat32, so we don't need to care about fat12 and fat16.
//! But we still reserve the fields of fat12 and fat16 for future maybe. See the [`BPB12_16`] and [`FatType`].
//!
//! FAT type Definitions
//!
//! See the realizetion in [`bpb::BIOSParameterBlock::fat_type()`]
//!
//! Another feature on FAT32 volumes that is not present on FAT16/FAT12 is the BPB_BkBootSec field.
//! FAT16/FAT12 volumes can be totally lost if the contents of sector 0 of the volume are overwritten or
//! sector 0 goes bad and cannot be read. This is a "single point of failure" for FAT16 and FAT12
//! volumes. The BPB_BkBootSec field reduces the severity of this problem for FAT32 volumes, because
//! starting at that sector number on the volume-6-there is a backup copy of the boot sector
//! information including the volume's BPB.

//! FAT File System Layout:
//!      Boot Sector - Reserved Sectors - FAT1 - FAT2 - (FAT32 without Root Directory Region) - Data Region
//! Note:
//!     1. Reserved Sectors include the Boot Sector, Boot Sector include the BPB and the FSInfo(structure)
//!     2. fat1_offset = reserved_sector_count * bytes_per_sector
//!     3. The file allocation table area contains two identical file allocation tables, because the storage space
//!        (cluster chain) occupied by the file and the management of the free space are implemented by FAT,
//!         and two are saved in case the first one is damaged, and the second one is available.
//!

//! FAT type Definitions
//! The one and only way that FAT type is determined.
//!
//! There is no such thing as a FAT12 volume that has more than 4084 clusters.
//! There is no such thing as a FAT16 volume that has less than 4085 clusters or more than 65,524 clusters.
//! There is no such thing as a FAT32 volume that has less than 65,525 clusters.
//! If you try to make a FAT volume that violates this rule, Microsoft operating systems
//! will not handle them correctly because they will think the volume has a different type of FAT than
//! what you think it does.
//!
//! See the realizetion in [`BIOSParameterBlock::fat_type()`]
//!
//! A FAT32 FAT entry is actually only a 28-bit entry. The high 4 bits of a FAT32 FAT entry are reserved.
//! The only time that the high 4 bits of FAT32 FAT entries should ever be changed is when the volume is formatted,
//! at which time the whole 32-bit FAT entry should be zeroed, including the high 4 bits.
//!
//! A bit more explanation is in order here, because this point about FAT32 FAT entries seems to cause a
//! great deal of confusion. Basically 32-bit FAT entries are not really 32-bit values; they are only 28-bit
//! values. For example, all of these 32-bit cluster entry values: 0x10000000, 0xF0000000, and
//! 0x00000000 all indicate that the cluster is FREE, because you ignore the high 4 bits when you read
//! the cluster entry value. If the 32-bit free cluster value is currently 0x30000000 and you want to mark
//! this cluster as bad by storing the value 0x0FFFFFF7 in it. Then the 32-bit entry will contain the value
//! 0x3FFFFFF7 when you are done, because you must preserve the high 4 bits when you write in the
//! 0x0FFFFFF7 bad cluster mark.
//!
//! Take note that because the BPB_BytsPerSec value is always divisible by 2 and 4, you never have to
//! worry about a FAT16 or FAT32 FAT entry spanning over a sector boundary (this is not true of FAT12).
//!
//! The way the data of a file is associated with the file is as follows. In the directory entry, the cluster
//! number of the first cluster of the file is recorded. The first cluster (extent) of the file is the data
//! associated with this first cluster number, and the location of that data on the volume is computed from
//! the cluster number as described earlier (computation of FirstSectorofCluster).
//!
//! Note that a zero-length file-a file that has no data allocated to it-has a first cluster number of 0
//! placed in its directory entry. This cluster location in the FAT (see earlier computation of
//! ThisFATSecNum and ThisFATEntOffset) contains either an EOC mark (End Of Clusterchain) or the
//! cluster number of the next cluster of the file. The EOC value is FAT type dependant (assume
//! FATContent is the contents of the cluster entry in the FAT being checked to see whether it is an EOC mark)
//!
//! Note that the cluster number whose cluster entry in the FAT contains the EOC mark is allocated to the
//! file and is also the last cluster allocated to the file.
//!
//! There is also a special "BAD CLUSTER" mark. Any cluster that contains the "BAD CLUSTER"
//! value in its FAT entry is a cluster that should not be placed on the free list because it is prone to disk
//! errors. The "BAD CLUSTER" value is 0x0FF7 for FAT12, 0xFFF7 for FAT16, and 0x0FFFFFF7 for
//! FAT32. The other relevant note here is that these bad clusters are also lost clusters-clusters that
//! appear to be allocated because they contain a non-zero value but which are not part of any files
//! allocation chain. Disk repair utilities must recognize lost clusters that contain this special value as bad
//! clusters and not change the content of the cluster entry.
//!
//! It is not possible for the bad cluster mark to be an allocatable cluster number on FAT12 and
//! FAT16 volumes, but it is feasible for 0x0FFFFFF7 to be an allocatable cluster number on FAT32
//! volumes. To avoid possible confusion by disk utilities, no FAT32 volume should ever be configured
//! such that 0x0FFFFFF7 is an allocatable cluster number.
//!
//! What are the two reserved clusters at the start of the FAT for? The first reserved cluster, FAT[0],
//! contains the BPB_Media byte value in its low 8 bits, and all other bits are set to 1. For example, if the
//! BPB_Media value is 0xF8, for FAT12 FAT[0] = 0x0FF8, for FAT16 FAT[0] = 0xFFF8, and for
//! FAT32 FAT[0] = 0x0FFFFFF8. The second reserved cluster, FAT[1], is set by FORMAT to the EOC
//! mark. On FAT12 volumes, it is not used and is simply always contains an EOC mark. For FAT16 and
//! FAT32, the file system driver may use the high two bits of the FAT[1] entry for dirty volume flags (all
//! other bits, are always left set to 1). Note that the bit location is different for FAT16 and FAT32,
//! because they are the high 2 bits of the entry.
//!
//! Bit ClnShutBitMask -- If bit is 1, volume is "clean". If bit is 0, volume is "dirty".
//! Bit HrdErrBitMask  -- If this bit is 1, no disk read/write errors were encountered.
//!     If this bit is 0, the file system driver encountered a disk I/O error on the Volume
//!     the last time it was mounted, which is an indicator that some sectors may have gone bad on the volume.
//!
//! Here are two more important notes about the FAT region of a FAT volume:
//! 1. The last sector of the FAT is not necessarily all part of the FAT. The FAT stops at the cluster
//!    number in the last FAT sector that corresponds to the entry for cluster number
//!    CountofClusters + 1 (see the CountofClusters computation earlier), and this entry is not
//!    necessarily at the end of the last FAT sector. FAT code should not make any assumptions
//!    about what the contents of the last FAT sector are after the CountofClusters + 1 entry. FAT
//!    format code should zero the bytes after this entry though.
//! 2. The BPB_FATSz16 (BPB_FATSz32 for FAT32 volumes) value may be bigger than it needs
//!    to be. In other words, there may be totally unused FAT sectors at the end of each FAT in the
//!    FAT region of the volume. For this reason, the last sector of the FAT is always computed
//!    using the CountofClusters + 1 value, never from the BPB_FATSz16/32 value. FAT code
//!    should not make any assumptions about what the contents of these "extra" FAT sectors are.
//!    FAT format code should zero the contents of these extra FAT sectors though.
//!
//! FAT Volume Initialization
//!
//! Given that the FAT type (FAT12, FAT16, or FAT32) is dependant on the number of clusters -- and that
//! the sectors available in the data area of a FAT volume is dependant on the size of the FAT --
//! when handed an unformatted volume that does not yet have a BPB, how do you determine all this and
//! compute the proper values to put in BPB_SecPerClus and either BPB_FATSz16 or BPB_FATSz32?
//! The way Microsoft operating systems do this is with a fixed value, several tables, and a clever
//! piece of arithmetic.

//! FAT32 FSInfo Sector Structure and Backup Boot Sector
//!
//! On a FAT32 volume, the FAT can be a large data structure, unlike on FAT16 where it is limited to a
//! maximum of 128K worth of sectors and FAT12 where it is limited to a maximum of 6K worth of sectors.
//! The FSInfo sector number is the value in the BPB_FSInfo field;
//! for Microsoft operating systems it is always set to 1.
//!
//! See struct [`FSInfo`] for the structure of the FSInfo sector.
//!
//!
//! Assume that the type WORD is a 16-bit unsigned and that the type DWORD is a 32-bit unsigned.
//!
//! Given any valid cluster number N, where in the FAT(s) is the entry for that cluster number?
//!
//! FATOffset = N * 4;
//! ThisFATSecNum = BPB_ResvdSecCnt + (FATOffset / BPB_BytsPerSec);
//! ThisFATEntOffset = REM(FATOffset / BPB_BytsPerSec);
//!
//! See [`FAT<T>`::write()`]

// 布局如下:
//      引导扇区 - 保留扇区 - FAT1 - FAT2 - 数据区
// 1. 保留扇区包括引导扇区, 引导扇区包括 BPB 和 FSInfo
// 2. FAT1 起始地址 = 保留扇区数 * 扇区大小
// 3. 文件分配表区共保存了两个相同的文件分配表, 因为文件所占用的存储空间 (簇链) 及空闲空间的管理都是通过FAT实现的, 保存两个以便第一个损坏时, 还有第二个可用

// #![allow(unused)]

use super::{
    LEAD_SIGNATURE, MAX_CLUSTER_FAT12, MAX_CLUSTER_FAT16, STRUCT_SIGNATURE, TRAIL_SIGNATURE,
};

/// BIOS Parameters
/// *On-disk* data structure for partition information.
#[derive(Debug, Copy, Clone)]
// repr(packed) 表示使用紧凑的表示方式来表示一个结构体或枚举, 编译器不会在字段间填充字节
// 使用 #[repr(packed)] 属性可能会导致访问未对齐的内存, 这可能会导致不可预测的结果, 例如内存访问异常, 程序崩溃等
#[repr(packed)]
pub struct BIOSParameterBlock {
    pub(crate) basic_bpb: BasicBPB, // size = 36B
    pub(crate) bpb32: BPB32,        // size = 54B
}

/// We intend to realize fat32, so we don't need to care about fat12 and fat16.
/// But we still reserve the fields of fat12 and fat16 for future maybe.
pub enum FatType {
    FAT32,
    FAT16,
    FAT12,
}

impl BIOSParameterBlock {
    #[inline(always)]
    /// Get the first sector offset bytes of the cluster from the cluster number
    pub fn offset(&self, cluster: u32) -> usize {
        // Q: why cluster - 2?
        // A: The first two clusters are reserved and the first data cluster is 2.
        assert!(cluster >= 2);
        ((self.basic_bpb.rsvd_sec_cnt as usize)
            + (self.basic_bpb.num_fats as usize) * (self.bpb32.fat_sz32 as usize)
            + (cluster as usize - 2) * (self.basic_bpb.sec_per_clus as usize))
            * (self.basic_bpb.byts_per_sec as usize)
        // (self.first_data_sector() + (cluster as usize - 2) * (self.bpb.sec_per_clus as usize))
        //     * (self.bpb.byts_per_sec as usize)
    }

    #[inline(always)]
    /// The first data sector beyond the root directory
    ///
    /// The start of the data region, the first sector of cluster 2, is computed as follows:
    ///
    // For FAT32, the root directory can be of variable size and is a cluster chain, just like any other
    // directory is. The first cluster of the root directory on a FAT32 volume is stored in BPB_RootClus.
    // Unlike other directories, the root directory itself on any FAT type does not have any date or time
    // stamps, does not have a file name (other than the implied file name “\”), and does not contain “.” and
    // ".." files as the first two directory entries in the directory. The only other special aspect of the root
    // directory is that it is the only directory on the FAT volume for which it is valid to have a file that has
    // only the ATTR_VOLUME_ID attribute bit set.
    //
    //  根目录在此处
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

        (self.basic_bpb.rsvd_sec_cnt as usize)
            + (self.basic_bpb.num_fats as usize) * self.bpb32.fat_sz32 as usize
            + self.root_dir_sector_cnt()
    }

    #[inline(always)]
    /// Given any valid data cluster number N, the sector number of the first sector of that cluster
    /// (again relative to sector 0 of the FAT volume) is computed as follows.
    pub fn first_sector_of_cluster(&self, cluster: u32) -> usize {
        self.first_data_sector() + (cluster as usize - 2) * self.basic_bpb.sec_per_clus as usize
    }

    #[inline(always)]
    /// Get FAT1 Offset
    pub fn fat1_offset(&self) -> usize {
        (self.basic_bpb.rsvd_sec_cnt as usize) * (self.basic_bpb.byts_per_sec as usize)
    }

    pub fn fat1_sector_id(&self) -> usize {
        self.basic_bpb.rsvd_sec_cnt as usize
    }

    #[inline(always)]
    /// Get FAT2 Offset
    pub fn fat2_offset(&self) -> usize {
        self.fat1_offset() + (self.bpb32.fat_sz32 as usize) * (self.basic_bpb.byts_per_sec as usize)
    }

    /// Get sector_per_cluster_usize as usize value
    pub fn sector_per_cluster(&self) -> usize {
        self.basic_bpb.sec_per_clus as usize
    }

    #[inline(always)]
    /// Sectors occupied by the root directory
    ///
    /// Note that on a FAT32 volume, the BPB_RootEntCnt value is always 0; so on a FAT32 volume,
    /// RootDirSectors is always 0.
    /// The 32 in the above is the size of one FAT directory entry in bytes.
    /// Note also that this computation rounds up
    pub fn root_dir_sector_cnt(&self) -> usize {
        ((self.basic_bpb.root_ent_cnt * 32) as usize + (self.basic_bpb.byts_per_sec - 1) as usize)
            / self.basic_bpb.byts_per_sec as usize
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

        self.basic_bpb.tot_sec32 as usize
            - (self.basic_bpb.rsvd_sec_cnt as usize)
            - (self.basic_bpb.num_fats as usize) * (self.bpb32.fat_sz32 as usize)
            - self.root_dir_sector_cnt()
    }

    /// The count of (data) clusters
    ///
    /// This function should round DOWN.
    #[inline(always)]
    pub fn data_cluster_cnt(&self) -> usize {
        self.data_sector_cnt() / (self.basic_bpb.sec_per_clus as usize)
    }

    #[inline(always)]
    /// The total size of the data region
    pub fn total_data_volume(&self) -> usize {
        self.data_sector_cnt() * self.basic_bpb.byts_per_sec as usize
    }

    pub fn is_valid(&self) -> bool {
        self.basic_bpb.root_ent_cnt == 0
            && self.basic_bpb.tot_sec16 == 0
            && self.basic_bpb.tot_sec32 != 0
            && self.basic_bpb.fat_sz16 == 0
            && self.bpb32.fat_sz32 != 0
    }

    #[inline(always)]
    pub fn cluster_size(&self) -> usize {
        self.basic_bpb.sec_per_clus as usize * self.basic_bpb.byts_per_sec as usize
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

    pub fn bytes_per_sector(&self) -> usize {
        self.basic_bpb.byts_per_sec as usize
    }

    pub fn sectors_per_cluster(&self) -> usize {
        self.basic_bpb.sec_per_clus as usize
    }

    pub fn fat_cnt(&self) -> usize {
        self.basic_bpb.num_fats as usize
    }

    pub fn reserved_sector_cnt(&self) -> usize {
        self.basic_bpb.rsvd_sec_cnt as usize
    }

    pub fn total_sector_cnt(&self) -> usize {
        self.basic_bpb.tot_sec32 as usize
    }

    pub fn sector_pre_fat(&self) -> usize {
        self.bpb32.fat_sz32 as usize
    }

    pub fn root_cluster(&self) -> usize {
        self.bpb32.root_clus as usize
    }

    pub fn fat_info_sector(&self) -> usize {
        self.bpb32.fs_info as usize
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(packed)]
/// Boot Sector and BPB Structure For FAT12/16/32
pub struct BasicBPB {
    //  0x00~0x02 3个字节: 跳转指令与空值指令.
    //
    /// x86 assembly to jump instruction to boot code.
    ///
    /// Jump and NOP instructions    Size: 3 bytes    Value: 0xEB ?? 0x90    Offset: 0x00
    //
    //  跳转指令与空值指令    大小: 3字节    值: 0xEB(跳转指令) ??(跳转地址) 0x90(空指令)    偏移: 0x00
    pub(crate) bs_jmp_boot: [u8; 3],

    //  0x03~0x0A 8个字节: OEM名称
    //
    /// It is only a name string.
    ///
    /// OEM name    Size: 8 bytes    Value: ???    Offset: 0x03
    //
    //  OEM名称    大小: 8字节    值: ???    偏移: 0x03
    pub(crate) bs_oem_name: [u8; 8],

    //  从 0x0B 开始的79个字节的数据叫做 BPB (BIOS Paramter Block)
    //
    /// Bytes per sector, This value may take on only the
    /// following values: 512, 1024, 2048 or 4096. 512 for SD card
    ///
    /// Bytes per sector    Size: 2 bytes    Value: 512 (0x200)    Offset: 0x0B
    //
    // 	每扇区字数    大小: 2字节    值: 512 (0x200)    偏移: 0x0B
    pub(crate) byts_per_sec: u16,
    /// Sector per cluster. Number of sectors per allocation unit. This value
    /// must be a power of 2 that is greater than 0. The legal values are
    /// 1, 2, 4, 8, 16, 32, 64, and 128.Note however, that a value should
    /// never be used that results in a "bytes per cluster" value
    /// (BPB_BytsPerSec * BPB_SecPerClus) greater than 32K (32 * 1024).
    /// Usually 8 for SD card.
    ///
    /// Sector per cluster    Size: 1 byte    Value: 8 (0x08)    Offset: 0x0D
    //
    //  每簇扇区数    大小: 1字节    值: 8 (0x08)    偏移: 0x0D
    pub(crate) sec_per_clus: u8,
    /// Sector number of the reserved area.
    /// Number of reserved sectors in the Reserved region of the volume
    /// starting at the first sector of the volume.
    /// For FAT32 volumes, this value is typically 32.
    ///
    /// Reserved sector count    Size: 2 bytes    Value: 32 (0x20)    Offset: 0x0E
    //
    //  保留扇区数    大小: 2字节    值: 32 (0x20)    偏移: 0x0E
    pub(crate) rsvd_sec_cnt: u16,
    /// Number of FATs
    /// This field should always contain the value 2 for any FAT
    /// volume of any type.
    ///
    /// Number of FATs    Size: 1 byte    Value: 2 (0x02)    Offset: 0x10
    //
    //  FAT表数      大小: 1字节    值: 2 (0x02)    偏移: 0x10
    pub(crate) num_fats: u8,
    /// For FAT32 volumes, this field must be set to 0.
    //
    //  根目录最大文件数 (最大目录项个数) (FAT32 已经突破限制, 无效)    大小: 2字节    值: 0 (0x00)   偏移: 0x11
    pub(crate) root_ent_cnt: u16,
    /// For FAT32 volumes, this field must be 0.
    /// If it is 0, then BPB_TotSec32 must be non-zero.
    ///
    /// Total sectors (for FAT12/16)    Size: 2 bytes    Value: 0 (0x00)    Offset: 0x13
    //
    //  扇区总数 (给FAT12/16使用)    大小: 2字节    值: 0 (0x00)   偏移: 0x13
    pub(crate) tot_sec16: u16,
    /// Used to denote the media type. This is a legacy field that is no longer
    /// in use. 0xF8 is the standard value for "fixed" (non-removable) media.
    /// For removable media, 0xF0 is frequently used. The legal values for this
    /// field are: 0xF0, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, and 0xFF.
    /// The only other important point is that whatever value is put in here must
    /// also be put in the low byte of the FAT[0] entry.
    ///
    /// Media descriptor    Size: 1 byte    Value: 0xF8 (0xF8)    Offset: 0x15
    //
    //  媒体描述符    大小: 1字节    值: 0xF8 (0xF8)   偏移: 0x15
    pub(crate) media: u8,
    /// On FAT32 volumes this field must be 0, and fat_sz32 contains the FAT size count.
    ///
    /// FAT size (for FAT12/16)    Size: 2 bytes    Value: 0 (0x00)    Offset: 0x16
    //
    //  每个FAT扇区数 (给FAT12/16使用)    大小: 2字节    值: 0 (0x00)    偏移: 0x16
    pub(crate) fat_sz16: u16,
    /// Sector per track used by interrupt 0x13.
    /// Not needed by SD card.
    ///
    /// Sectors per track    Size: 2 bytes    Value: 0 (0x00)    Offset: 0x18
    //
    // 每磁道扇区数    大小: 2字节    值: 0 (0x00)    偏移: 0x18
    pub(crate) sec_per_trk: u16,
    /// Number of heads for interrupt 0x13.
    /// This field is relevant as discussed earlier for BPB_SecPerTrk.
    /// This field contains the one based "count of heads".
    /// Not needed by SD card.
    ///
    /// Number of heads    Size: 2 bytes    Value: 0 (0x00)    Offset: 0x1A
    //
    //  磁头数      大小: 2字节    值: 0 (0x00)   偏移: 0x1A
    pub(crate) num_heads: u16,
    /// Count of hidden sectors preceding the partition that contains this
    /// FAT volume. This field is generally only relevant for media visible
    /// on interrupt 0x13. This field should always be zero on media that
    /// are not partitioned. Exactly what value is appropriate is operating
    /// system specific.
    ///
    /// Hidden sector count    Size: 4 bytes    Value: 0 (0x00)    Offset: 0x1C
    //
    //  隐藏扇区数    大小: 4字节    值: 0 (0x00)   偏移: 0x1C
    pub(crate) hidd_sec: u32,
    /// This field is the new 32-bit total count of sectors on the volume.
    /// This count includes the count of all sectors in all four regions of the
    /// volume. For FAT32 volumes, this field must be non-zero.
    ///
    /// Total sectors (for FAT32)    Size: 4 bytes    Value: non-zero    Offset: 0x20
    //
    //  扇区总数 (给FAT32使用)    大小: 4字节    值: non-zero   偏移: 0x20
    pub(crate) tot_sec32: u32,
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
    ///
    /// FAT size (for FAT32)    Size: 4 bytes    Value: non-zero    Offset: 0x24
    //
    // 每个FAT扇区数 (给FAT32使用)    大小: 4字节    值: non-zero  偏移: 0x24
    pub(crate) fat_sz32: u32,
    /// This field is only defined for FAT32 media and does not exist on
    /// FAT12 and FAT16 media.
    /// Bits 0-3    -- Zero-based number of active FAT. Only valid if mirroring
    ///                is disabled.
    /// Bits 4-6    -- Reserved.
    /// Bit 7       -- 0 means the FAT is mirrored at runtime into all FATs.
    ///             -- 1 means only one FAT is active; it is the one referenced
    ///                in bits 0-3.
    /// Bits 8-15   -- Reserved.
    ///
    /// Extended flags    Size: 2 bytes    Value: 0 (0x00)    Offset: 0x28
    //
    // 扩展标志    大小: 2字节    值: ???    偏移: 0x28
    pub(crate) ext_flags: u16,
    /// This is the version number of the FAT32 volume.
    /// This field is only defined for FAT32 media. High byte is major
    /// revision number. Low byte is minor revision number.
    /// Disk utilities should respect this field and not operate on
    /// volumes with a higher major or minor version number than that for
    /// which they were designed. FAT32 file system drivers must check
    /// this field and not mount the volume if it does not contain a version
    /// number that was defined at the time the driver was written.
    ///
    /// File system version (always 0)    Size: 2 bytes    Value: 0x0000 (0x0000)    Offset: 0x2A
    //
    //  文件系统版本 (通常为零)    大小: 2字节    值: 0x0000 (0x0000)   偏移: 0x2A
    pub(crate) fs_ver: u16,
    /// This is set to the cluster number of the first cluster of the root
    /// directory, usually 2 but not required to be 2.
    ///
    /// Root directory first cluster (always 2)    Size: 4 bytes    Value: 2 (0x02)    Offset: 0x2C
    //
    //  根目录起始簇号    大小: 4字节    值: 2 (0x02)    偏移: 0x2C
    pub(crate) root_clus: u32,
    /// Sector number of FSINFO structure in the reserved area of
    /// the FAT32 volume. Usually 1.
    ///
    /// FSINFO sector (always 1)    Size: 2 bytes    Value: 1 (0x01)    Offset: 0x30
    //
    //  FSINFO 扇区号 (Boot占用扇区数)    大小: 2字节    值: 1 (0x01)   偏移: 0x30
    pub(crate) fs_info: u16,
    /// The sector number in the reserved area of the volume of
    /// a copy of the boot record. Usually 6.
    /// The case-sector 0 goes bad-is the reason why no value other than 6 should ever be placed
    /// in the BPB_BkBootSec field. If sector 0 is unreadable, various operating systems are "hard wired" to
    /// check for backup boot sector(s) starting at sector 6 of the FAT32 volume. Note that starting at the
    /// BPB_BkBootSec sector is a complete boot record. The Microsoft FAT32 "boot sector" is actually
    /// three 512-byte sectors long. There is a copy of all three of these sectors starting at the
    /// BPB_BkBootSec sector. A copy of the FSInfo sector is also there, even though the BPB_FSInfo field
    /// in this backup boot sector is set to the same value as is stored in the sector 0 BPB.
    ///
    /// Backup boot sector (always 6)    Size: 2 bytes    Value: 6 (0x06)    Offset: 0x32
    //
    //  备份引导扇区扇区号    大小: 2字节    值: 6 (0x06)   偏移: 0x32
    pub(crate) bk_boot_sec: u16,
    /// Reserved for future expansion. Code that formats FAT32 volumes
    /// should always set all of the bytes of this field to 0.
    //
    //  保留区    大小: 12字节    值: 0 (0x00)    偏移: 0x34
    pub(crate) reserved: [u8; 12],
    //  以下 26B 与 FAT12/16 的 BPB 完全相同, 但是偏移不同
    //
    /// This field is the physical drive number for the INT 13h.
    /// This field has the same definition as it does for FAT12 and FAT16
    /// media. The only difference for FAT32 media is that the field is at a
    /// different offset in the boot sector.
    ///
    /// Physical drive number    Size: 1 byte    Value: 0x80    Offset: 0x40
    //
    //  物理驱动器号    大小: 1字节    值: 0x80    偏移: 0x40
    pub(crate) bs_drv_num: u8,
    /// This field is no longer used and should always be set to 0.
    ///
    /// Reserved (used by Windows NT)    Size: 1 byte    Value: 0 (0x00)    Offset: 0x41
    //
    //  保留区    大小: 1字节    值: 0 (0x00)    偏移: 0x41
    pub(crate) bs_reserved1: u8,
    /// This field is the extended boot signature. This field is set to 0x29.
    /// This is a signature byte that indicates that the following three fields
    /// in the boot sector are present. (BS_VolID, BS_VolLab, BS_FilSysType)
    ///
    /// Extended boot signature    Size: 1 byte    Value: 0x29 (0x29)    Offset: 0x42
    //
    //  扩展引导标记    大小: 1字节    值: 0x29 (0x29)   偏移: 0x42
    pub(crate) bs_boot_sig: u8,
    /// Volume serial number. This field, together with BS_VolLab,
    /// supports volume tracking on removable media. These values allow
    /// FAT file system drivers to detect that the wrong disk is inserted in a
    /// removable drive. This ID is usually generated by simply combining
    /// the current date and time into a 32-bit value.
    ///
    /// Volume serial number    Size: 4 bytes    Value: ???    Offset: 0x43
    //
    //  卷序列号    大小: 4字节    值: ???    偏移: 0x43
    pub(crate) bs_vol_id: u32,
    /// Volume label. This field matches the 11-byte volume label recorded in
    /// the root directory.
    /// NOTE: FAT file system drivers should make sure that they update
    /// this field when the volume label file in the root directory has its
    /// name changed or created. The setting for this field when there is no
    /// volume label is the string "NO NAME    ".
    ///
    /// Volume label    Size: 11 bytes    Value: ???    Offset: 0x47
    //
    //  卷标    大小: 11字节    值: ???    偏移: 0x47
    pub(crate) bs_vol_lab: [u8; 11],
    /// File system type.
    /// This string is informational only and is not used by Microsoft
    /// file system drivers to determine FAT typ,e because it is frequently
    /// not set correctly or is not present.
    ///
    /// File system type    Size: 8 bytes    Value: "FAT32   "    Offset: 0x52
    //
    //  文件系统类型    大小: 8字节    值: "FAT32   "    偏移: 0x52
    pub(crate) bs_fil_sys_type: [u8; 8],
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

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
/// FAT32 FSInfo Sector Structure and Backup Boot Sector
pub struct FSInfo {
    /// Value 0x41615252. This lead signature is used to validate that this is in fact an FSInfo sector.
    ///
    /// Lead signature    Size: 4 bytes    Value: 0x41615252    Offset: 0
    //
    //  引导签名   大小: 4 字节    值: 0x41615252    偏移: 0
    pub(crate) lead_sig: u32,
    /// The reserved area should be empty.
    /// This field is currently reserved for future expansion. FAT32 format
    /// code should always initialize all bytes of this field to 0. Bytes in
    /// this field must currently never be used.
    ///
    /// Reserved    Size: 480 bytes    Value: 0    Offset: 4
    //
    //  保留区   大小: 480 字节    值: 0    偏移: 4
    pub(crate) reserved1: [u8; 480],
    /// Value 0x61417272.
    /// Another signature that is more localized in the sector to the location of the fields that are used.
    ///
    /// Structure signature    Size: 4 bytes    Value: 0x61417272    Offset: 484
    //
    //  结构签名(表明已使用)   大小: 4 字节    值: 0x61417272    偏移: 484
    pub(crate) struc_sig: u32,
    /// Contains the last known free cluster count on the volume. If the
    /// value is 0xFFFFFFFF, then the free count is unknown and must be
    /// computed. Any other value can be used, but is not necessarily
    /// correct. It should be range checked at least to make sure it is <=
    /// volume cluster count.
    ///
    /// Free cluster count    Size: 4 bytes    Value: 0xFFFFFFFF    Offset: 488
    //
    //  剩余簇数量   大小: 4 字节    值: 0xFFFFFFFF    偏移: 488
    pub(crate) free_count: u32,
    /// This is a hint for the FAT driver. It indicates the cluster number at
    /// which the driver should start looking for free clusters. Because a
    /// FAT32 FAT is large, it can be rather time consuming if there are a
    /// lot of allocated clusters at the start of the FAT and the driver starts
    /// looking for a free cluster starting at cluster 2. Typically this value is
    /// set to the last cluster number that the driver allocated. If the value is
    /// 0xFFFFFFFF, then there is no hint and the driver should start
    /// looking at cluster 2. Any other value can be used, but should be
    /// checked first to make sure it is a valid cluster number for the
    /// volume.
    ///
    /// Next free cluster    Size: 4 bytes    Value: 0xFFFFFFFF / ???    Offset: 492
    //
    //  值 0xFFFFFFFF 表示没有提示, 从簇2开始查找; 其他值表示从该簇开始查找
    //
    //  最后一个已分配簇的簇号   大小: 4 字节    值: ??? / 0xFFFFFFFF    偏移: 492
    pub(crate) nxt_free: u32,
    /// The reserved area should be empty.
    /// This field is currently reserved for future expansion. FAT32 format
    /// code should always initialize all bytes of this field to 0. Bytes in
    /// this field must currently never be used.
    ///
    /// Reserved    Size: 12 bytes    Value: 0    Offset: 496
    //
    //  保留区   大小: 12 字节    值: 0    偏移: 496
    pub(crate) reserved2: [u8; 12],
    /// Value 0xAA550000.
    /// This trail signature is used to validate that this is in fact an FSInfo sector.
    /// Note that the high 2 bytes of this value which go into the bytes at offsets 510 and 511
    /// match the signature bytes used at the same offsets in sector 0.
    ///
    /// Trail signature    Size: 4 bytes    Value: 0xAA550000    Offset: 508
    //
    //  结束签名   大小: 4 字节    值: 0xAA550000    偏移: 508
    pub(crate) trail_sig: u32,
}

impl FSInfo {
    // Check the signature
    pub fn check_signature(&self) -> bool {
        self.lead_sig == LEAD_SIGNATURE
            && self.struc_sig == STRUCT_SIGNATURE
            && self.trail_sig == TRAIL_SIGNATURE
    }

    // Get the number of free clusters
    pub fn free_cluster_cnt(&self) -> u32 {
        self.free_count
    }

    // Set the number of free clusters
    pub fn set_free_clusters(&mut self, free_clusters: u32) {
        self.free_count = free_clusters
    }

    // Get next free cluster location
    pub fn next_free_cluster(&self) -> u32 {
        self.nxt_free
    }

    // Set next free cluster location
    pub fn set_next_free_cluster(&mut self, start_cluster: u32) {
        self.nxt_free = start_cluster
    }
}
