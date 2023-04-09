#![no_std]
pub mod bpb;
pub mod fat;

pub const LEAD_SIGNATURE: u32 = 0x41615252;
pub const STRUC_SIGNATURE: u32 = 0x61417272;
pub const FREE_CLUSTER: u32 = 0x00000000;
pub const END_CLUSTER: u32 = 0x0FFFFFF8;
pub const BAD_CLUSTER: u32 = 0x0FFFFFF7;

pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

pub const DIRENT_SZ: usize = 32;
pub const LONG_NAME_LEN: u32 = 13;
pub const BLOCK_SIZE: usize = 512;

pub const MAX_CLUSTER_FAT12: usize = 4085;
pub const MAX_CLUSTER_FAT16: usize = 65525;
pub const MAX_CLUSTER_FAT32: usize = 268435445;

/// The two reserved clusters at the start of the FAT, and FAT[1] high bit mask as follows:
/// Bit ClnShutBitMask -- If bit is 1, volume is “clean”. If bit is 0, volume is “dirty”.
/// Bit HrdErrBitMask  -- If this bit is 1, no disk read/write errors were encountered.
///                       If this bit is 0, the file system driver encountered a disk I/O error on the Volume
///                       the last time it was mounted, which is an indicator that some sectors may have gone bad on the volume.
pub const CLN_SHUT_BIT_MASK_FAT32: usize = 0x08000000;
pub const HRD_ERR_BIT_MASK_FAT32: usize = 0x04000000;
