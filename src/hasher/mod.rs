mod file_iter;
mod names;
mod runner;
mod sum_file;

use digest::{Digest, FixedOutputReset};
use generic_array::GenericArray;
use parking_lot::Mutex;
use std::marker::PhantomData;
use std::mem::{align_of, transmute};
use std::ops::Index;
use std::path::{Path, PathBuf};
use std::slice::{from_raw_parts, from_raw_parts_mut};
use std::sync::atomic::AtomicU64;
use std::{
    cmp::Ordering,
    fmt::{Debug, LowerHex, UpperHex},
    io,
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

    pub fn aligned_data_chunks<'a>(&'a self, other: &'a Self) -> impl DoubleEndedIterator<Item = (DataChunk, DataChunk)> + 'a {
        let (_, a, _) = unsafe { self.array.align_to::<DataChunk>() };
        let (_, b, _) = unsafe { other.array.align_to::<DataChunk>() };
        assert_eq!(a.len(), b.len());
        let len = self.array.len() / size_of::<DataChunk>();
        assert_eq!(a.len(), len);
        a.iter().copied().zip(b.iter().copied())
    }

    pub fn aligned_chunks_mut(&mut self) -> &mut [DataChunk] {
        let len = self.array.len() / size_of::<DataChunk>();
        let (_, a, _) = unsafe { self.array.align_to_mut::<DataChunk>() };
        assert_eq!(a.len(), len);
        a
    }
    pub fn aligned_chunks(&self) -> &[DataChunk] {
        let len = self.array.len() / size_of::<DataChunk>();
        let (_, a, _) = unsafe { self.array.align_to::<DataChunk>() };
        assert_eq!(a.len(), len);
        a
    }

    pub fn wrapping_add(&self, other: Self) -> Self {
        let mut result = Self::zero();
        let mut carry = false;
        for ((a, b), r) in self.aligned_data_chunks(&other).zip(result.aligned_chunks_mut()) {
            let (add, c1) = DataChunk::from_le(a).overflowing_add(DataChunk::from_le(b));
            let (res, c2) = add.overflowing_add(carry as _);
            carry = c1 || c2;
            *r = res.to_le();
        }
        result
    }

    pub fn wrapping_sub(&self, other: Self) -> Self {
        let mut result = Self::zero();
        let mut carry = false;
        for ((a, b), r) in self.aligned_data_chunks(&other).zip(result.aligned_chunks_mut()) {
            let (add, c1) = DataChunk::from_le(a).overflowing_sub(DataChunk::from_le(b));
            let (res, c2) = add.overflowing_sub(carry as _);
            carry = c1 || c2;
            *r = res.to_le();
        }
        result
    }

    pub fn checked_div_rem(&self, b: u64) -> Option<(Self, u64)> {
        if b == 0 {
            return None;
        }
        let mut a = *self;

        let mut rem = 0;

        if b <= HALF {
            for d in a.aligned_chunks_mut().iter_mut().rev() {
                let (q, r) = Self::div_half(rem, *d, b);
                *d = q;
                rem = r;
            }
        } else {
            for d in a.aligned_chunks_mut().iter_mut().rev() {
                let (q, r) = Self::div_wide(rem, *d, b);
                *d = q;
                rem = r;
            }
        }

        Some((a, rem))
    }

    pub fn not(&self) -> Self {
        let mut val = *self;
        val.aligned_chunks_mut().iter_mut().for_each(|v| *v = !*v);
        val
    }

    pub fn to_sign_reduced(&self) -> Self {
        let mut first_bit = false;
        let mut result = *self;
        if result.aligned_chunks().last().unwrap() & LAST_BIT != 0 {
            first_bit = true;
            result = result.not()
        }

        for r in result.aligned_chunks_mut().iter_mut() {
            let v = DataChunk::from_le(*r);
            *r = v.wrapping_shl(1) | if first_bit { 1 } else { 0 };
            first_bit = v & LAST_BIT != 0
        }
        result
    }

    #[inline]
    fn div_half(rem: DataChunk, digit: DataChunk, divisor: DataChunk) -> (DataChunk, DataChunk) {
        debug_assert!(rem < divisor && divisor <= HALF);
        let v = (rem << HALF_BITS) | (digit >> HALF_BITS);
        let (hi, rem) = (v / divisor, v % divisor);
        let v = (rem << HALF_BITS) | (digit & HALF);
        let (lo, rem) = (v / divisor, v % divisor);
        ((hi << HALF_BITS) | lo, rem)
    }

    #[inline]
    fn div_wide(hi: DataChunk, lo: DataChunk, divisor: DataChunk) -> (DataChunk, DataChunk) {
        debug_assert!(hi < divisor);

        let lhs = u128::from(lo) | (u128::from(hi) << DataChunk::BITS);
        let rhs = u128::from(divisor);
        ((lhs / rhs) as DataChunk, (lhs % rhs) as DataChunk)
    }
}

pub(crate) const HALF_BITS: u8 = (DataChunk::BITS as u8) / 2;
pub(crate) const HALF: DataChunk = (1 << HALF_BITS) - 1;
const LAST_BIT: DataChunk = (1 << (DataChunk::BITS - 1));

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
        for (a, b) in self.aligned_data_chunks(other).rev() {
            let res = a.cmp(&b);
            if res.is_ne() {
                return res;
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
    type NameState<'a>;
    type FileState<'a>;

    fn consume_name<'a>(&self, path: &'a Path) -> Self::NameState<'a>;

    fn start_file(&self) -> Self::FileState<'_>;

    fn update_file<'a>(&'a self, state: &mut Self::FileState<'a>, data: &[u8]);

    fn finish_consume(&self, name: Self::NameState<'_>, file: Self::FileState<'_>);

    fn on_error(&self, error: io::Error, path: &Path) {
        let file = path.to_string_lossy();
        println!("Error reading file \"{file}\" => {error}");
    }
}

pub struct DigestConsumer<const ID: usize, const DATA: usize, D: Digest, F: Fn(HashEntry<ID, DATA>)> {
    consume: F,
    total_bytes: AtomicU64,
    _phantom: PhantomData<D>,
}

impl<const ID: usize, const DATA: usize, D: Digest, F: Fn(HashEntry<ID, DATA>)> DigestConsumer<ID, DATA, D, F> {
    pub fn new(consume: F) -> Self {
        Self {
            consume,
            total_bytes: AtomicU64::new(0),
            _phantom: PhantomData,
        }
    }
    pub fn get_total_bytes(&self) -> u64 {
        self.total_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl<const ID: usize, const DATA: usize, D: Digest, F: Fn(HashEntry<ID, DATA>)> Consumer for DigestConsumer<ID, DATA, D, F> {
    type NameState<'a> = HashArray<ID>;
    type FileState<'a> = D;

    fn consume_name<'a>(&self, path: &'a Path) -> Self::NameState<'a> {
        //todo review file name hashing
        let mut hasher = D::new_with_prefix(path.to_string_lossy().as_bytes());

        let mut name = HashArray::zero();
        hasher.finalize_into(GenericArray::from_mut_slice(name.get_mut()));
        name
    }

    fn start_file(&self) -> Self::FileState<'_> {
        D::new()
    }
    fn update_file(&self, state: &mut Self::FileState<'_>, data: &[u8]) {
        self.total_bytes.fetch_add(data.len() as _, std::sync::atomic::Ordering::Relaxed);
        state.update(data);
    }

    fn finish_consume(&self, name: Self::NameState<'_>, file: Self::FileState<'_>) {
        let mut entry = HashEntry {
            id: name,
            data: HashArray::zero(),
        };
        file.finalize_into(GenericArray::from_mut_slice(entry.data.get_mut()));
        (self.consume)(entry);
    }
}

pub struct HashZeroChunksFinder {
    pub min_size: u64,
    pub chunks: Mutex<Vec<PathBuf>>,
}

impl Consumer for HashZeroChunksFinder {
    type NameState<'a> = &'a Path;
    type FileState<'a> = (u64, Option<u64>, bool);

    fn consume_name<'a>(&self, path: &'a Path) -> Self::NameState<'a> {
        path
    }

    fn start_file(&self) -> Self::FileState<'_> {
        (0, None, false)
    }

    fn update_file(&self, state: &mut Self::FileState<'_>, data: &[u8]) {
        if state.2 {
            state.0 += data.len() as u64;
            return;
        }
        let mut iter = data.iter();
        while iter.len() > 0 {
            if let Some(pos) = state.1 {
                let len = iter.len();
                if let Some(end) = iter.by_ref().position(|&v| v != 0) {
                    let index = state.0 + end as u64;
                    if index - pos >= self.min_size {
                        state.2 = true;
                    } else {
                        state.1 = None;
                    }
                    state.0 = index + 1;
                } else {
                    state.0 += len as u64;
                }
            } else {
                let len = iter.len();
                if let Some(pos) = iter.by_ref().position(|&v| v == 0) {
                    let index = state.0 + pos as u64;
                    state.1 = Some(index);
                    state.0 = index + 1;
                } else {
                    state.0 += len as u64;
                }
            }
        }
    }

    fn finish_consume(&self, name: Self::NameState<'_>, file: Self::FileState<'_>) {
        if let Some(v) = file.1 {
            let footer_size = file.0 - v;
            if footer_size >= self.min_size {
                self.chunks.lock().push(name.to_path_buf());
            }
        }
    }
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

    #[test]
    fn test_zero_find() {
        let test = HashZeroChunksFinder {
            min_size: 10,
            chunks: Default::default(),
        };

        let mut state = test.start_file();
        test.update_file(&mut state, &[1, 2, 3, 4, 0, 0, 0, 0]);
        test.update_file(&mut state, &[0, 0, 0, 0, 0, 0, 1, 1, 1]);
        assert_eq!(state, (17, Some(4), true));

        let mut state = test.start_file();
        test.update_file(&mut state, &[1, 2, 3, 4, 0, 0, 0, 0]);
        test.update_file(&mut state, &[0, 0]);
        test.update_file(&mut state, &[0, 0, 0, 0, 1, 1, 1]);
        assert_eq!(state, (17, Some(4), true));

        let mut state = test.start_file();
        test.update_file(&mut state, &[1, 2, 3, 4, 0, 0, 0, 0]);
        test.update_file(&mut state, &[0, 0]);
        test.update_file(&mut state, &[0, 0, 0, 0, 1, 1, 1]);
        test.update_file(&mut state, &[1, 2, 3, 4, 0, 0, 0, 0]);
        test.update_file(&mut state, &[0, 0, 0, 0, 0, 0, 1, 1, 1]);
        assert_eq!(state, (34, Some(4), true));
    }
}
