pub mod chunk;
mod codecs;
mod sum_file;

pub use sum_file::*;

use std::io::{BufReader, Read, Seek, Write};

pub struct BlockBuf<T> {
    file: T,
    file_pos: Option<u64>,
    buffer: Box<[u8]>,
    buf_pos: u64,
}

impl<T> BlockBuf<T> {
    pub const DEFAUL_BLOCK: usize = 1024 * 8;
    pub fn with_block_size(data: T, size: usize) -> Self {
        Self {
            file: data,
            file_pos: None,
            buffer: vec![0u8; size].into_boxed_slice(),
            buf_pos: 0,
        }
    }
    pub fn new(data: T) -> Self {
        Self::with_block_size(data, Self::DEFAUL_BLOCK)
    }
}

impl<T> BlockBuf<T>
where
    T: Read + Write + Seek,
{
    fn test(&mut self) {}
}
