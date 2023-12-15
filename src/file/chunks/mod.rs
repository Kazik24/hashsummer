mod hashes_chunk;
mod names_chunk;

use crate::HashArray;
use digest::Digest;
pub use hashes_chunk::*;
pub use names_chunk::*;
use num_traits::FromPrimitive;
use rustfft::num_traits;
use std::io;
use std::io::ErrorKind;

pub const BLOCK_HEADER_MAGIC: [u8; 3] = *b"hSb";

pub trait HsumChunk {
    fn append_to(&self, digest: &mut impl Digest);
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, num_derive::FromPrimitive)]
#[repr(u8)]
pub enum BlockType {
    #[default]
    None = 0,
    MainHeader = 1, //main header is always 64 bytes, should be only one in file,
    Hashes = 2,     //hashes chunk
    Names = 3,      //names of files for corresponding hashes

    Reserved = 254,
    MoreBlocks = 255,
}

impl BlockType {
    pub const MAGIC_SIZE: usize = 4;
    pub fn decode_magic(header: [u8; Self::MAGIC_SIZE]) -> io::Result<Option<Self>> {
        if header[..3] != BLOCK_HEADER_MAGIC {
            return Err(io::Error::new(ErrorKind::InvalidData, "Unexpected block magic bytes"));
        }
        Ok(BlockType::from_u8(header[3]))
    }
}

pub enum AnyBlock {
    Hashes(HashesChunk),
    Names(NamesChunk),
    Snapshot(),
    EndSnapshot(),
    Info(InfoChunk),
    End(EndingChunk),
}

/// Chunk that is always at the end of a file, contains a hash of whole file and all of it's chunks, it marks also
/// end of hash file
pub struct EndingChunk {
    hash: HashArray<32>,
    hash_type: HashType,
}
