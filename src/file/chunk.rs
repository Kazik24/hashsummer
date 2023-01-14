use crate::file::BLOCK_HEADER_MAGIC;
use crate::{DataEntry, HashArray, HashEntry};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::io;
use std::io::{Error, ErrorKind, Read, Write};
use std::mem::size_of;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Hashes {
    pub data: Vec<DataEntry>,
    pub sorted: bool,
    pub name_hash: HashType,
    pub data_hash: HashType,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum HashType {
    Sha256,
}

macro_rules! impl_fingerprint {
    ($($value:ident => $array:literal),*) => {
        impl HashType {
            pub const FINGERPRINT_SIZE: usize = 8;
            pub fn get_fingerprint(&self) -> [u8; Self::FINGERPRINT_SIZE] {
                match self {
                    $(Self::$value => *$array,)*
                }
            }
            pub fn from_fingerprint(fp: [u8; Self::FINGERPRINT_SIZE]) -> Option<Self> {
                match &fp {
                    $($array => Some(Self::$value),)*
                    _ => None,
                }
            }
        }
    }
}

impl_fingerprint! {
    Sha256 => b"Sha256__"
}

struct HashesHeader {
    size: u64,
    sorted: bool,
    name_hash: HashType,
    data_hash: HashType,
}

impl HashesHeader {
    const SORTED_FLAG: u32 = 1;
    pub fn to_array(&self) -> HashArray<64> {
        let mut array = HashArray::zero();
        array.get_mut()[..BLOCK_HEADER_MAGIC.len()].copy_from_slice(BLOCK_HEADER_MAGIC);
        let mut flags = 0;
        flags |= if self.sorted { Self::SORTED_FLAG } else { 0 };
        array.set_u32(4, flags);
        array.set_u64(8, self.size);
        array
    }
    pub fn read<R: Read>(read: &mut R) -> io::Result<Self> {
        let mut header = HashArray::zero();
        read.read_exact(header.get_mut())?;
        Self::from_array(header)
    }
    pub fn from_array(array: HashArray<64>) -> io::Result<Self> {
        if &array.get_ref()[..BLOCK_HEADER_MAGIC.len()] != BLOCK_HEADER_MAGIC {
            return Err(Error::new(ErrorKind::InvalidData, "Block magic data doesn't match"));
        }
        let flags = array.get_u32(4);
        let size = array.get_u64(8);
        let sorted = (flags & Self::SORTED_FLAG) != 0;
        let name_hash = HashType::from_fingerprint(array.get_slice(16))
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Unknown name hash type fingerprint"))?;
        let data_hash = HashType::from_fingerprint(array.get_slice(24))
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Unknown data hash type fingerprint"))?;

        Ok(Self {
            sorted,
            size,
            name_hash,
            data_hash,
        })
    }
}

impl Hashes {
    pub fn new_sha256(data: Vec<DataEntry>, sorted: bool) -> Self {
        Self {
            data,
            sorted,
            name_hash: HashType::Sha256,
            data_hash: HashType::Sha256,
        }
    }
    pub fn read<R: Read>(read: &mut R) -> io::Result<Self> {
        let header = HashesHeader::read(read)?;
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
            sorted: header.sorted,
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
            sorted: self.sorted,
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
        self.sorted = self.verify_sorted();
    }

    pub fn sort(&mut self) {
        self.data.sort_unstable();
        self.sorted = true;
    }
}
