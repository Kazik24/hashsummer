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
    pub const MAGIC_SIZE: usize = BLOCK_HEADER_MAGIC.len() + 1;
    pub fn decode_magic(header: [u8; Self::MAGIC_SIZE]) -> io::Result<Option<Self>> {
        Ok(BlockType::from_u8(Self::decode_magic_code(header)?))
    }

    fn decode_magic_code(header: [u8; Self::MAGIC_SIZE]) -> io::Result<u8> {
        let header_bytes = &header[..BLOCK_HEADER_MAGIC.len()];
        if header_bytes != BLOCK_HEADER_MAGIC {
            let bytes = header_bytes
                .iter()
                .flat_map(|c| c.escape_ascii())
                .map(|c| c as char)
                .collect::<String>();
            let msg = format!("Unexpected block magic prefix bytes '{bytes}'");
            return Err(io::Error::new(ErrorKind::InvalidData, msg));
        }
        Ok(header[BLOCK_HEADER_MAGIC.len()])
    }

    pub fn require_magic(self, header: [u8; Self::MAGIC_SIZE]) -> io::Result<()> {
        let code = Self::decode_magic_code(header)?;
        match Self::from_u8(code) {
            None => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Expected {self:?} block type, but got unsupported code: 0x{code:x}"),
            )),
            Some(ty) if ty != self => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Expected {self:?} block type, but got {ty:?}"),
            )),
            Some(_) => Ok(()),
        }
    }

    pub fn magic(&self) -> [u8; Self::MAGIC_SIZE] {
        let mut arr = [0; Self::MAGIC_SIZE];
        arr[..BLOCK_HEADER_MAGIC.len()].copy_from_slice(&BLOCK_HEADER_MAGIC);
        arr[BLOCK_HEADER_MAGIC.len()] = *self as u8;
        arr
    }
}

pub trait ReadableChunk {
    type Header;
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
