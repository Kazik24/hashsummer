use crate::file::chunks::{BlockType, BLOCK_HEADER_MAGIC};
use crate::file::StdHashArray;
use crate::utils::{BungeeIndex, BungeeStr, MeasureMemory};
use crate::{DataEntry, HashArray, HashEntry};
use rustfft::num_traits::ToPrimitive;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::io;
use std::io::{Error, ErrorKind, Read, Seek, Write};
use std::mem::size_of;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct HashesChunk {
    pub data: Vec<DataEntry>,
    pub sort: SortOrder,
    pub name_hash: HashType,
    pub data_hash: HashType,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum SortOrder {
    Unordered = 0,
    SortedByName = 1,
    Unknown = 2,
    SortedByData = 3,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum HashType {
    Sha256,
    Blake3,
}

macro_rules! impl_fingerprint {
    ($($value:ident => $array:literal $( or $($more:literal)|+ )? $(bytes: $bytes:expr)?),*) => {
        impl HashType {
            pub const FINGERPRINT_SIZE: usize = 8;
            pub fn get_fingerprint(&self) -> [u8; Self::FINGERPRINT_SIZE] {
                match self {
                    $(Self::$value => *$array,)*
                }
            }
            pub fn from_fingerprint(fp: [u8; Self::FINGERPRINT_SIZE]) -> Option<Self> {
                match &fp {
                    $($array $( $(| $more)+)? => Some(Self::$value),)*
                    _ => None,
                }
            }
            pub fn bytes_count(&self) -> usize {
                match self {
                    $(Self::$value => { 32 $(; $bytes)? },)*
                }
            }
        }
    }
}

impl_fingerprint! {
    Sha256 => b"Sha2_256" or b"Sha256__" | b"Sha2-256",
    Blake3 => b"Blake3__" or b"BLAKE3__"
}

pub struct HashesHeader {
    size: u64,
    sort: SortOrder,
    name_hash: HashType,
    data_hash: HashType,
}

impl HashesHeader {
    const FLAG_SORTED: u32 = 1;
    const FLAG_SORTED_BY_DATA: u32 = 1;

    pub fn to_array(&self) -> HashArray<64> {
        let mut array = HashArray::zero();
        array.set_slice(0, BlockType::Hashes.magic());
        let mut flags = 0;
        flags |= self.sort as u32 & 0x3;
        array.set_u32(4, flags);
        array.set_u64(8, self.size);
        array.set_slice(16, self.name_hash.get_fingerprint());
        array.set_slice(24, self.data_hash.get_fingerprint());
        //bytes 32..64 are zeroed
        array
    }
    pub fn read<R: Read + ?Sized>(read: &mut R) -> io::Result<Self> {
        let mut header = HashArray::zero();
        read.read_exact(header.get_mut())?;
        Self::from_array(header)
    }

    pub fn from_array(array: StdHashArray) -> io::Result<Self> {
        BlockType::Hashes.require_magic(array.get_slice(0))?;
        let flags = array.get_u32(4);
        let size = array.get_u64(8);
        let sort = match flags & 0x3 {
            0 => SortOrder::Unordered,
            1 => SortOrder::SortedByName,
            3 => SortOrder::SortedByData,
            _ => SortOrder::Unknown,
        };
        let sorted = (flags & Self::FLAG_SORTED) != 0;
        let sorted_by_data = (flags & Self::FLAG_SORTED_BY_DATA) != 0;
        let name_hash = HashType::from_fingerprint(array.get_slice(16))
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Unknown name hash type fingerprint"))?;
        let data_hash = HashType::from_fingerprint(array.get_slice(24))
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Unknown data hash type fingerprint"))?;

        Ok(Self {
            sort,
            size,
            name_hash,
            data_hash,
        })
    }
}

impl HashesChunk {
    pub fn new_sha256(data: Vec<DataEntry>, sorted: bool) -> Self {
        Self {
            data,
            sort: SortOrder::SortedByName,
            name_hash: HashType::Sha256,
            data_hash: HashType::Sha256,
        }
    }

    pub fn read<R: Read + ?Sized>(read: &mut R) -> io::Result<Self> {
        let header = HashesHeader::read(read)?;
        Self::read_body(header, read)
    }
    pub fn read_body<R: Read + ?Sized>(header: HashesHeader, read: &mut R) -> io::Result<Self> {
        if header.size > u32::MAX as _ {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "More that u32::MAX hash entries are not supported",
            ));
        }

        let mut data = vec![HashEntry::zero(); header.size as usize];

        let data_bytes = unsafe { data.as_mut_slice().align_to_mut::<u8>().1 };
        read.read_exact(data_bytes)?;

        //todo fix any endianess issues?
        //Self::fix_endianness(data_bytes);

        Ok(Self {
            sort: header.sort,
            data,
            name_hash: header.name_hash,
            data_hash: header.data_hash,
        })
    }

    pub fn write<W: Write>(&self, write: &mut W) -> io::Result<()> {
        if self.data.len() > u32::MAX as _ {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "More that u32::MAX hash entries are not supported",
            ));
        }

        let header = HashesHeader {
            size: self.data.len() as _,
            sort: self.sort,
            name_hash: self.name_hash,
            data_hash: self.data_hash,
        };
        write.write_all(header.to_array().get_ref())?;

        let data_bytes = unsafe { Cow::Borrowed(self.data.as_slice().align_to::<u8>().1) };

        //todo fix any endianess issues?
        //let data_bytes = Self::fix_endianness_write(data_bytes);

        write.write_all(data_bytes.as_ref())
    }

    pub fn verify_sorted(&self) -> bool {
        self.data.as_slice().windows(2).all(|w| w[0].cmp(&w[1]) != Ordering::Greater)
    }

    pub fn verify_update_sorted(&mut self) {
        self.sort = if self.verify_sorted() {
            SortOrder::SortedByName
        } else {
            SortOrder::Unordered
        };
    }

    pub fn sort(&mut self) {
        self.data.sort_unstable();
        self.sort = SortOrder::SortedByName;
    }
}

impl MeasureMemory for HashesChunk {
    fn memory_usage(&self) -> usize {
        self.data.capacity() * size_of::<DataEntry>()
    }
}

pub struct HashesIterChunk<R> {
    header: HashesHeader,
    reader: R,
    count: usize,
    start_data_pos: u64,
}

impl<R: Read + Seek> HashesIterChunk<R> {
    fn with_header(header: HashesHeader, mut reader: R, save_start: Option<bool>) -> io::Result<Self> {
        usize::try_from(header.size).map_err(|_| Error::new(ErrorKind::Unsupported, "File has too many entries"))?;
        let start_data_pos = match save_start {
            Some(true) => reader.stream_position()?,       //verbose save
            Some(false) => 0,                              //don't save anything
            None => reader.stream_position().unwrap_or(0), //implicit save, ignore error
        };
        Ok(Self {
            start_data_pos,
            header,
            reader,
            count: 0,
        })
    }

    pub fn new(mut reader: R) -> io::Result<Self> {
        Self::with_header(HashesHeader::read(&mut reader)?, reader, Some(true))
    }
}

impl<R: Read> Iterator for HashesIterChunk<R> {
    type Item = io::Result<DataEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == self.header.size as _ {
            return None;
        }
        self.count += 1;
        let mut entry = DataEntry::zero();
        Some(self.reader.read_exact(entry.as_mut_buf()).map(|_| entry).map_err(|e| {
            self.count = self.header.size as _; //mark as finished after first error
            e
        }))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let exact = self.header.size as _;
        (exact, Some(exact))
    }
}

impl<R: Read> ExactSizeIterator for HashesIterChunk<R> {}
