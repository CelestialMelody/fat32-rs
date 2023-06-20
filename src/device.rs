//! Block device interface

use core::any::Any;
use core::marker::{Send, Sync};
use core::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceErr {
    ReadError,
    WriteError,
}

pub trait BlockDevice: Send + Sync + Any {
    /// Read block from BlockDevice
    ///
    /// - offset must be a multiple of BLOCK_SIZE
    /// - block_cnt = buf.len() / BLOCK_SIZE
    fn read_blocks(
        &self,
        buf: &mut [u8],
        offset: usize,
        _block_cnt: usize,
    ) -> Result<(), DeviceErr>;

    /// Write block into the file system.
    /// - buf.len() must be a multiple of BLOCK_SIZE
    /// - offset must be a multiple of BLOCK_SIZE
    /// - block_cnt = buf.len() / BLOCK_SIZE
    fn write_blocks(&self, buf: &[u8], offset: usize, _block_cnt: usize) -> Result<(), DeviceErr>;
}
