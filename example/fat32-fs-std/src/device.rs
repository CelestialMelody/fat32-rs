use fat32::device::BlockDevice;
use fat32::BlockDeviceError;
use fat32::BLOCK_SIZE;

use spin::RwLock;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

pub struct BlockFile(pub RwLock<File>);

impl BlockDevice for BlockFile {
    /// Read block from BlockDevice
    ///
    /// - offset must be a multiple of BLOCK_SIZE
    /// - block_cnt = buf.len() / BLOCK_SIZE
    fn read_blocks(
        &self,
        buf: &mut [u8],
        offset: usize,
        block_cnt: usize,
    ) -> Result<(), BlockDeviceError> {
        let mut file = self.0.write();
        assert!(
            offset % BLOCK_SIZE == 0,
            "offset must be a multiple of BLOCK_SIZE"
        );
        assert!(
            buf.len() % BLOCK_SIZE == 0,
            "buf.len() must be a multiple of BLOCK_SIZE"
        );
        file.seek(SeekFrom::Start(offset as u64))
            .expect("Error when seeking!");
        assert_eq!(file.read(buf).unwrap(), buf.len(), "Not a complete block");
        Ok(())
    }

    /// Write block into the file system.
    /// - buf.len() must be a multiple of BLOCK_SIZE
    /// - offset must be a multiple of BLOCK_SIZE
    /// - block_cnt = buf.len() / BLOCK_SIZE
    fn write_blocks(
        &self,
        buf: &[u8],
        offset: usize,
        block_cnt: usize,
    ) -> Result<(), BlockDeviceError> {
        let mut file = self.0.write();
        assert!(
            offset % BLOCK_SIZE == 0,
            "offset must be a multiple of BLOCK_SIZE"
        );
        assert!(
            buf.len() % BLOCK_SIZE == 0,
            "buf.len() must be a multiple of BLOCK_SIZE"
        );
        file.seek(SeekFrom::Start(offset as u64))
            .expect("Error when seeking!");
        assert_eq!(file.write(buf).unwrap(), buf.len(), "Not a complete block");
        Ok(())
    }
}
