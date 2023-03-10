mod file_iter;
mod names;
mod runner;
mod sum_file;

use digest::{Digest, FixedOutputReset};
use std::mem::{align_of, transmute};
use std::ops::Index;
use std::slice::{from_raw_parts, from_raw_parts_mut};
use std::{
    cmp::Ordering,
    fmt::{Debug, LowerHex, UpperHex},
    mem::size_of,
};

pub use file_iter::*;
pub use names::*;
pub use runner::*;
pub use sum_file::*;

pub type DataChunk = u64;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
#[repr(align(8))]
pub struct HashArray<const N: usize> {
    array: [u8; N],
}

const _: () = {
    assert!(size_of::<HashArray<32>>() == 32);
    assert!(align_of::<HashArray<32>>() >= align_of::<DataChunk>());
    // assert correct transmute layout when converting to byte array
    // bytes 0 to 31 must containt id field
    // bytest 32 to 63 must containt data field
    // this ensures that vector of HashEntry<32,32> can be written exactly as bytes to file
    let mut a = HashEntry::<32, 32>::zero();
    a.id.array[0] = 1;
    a.data.array[0] = 2;
    let array = unsafe { transmute::<_, [u8; 64]>(a) };
    assert!(array[0] == 1);
    assert!(array[32] == 2);
    assert!(array[63] == 0);
    assert!(array[31] == 0);
};

impl<const N: usize> HashArray<N> {
    pub const fn zero() -> Self {
        Self { array: [0; N] }
    }
    pub const fn new(array: [u8; N]) -> Self {
        Self { array }
    }

    pub const fn parse_hex(val: &str, big_endian: bool) -> Self {
        let mut i = 0;
        let val = val.as_bytes();
        let mut array = [0u8; N];
        while i < N {
            let num = (Self::hex_digit(val[i * 2]) << 4) | Self::hex_digit(val[i * 2 + 1]);
            if big_endian {
                array[N - i - 1] = num;
            } else {
                array[i] = num;
            }
            i += 1;
        }
        Self { array }
    }

    pub const fn put_bytes(mut self, mut pos: usize, mut bytes: &[u8]) -> Self {
        let end = pos + bytes.len();
        let mut i = 0;
        while pos < end {
            self.array[pos] = bytes[i];
            pos += 1;
            i += 1;
        }
        self
    }

    const fn hex_digit(b: u8) -> u8 {
        match Self::match_hex_digit(b) {
            Ok(v) => v,
            Err(_) => panic!("Cannot parse hex digit"),
        }
    }
    const fn match_hex_digit(b: u8) -> Result<u8, u8> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            v => Err(v),
        }
    }

    pub fn parse_fill_zero(value: &str) -> Self {
        if value.len() < N * 2 {
            let mut s = "0".repeat(N * 2 - value.len());
            s.push_str(value);
            Self::parse_hex(&s, false)
        } else {
            Self::parse_hex(value, false)
        }
    }
    #[inline]
    pub const fn get_ref(&self) -> &[u8; N] {
        &self.array
    }
    #[inline]
    pub fn get_mut(&mut self) -> &mut [u8; N] {
        &mut self.array
    }
    #[inline]
    pub fn get_u16(&self, index: usize) -> u16 {
        u16::from_le_bytes(self.get_slice::<{ size_of::<u16>() }>(index))
    }
    #[inline]
    pub fn get_u32(&self, index: usize) -> u32 {
        u32::from_le_bytes(self.get_slice::<{ size_of::<u32>() }>(index))
    }
    #[inline]
    pub fn get_u64(&self, index: usize) -> u64 {
        u64::from_le_bytes(self.get_slice::<{ size_of::<u64>() }>(index))
    }

    #[inline]
    pub fn get_slice<const B: usize>(&self, index: usize) -> [u8; B] {
        self.array[index..(index + B)].try_into().unwrap()
    }

    #[inline]
    pub fn set_u16(&mut self, index: usize, value: u16) {
        self.set_slice::<{ size_of::<u16>() }>(index, value.to_le_bytes());
    }
    #[inline]
    pub fn set_u32(&mut self, index: usize, value: u32) {
        self.set_slice::<{ size_of::<u32>() }>(index, value.to_le_bytes());
    }
    #[inline]
    pub fn set_u64(&mut self, index: usize, value: u64) {
        self.set_slice::<{ size_of::<u64>() }>(index, value.to_le_bytes());
    }
    #[inline]
    pub fn set_slice<const B: usize>(&mut self, index: usize, data: [u8; B]) {
        self.array[index..(index + B)].copy_from_slice(&data);
    }

    pub fn top_bits(&self) -> u64 {
        const BYTES: usize = size_of::<u64>();
        if N >= BYTES {
            u64::from_le_bytes(self.array[(N - BYTES)..].try_into().unwrap())
        } else {
            let mut array = [0u8; BYTES];
            array[..N].copy_from_slice(&self.array);
            u64::from_le_bytes(array)
        }
    }

    // todo little and big endians might get confused when writing bytes here on different platforms, and then comparing HashArray's
    pub fn as_bytes(&self) -> &[u8] {
        self.array.as_slice()
    }
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        self.array.as_mut_slice()
    }
}

impl<const N: usize> LowerHex for HashArray<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for d in self.array.iter().rev() {
            write!(f, "{:02x}", d)?;
        }
        Ok(())
    }
}

impl<const N: usize> UpperHex for HashArray<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for d in self.array.iter().rev() {
            write!(f, "{:02x}", d)?;
        }
        Ok(())
    }
}

impl<const N: usize> Debug for HashArray<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}", self)
    }
}

impl<const N: usize> PartialOrd for HashArray<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for HashArray<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        let (_, a, _) = unsafe { self.array.align_to::<DataChunk>() };
        let (_, b, _) = unsafe { other.array.align_to::<DataChunk>() };
        assert_eq!(a.len(), b.len());
        let len = self.array.len() / size_of::<DataChunk>();
        assert_eq!(a.len(), len);

        for i in (0..len).rev() {
            unsafe {
                let a = DataChunk::from_le(*a.get_unchecked(i));
                let b = DataChunk::from_le(*b.get_unchecked(i));
                let res = a.cmp(&b);
                if res.is_ne() {
                    return res;
                }
            }
        }
        Ordering::Equal
    }
}

//entries that are easy sortable
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Debug)]
#[repr(C)]
pub struct HashEntry<const ID: usize, const DATA: usize> {
    pub id: HashArray<ID>,     //for file name hash (full or relative path)
    pub data: HashArray<DATA>, //for file content hash
}

pub type DataEntry = HashEntry<32, 32>; //default hash entry size

impl<const ID: usize, const DATA: usize> HashEntry<ID, DATA> {
    pub const fn zero() -> Self {
        Self {
            id: HashArray::zero(),
            data: HashArray::zero(),
        }
    }
}

impl DataEntry {
    pub fn as_buf(&self) -> &[u8] {
        let size = self.id.array.len() + self.data.array.len();
        //id is first in struct
        unsafe { from_raw_parts(self.id.get_ref().as_ptr(), size) }
    }
    pub fn as_mut_buf(&mut self) -> &mut [u8] {
        let size = self.id.array.len() + self.data.array.len();
        //id is first in struct
        unsafe { from_raw_parts_mut(self.id.get_mut().as_mut_ptr(), size) }
    }
}

pub fn sort_by_id<const ID: usize, const DATA: usize>(array: &mut [HashEntry<ID, DATA>]) {
    //sort first by name, then by content
    array.sort_unstable()
}

pub trait HashDigest {
    type Output;
    fn new() -> Self;
    fn update(&mut self, array: &[u8]);
    fn finish(&self, output: &mut Self::Output);
    fn finish_reset(&mut self, output: &mut Self::Output);
}

pub trait Consumer {
    fn consume(&self, value: HashEntry<32, 32>);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    const MODEL_EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn test_array() {
        let mut array = HashArray::<4>::zero();

        array.as_bytes_mut()[1] = 1;

        println!("Arr: {:?}", array);
    }

    #[test]
    fn test_top_bits() {
        let mut arr = HashArray::<16>::zero();
        arr.as_bytes_mut()[1] = 3;
        arr.as_bytes_mut()[8] = 8;
        arr.as_bytes_mut()[9] = 1;
        assert_eq!(arr.top_bits(), 264);
        let mut arr = HashArray::<3>::zero();
        arr.as_bytes_mut()[0] = 3;
        arr.as_bytes_mut()[1] = 6;
        assert_eq!(arr.top_bits(), 1539);
    }

    #[test]
    fn test_eq() {
        let mut a = HashArray::<32>::zero();
        let mut b = HashArray::<32>::zero();

        let val = EMPTY_SHA256;

        let parsed = HashArray::<32>::parse_hex(MODEL_EMPTY_SHA256, false);

        println!("Empty hash: {:x}", EMPTY_SHA256);

        assert_eq!(parsed, EMPTY_SHA256, "Empty hashes differ");

        assert_eq!(a, b);
        a.as_bytes_mut()[0] = 1;
        assert!(a > b);
        b.as_bytes_mut()[7] = 1;
        assert!(a < b);
        a.as_bytes_mut()[8] = 1;
        assert!(a > b);
        b.as_bytes_mut()[15] = 1;
        assert!(a < b);
        a.as_bytes_mut()[17] = 1;
        assert!(a > b);
    }
}
