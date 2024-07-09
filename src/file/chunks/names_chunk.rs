use crate::file::chunks::BlockType;
use crate::file::StdHashArray;
use crate::utils::{BungeeIndex, BungeeStr, MeasureMemory};
use std::io;
use std::io::{Error, ErrorKind, Read};
use std::mem::size_of;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct NamesChunk {
    bungee: BungeeStr,
    indexes: Vec<BungeeIndex>,
}

pub struct InfoChunk {}

pub struct NamesHeader {
    bungee_size: u64,
    bungee_entry_count: u64,
}

impl NamesHeader {
    pub fn from_array(array: StdHashArray) -> io::Result<Self> {
        BlockType::Names.require_magic(array.get_slice(0))?;
        let flags = array.get_u32(4);
        let bungee_size = array.get_u64(8);
        let bungee_entry_count = array.get_u64(16);

        Ok(Self {
            bungee_size,
            bungee_entry_count,
        })
    }
}

impl NamesChunk {
    pub fn new(bungee: BungeeStr, indexes: Vec<BungeeIndex>) -> Self {
        Self { bungee, indexes }
    }

    pub fn read_body<R: Read + ?Sized>(header: NamesHeader, read: &mut R) -> io::Result<Self> {
        if header.bungee_size > u32::MAX as _ {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "More that u32::MAX hash entries are not supported",
            ));
        }

        todo!()
    }
}

impl MeasureMemory for NamesChunk {
    fn memory_usage(&self) -> usize {
        (self.indexes.capacity() * size_of::<BungeeIndex>()) + self.bungee.memory_usage()
    }
}
