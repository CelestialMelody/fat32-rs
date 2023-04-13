use core::any::Any;

pub trait BlockDevice: Send + Sync + Any {
    type Error;

    fn read(&self, buf: &mut [u8], offset: usize, block_cnt: usize) -> Result<(), Self::Error>;

    fn write(&self, buf: &[u8], offset: usize, block_cnt: usize) -> Result<(), Self::Error>;
}
