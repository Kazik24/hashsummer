use super::codecs::*;
use crate::file::chunks::{AnyBlock, BlockType, HashesChunk, InfoChunk, NamesChunk};
use crate::utils::with_counted_read;
use crate::{HashArray, SumFileHeader};
use std::fs::File;
use std::io;
use std::io::{Error, ErrorKind, Read, Seek, Write};
use std::path::Path;

pub const MAIN_HEADER_MAGIC: [u8; 4] = *b"HsUm";

//must be sorted
pub static CODECS: &[([u8; 3], &dyn VersionCodec)] = &[
    ([0, 0, 1], &Codec0_0_1::new()), //latest version
];

fn get_latest_codec() -> ([u8; 3], &'static dyn VersionCodec) {
    *CODECS.iter().max_by_key(|v| v.0).expect("Init error: No codec for latest version")
}
pub fn get_codec(version: [u8; 3]) -> Option<&'static dyn VersionCodec> {
    CODECS.iter().find(|(v, _)| v == &version).map(|(_, c)| c).copied() //linear search for now

    //replace when CODECS can be statically ensured to be sorted
    //CODECS.binary_search_by_key(&version, |(v, _)| *v).ok().map(|i| CODECS[i].1)
}

pub trait VersionCodec: Send + Sync + 'static {
    fn decode_header_fields(&self, array: HashArray<57>, header: &mut MainHeader) -> io::Result<()>;
    fn decode_additional_header(&self, read: &mut dyn Read, header: &mut MainHeader) -> io::Result<()>;
}

pub struct SumFile<T: Read + Write + Seek> {
    file: T,
    current_pos: Option<u64>,
    main_header: MainHeader,
    initialized: bool,
}

pub struct MainHeader {
    codec: &'static dyn VersionCodec,
    flags: u8,
}

impl MainHeader {
    pub fn new() -> Self {
        Self {
            flags: 0,
            codec: get_latest_codec().1,
        }
    }
    pub fn read<R: Read>(stream: &mut R) -> io::Result<(Self, u64)> {
        let mut main_header = HashArray::<64>::zero();
        stream.read_exact(main_header.as_bytes_mut())?;
        if main_header.get_slice::<4>(0) != MAIN_HEADER_MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic bytes"));
        }
        let version = main_header.get_slice::<3>(4);
        let codec = get_codec(version).ok_or_else(|| {
            let ([lma, lmi, lpa], _) = get_latest_codec();
            let [maj, min, pat] = version;
            let m = format!("Unknown fingerprint file version v{maj}.{min}.{pat}, latest supported version is v{lma}.{lmi}.{lpa}");
            io::Error::new(io::ErrorKind::InvalidData, m)
        })?;
        let mut header = Self { codec, flags: 0 };
        let rest = main_header.get_slice::<57>(7);
        codec.decode_header_fields(HashArray::new(rest), &mut header)?;

        let (_, count) = with_counted_read(stream, |read| codec.decode_additional_header(read, &mut header))?;

        Ok((header, (main_header.as_bytes().len() as u64) + count))
    }
}

impl SumFile<File> {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let (main_header, pos) = MainHeader::read(&mut file)?;

        Ok(Self {
            main_header,
            initialized: true,
            current_pos: Some(pos),
            file,
        })
    }
}

pub enum BlockError {
    /// End of block stream
    NoBlock,
    UnknownBlockType,
    Io(io::Error),
}

impl From<BlockError> for io::Error {
    fn from(value: BlockError) -> Self {
        match value {
            BlockError::NoBlock => io::Error::new(ErrorKind::InvalidData, "No more blocks"),
            BlockError::UnknownBlockType => io::Error::new(ErrorKind::InvalidData, "Unknown block type"),
            BlockError::Io(e) => e,
        }
    }
}

impl From<io::Error> for BlockError {
    fn from(value: Error) -> Self {
        BlockError::Io(value)
    }
}

impl<T> SumFile<T>
where
    T: Read + Write + Seek,
{
    pub fn new(mut file: T) -> Self {
        Self {
            main_header: MainHeader::new(),
            current_pos: None,
            file,
            initialized: false,
        }
    }

    pub fn read_next_block(&mut self) -> Result<AnyBlock, BlockError> {
        let mut first_chunk = HashArray::<64>::zero();
        match self.file.read_exact(first_chunk.as_bytes_mut()) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Err(BlockError::NoBlock), //no blocks
            Err(e) => return Err(BlockError::Io(e)),
        }

        let block_type = BlockType::decode_magic(first_chunk.get_slice(0))?.ok_or(BlockError::UnknownBlockType)?;

        todo!()
    }

    pub fn write_next_block(&mut self, block: &AnyBlock) -> io::Result<()> {
        Ok(())
    }
}
