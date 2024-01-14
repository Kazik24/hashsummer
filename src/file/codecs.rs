use crate::file::chunks::{AnyBlock, BlockType, HashesChunk, HashesHeader, NamesChunk, NamesHeader};
use crate::file::codec_utils::read_first_data_chunk;
use crate::file::{BlockError, MainHeader, StdHashArray, VersionCodec};
use crate::HashArray;
use std::io;
use std::io::{BufReader, ErrorKind, Read};

pub struct Codec0_0_1 {}

impl Codec0_0_1 {
    pub const fn new() -> Self {
        Self {}
    }
}

impl VersionCodec for Codec0_0_1 {
    fn decode_header_fields(&self, array: HashArray<57>, header: &mut MainHeader) -> io::Result<()> {
        Ok(())
    }

    fn decode_additional_header(&self, read: &mut dyn Read, header: &mut MainHeader) -> io::Result<()> {
        Ok(())
    }

    fn decode_block(&self, first_block: StdHashArray, read: &mut dyn Read, header: &MainHeader) -> Result<AnyBlock, BlockError> {
        let block_type = BlockType::decode_magic(first_block.get_slice(0))?.ok_or(BlockError::UnknownBlockType)?;

        match block_type {
            BlockType::Hashes => {
                let header = HashesHeader::from_array(first_block)?;
                let chunk = HashesChunk::read_body(header, read)?;
                Ok(AnyBlock::Hashes(chunk))
            }
            BlockType::Names => {
                let header = NamesHeader::from_array(first_block)?;
                let chunk = NamesChunk::read_body(header, read)?;
                Ok(AnyBlock::Names(chunk))
            }

            _ => Err(BlockError::UnknownBlockType),
        }
    }
}
