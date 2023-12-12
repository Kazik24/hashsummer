use super::codecs::*;
use crate::file::hashes_chunk::HashesChunk;
use crate::file::names_chunk::{InfoChunk, NamesChunk};
use crate::{HashArray, SumFileHeader};
use std::fs::File;
use std::io;
use std::io::{ErrorKind, Read, Seek, Write};
use std::path::Path;

pub const MAIN_HEADER_MAGIC: [u8; 4] = *b"HsUm";
pub const BLOCK_HEADER_MAGIC: [u8; 4] = *b"HsBk";

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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
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

fn with_counted_read<R: Read, T>(read: &mut R, func: impl FnOnce(&mut dyn Read) -> io::Result<T>) -> io::Result<(T, u64)> {
    struct StreamCountWrapper<'a, R>(&'a mut R, u64, bool);
    impl<R: Read> Read for StreamCountWrapper<'_, R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let res = self.0.read(buf);
            match &res {
                Ok(count) => self.1 += *count as u64,
                Err(err) if err.kind() != ErrorKind::Interrupted => self.2 = true, //register error
                _ => {}
            }
            res
        }
    }
    //count how many bytes was read from stream
    let mut wrapper = StreamCountWrapper(read, 0, false);
    let result = func(&mut wrapper)?;
    if wrapper.2 {
        //if there was unpropagated error, raise it here.
        return Err(io::Error::new(ErrorKind::Other, "IO Error was ignored by file codec"));
    }
    Ok((result, wrapper.1))
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

    pub fn blocks(&self) -> &[AnyBlock] {
        &[]
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
}

pub enum AnyBlock {
    Hashes(HashesChunk),
    Names(NamesChunk),
    Info(InfoChunk),
}
