mod file_iter;
mod names;
mod runner;
mod sum_file;

use digest::{Digest, FixedOutputReset};
use std::ops::Index;
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

    pub const fn get_ref(&self) -> &[u8; N] {
        &self.array
    }
    pub fn get_mut(&mut self) -> &mut [u8; N] {
        &mut self.array
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
pub struct HashEntry<const ID: usize, const DATA: usize> {
    pub id: HashArray<ID>,     //for file name hash (full or relative path)
    pub data: HashArray<DATA>, //for file content hash
}

impl<const ID: usize, const DATA: usize> HashEntry<ID, DATA> {
    pub fn zero() -> Self {
        Self {
            id: HashArray::zero(),
            data: HashArray::zero(),
        }
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
