use super::codecs::*;
use crate::HashArray;
use std::fs::File;
use std::io::{Read, Seek, Write};

pub const MAIN_HEADER_MAGIC: &[u8; 4] = b"HsUm";
pub const BLOCK_HEADER_MAGIC: &[u8; 4] = b"HsBk";

pub const LATEST_VERSION: [u8; 3] = [0, 0, 1];

//must be sorted
pub static CODECS: &[([u8; 3], &dyn VersionCodec)] = &[
    ([0, 0, 1], &Codec0_0_1::new()), //latest version
];

fn get_latest_codec() -> &'static dyn VersionCodec {
    get_codec(LATEST_VERSION).expect("Init error: No codec for latest version")
}
pub fn get_codec(version: [u8; 3]) -> Option<&'static dyn VersionCodec> {
    CODECS.iter().find(|(v, _)| v == &version).map(|(_, c)| c).copied() //linear search for now

    //replace when CODECS can be statically ensured to be sorted
    //CODECS.binary_search_by_key(&version, |(v, _)| *v).ok().map(|i| CODECS[i].1)
}

pub trait VersionCodec: Send + Sync + 'static {}

pub struct SumFile<T: Read + Write + Seek> {
    file: T,
    current_pos: Option<u64>,
    main_header: HashArray<64>,
    initialized: bool,
}

pub struct FileBlock {
    header: HashArray<64>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum BlockType {
    #[default]
    None = 0,
    MainHeader = 1, //main header is always 64 bytes, should be only one in file,
    Hashes = 2,     //hashes chunk

    Reserved = 254,
    MoreBlocks = 255,
}

impl<T> SumFile<T>
where
    T: Read + Write + Seek,
{
    pub fn new(mut file: T) -> Self {
        Self {
            main_header: HashArray::zero(),
            current_pos: None,
            file,
            initialized: false,
        }
    }
}
