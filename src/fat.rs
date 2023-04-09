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
//! See the realizetion in [`bpb::BIOSParameterBlock::fat_type()`]
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
//! Note that a zero-length file—a file that has no data allocated to it—has a first cluster number of 0
//! placed in its directory entry. This cluster location in the FAT (see earlier computation of
//! ThisFATSecNum and ThisFATEntOffset) contains either an EOC mark (End Of Clusterchain) or the
//! cluster number of the next cluster of the file. The EOC value is FAT type dependant (assume
//! FATContent is the contents of the cluster entry in the FAT being checked to see whether it is an EOC mark)
//!
//! Note that the cluster number whose cluster entry in the FAT contains the EOC mark is allocated to the
//! file and is also the last cluster allocated to the file.
//!
//! There is also a special “BAD CLUSTER” mark. Any cluster that contains the “BAD CLUSTER”
//! value in its FAT entry is a cluster that should not be placed on the free list because it is prone to disk
//! errors. The “BAD CLUSTER” value is 0x0FF7 for FAT12, 0xFFF7 for FAT16, and 0x0FFFFFF7 for
//! FAT32. The other relevant note here is that these bad clusters are also lost clusters—clusters that
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
//! Bit ClnShutBitMask -- If bit is 1, volume is “clean”. If bit is 0, volume is “dirty”.
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
//!    should not make any assumptions about what the contents of these “extra” FAT sectors are.
//!    FAT format code should zero the contents of these extra FAT sectors though.
//!
//! FAT Volume Initialization
//!
//! Given that the FAT type (FAT12, FAT16, or FAT32) is dependant on the number of clusters -— and that
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
//! See struct [`Fat32FsInfo`] for the structure of the FSInfo sector.

#![allow(unused)]

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
/// *On-disk* data structure.
pub struct Fat32FSInfo {
    /// Value 0x41615252. This lead signature is used to validate that this is in fact an FSInfo sector.
    lead_sig: u32,
    /// The reserved area should be empty.
    reserved1: [u8; 480],
    /// Value 0x61417272. Another signature that is more localized in the sector to the location of the fields that are used.
    struc_sig: u32,
    /// Contains the last known free cluster count on the volume. If the
    /// value is 0xFFFFFFFF, then the free count is unknown and must be
    /// computed. Any other value can be used, but is not necessarily
    /// correct. It should be range checked at least to make sure it is <=
    /// volume cluster count.
    free_count: u32,
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
    nxt_free: u32,
    reserved2: [u8; 12],
    /// Value 0xAA550000.
    /// This trail signature is used to validate that this is in fact an FSInfo sector.
    /// Note that the high 2 bytes of this value which go into the bytes at offsets 510 and 511
    /// match the signature bytes used at the same offsets in sector 0.
    trail_sig: u32,
}
