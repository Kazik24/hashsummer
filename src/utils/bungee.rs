use std::io::Read;
use std::iter::repeat;
use std::marker::PhantomData;
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::ops::{Add, Sub};
use std::path::PathBuf;
use std::str::from_utf8;

pub struct BungeeBytes<T: OffsetInt> {
    data: Vec<u8>,
    _phantom: PhantomData<T>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BungeeIndex {
    index: NonZeroUsize,
}

// fn xx(){
//     let b = &[1u8,2,3];
//     aaa(b.iter());
// }
// fn aaa(r: impl Read){
//
// }

pub trait OffsetInt: Copy + Eq {
    const MAX_BYTES: usize;
    ///read value in reverse (starting from last byte in slice), returns value and number of bytes read
    fn reverse_read(data: &[u8]) -> (Self, usize);
    ///read value in regular order, returns value and number of bytes read
    fn read(data: &[u8]) -> (Self, usize);
    fn write(self, data: &mut [u8]) -> usize;

    fn from_usize(val: usize) -> Self;
    fn to_usize(self) -> usize;
}

impl<T: OffsetInt> BungeeBytes<T> {
    pub const fn new() -> Self {
        Self {
            data: Vec::new(),
            _phantom: PhantomData,
        }
    }

    fn ensure_space(&mut self, space: usize) -> &mut [u8] {
        let pos = self.data.len();
        self.data.extend(repeat(0u8).take(space));
        &mut self.data[pos..]
    }

    fn with_ensured_space(&mut self, max_space: usize, func: impl FnOnce(&mut [u8]) -> usize) -> usize {
        let pos = self.data.len();
        self.data.extend(repeat(0u8).take(max_space));
        let count = func(&mut self.data[pos..]);
        let pos = pos + count;
        self.data.truncate(pos);
        pos
    }

    pub fn last_index(&self) -> Option<BungeeIndex> {
        NonZeroUsize::new(self.data.len()).map(|index| BungeeIndex { index })
    }

    pub fn raw_bytes(&self) -> &[u8] {
        self.data.as_slice()
    }

    fn reverse_read(&self, at: BungeeIndex) -> (&[u8], Option<BungeeIndex>, Option<BungeeIndex>) {
        let mut slice = &self.data.as_slice()[..at.index.get()];
        let (data_len, count) = T::reverse_read(slice);
        let data_len = data_len.to_usize();
        let data_range = {
            let end = slice.len() - count;
            (end - data_len)..end
        };
        let (prev_index, count) = T::reverse_read(&slice[..data_range.start]);
        let skip = data_range.start - count;
        let skip_pos = NonZeroUsize::new(skip).map(|index| BungeeIndex { index });
        let prev_pos = NonZeroUsize::new(skip - prev_index.to_usize()).map(|index| BungeeIndex { index });
        (&slice[data_range], skip_pos, prev_pos)
    }

    pub fn reverse_skip(&self, at: BungeeIndex) -> (&[u8], Option<BungeeIndex>) {
        let (data, skip, _prev) = self.reverse_read(at);
        (data, skip)
    }

    pub fn reverse_follow(&self, at: BungeeIndex) -> (&[u8], Option<BungeeIndex>) {
        let (data, _skip, prev) = self.reverse_read(at);
        (data, prev)
    }

    pub fn reverse_follow_collect<'a>(&'a self, at: BungeeIndex, to: &mut Vec<&'a [u8]>) {
        let mut current = at;
        loop {
            let (data, prev) = self.reverse_follow(current);
            to.push(data);
            match prev {
                Some(at) => current = at,
                None => break,
            }
        }
    }

    pub fn push(&mut self, prev: Option<BungeeIndex>, data: &[u8]) -> Option<BungeeIndex> {
        if data.is_empty() {
            return prev;
        }

        let pos = self.data.len();
        //at least this amount of memory is required
        let pos = self.with_ensured_space(T::MAX_BYTES * 2 + data.len(), |mut slice| {
            let mut count = 0;
            //write previous reference as relative offset
            let prev = prev.map(|v| v.index.get()).unwrap_or(0); //todo
            let prev = T::from_usize(pos - prev);
            let len = prev.write(slice);
            count += len;
            slice = &mut slice[len..];

            //write stored data
            slice[..data.len()].copy_from_slice(data);
            count += data.len();
            //write length of stored data
            let len = T::from_usize(data.len()).write(&mut slice[data.len()..]);
            count += len;
            count
        });
        Some(BungeeIndex {
            index: NonZeroUsize::new(pos).unwrap(),
        })
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FixedInt<T>(T);

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VarInt<T>(T);

macro_rules! impl_fixed_int {
    ($($name:ident),*) => {
        $(
        impl OffsetInt for FixedInt<$name> {
            const MAX_BYTES: usize = size_of::<$name>();

            fn reverse_read(data: &[u8]) -> (Self, usize) {
                let len = data.len() - Self::MAX_BYTES;
                let arr: [u8;Self::MAX_BYTES] = data[len..].try_into().unwrap();
                (Self($name::from_le_bytes(arr)),Self::MAX_BYTES)
            }

            fn read(data: &[u8]) -> (Self, usize) {
                let arr: [u8;Self::MAX_BYTES] = data[..Self::MAX_BYTES].try_into().unwrap();
                (Self($name::from_le_bytes(arr)),Self::MAX_BYTES)
            }

            fn write(self, data: &mut [u8]) -> usize {
                let arr = self.0.to_le_bytes();
                data[..arr.len()].copy_from_slice(&arr);
                Self::MAX_BYTES
            }
            fn from_usize(val: usize) -> Self { Self(val as _) }
            fn to_usize(self) -> usize { self.0 as _ }
        }
        )*
    }
}

impl_fixed_int!(u8, u16, u32, u64, usize);

type NAME = usize;

impl OffsetInt for VarInt<NAME> {
    const MAX_BYTES: usize = size_of::<NAME>() + 1;

    fn reverse_read(data: &[u8]) -> (Self, usize) {
        todo!()
    }

    fn read(data: &[u8]) -> (Self, usize) {
        let mut value = 0;
        let mut count = 0;
        while count < Self::MAX_BYTES {
            let byte = data[count];
            value |= ((byte & 0x7f) as NAME) << (count * 7);
            count += 1;
            if byte & 0x80 == 0 {
                break;
            }
        }
        (VarInt(value), count)
    }

    fn write(self, data: &mut [u8]) -> usize {
        todo!()
    }
    fn from_usize(val: usize) -> Self {
        Self(val as _)
    }
    fn to_usize(self) -> usize {
        self.0 as _
    }
}

pub struct BungeeStr {
    inner: BungeeBytes<FixedInt<usize>>,
}

impl BungeeStr {
    pub const fn new() -> Self {
        Self { inner: BungeeBytes::new() }
    }

    pub fn last_index(&self) -> Option<BungeeIndex> {
        self.inner.last_index()
    }

    pub fn push(&mut self, prev: Option<BungeeIndex>, data: &str) -> Option<BungeeIndex> {
        self.inner.push(prev, data.as_bytes())
    }

    pub fn reverse_skip(&self, at: BungeeIndex) -> (&str, Option<BungeeIndex>) {
        let (data, skip) = self.inner.reverse_skip(at);
        (from_utf8(data).unwrap(), skip)
    }

    pub fn reverse_follow(&self, at: BungeeIndex) -> (&str, Option<BungeeIndex>) {
        let (data, prev) = self.inner.reverse_follow(at);
        (from_utf8(data).unwrap(), prev)
    }

    pub fn path_of(&self, sep: &str, at: BungeeIndex) -> String {
        let mut parts = &mut Vec::new();
        self.inner.reverse_follow_collect(at, &mut parts);
        let bytes: usize = parts.iter().map(|v| v.len()).sum();
        let bytes = bytes + sep.len() * parts.len().saturating_sub(1);
        let mut result = String::with_capacity(bytes);
        let mut it = parts.into_iter().rev();
        if let Some(v) = it.next() {
            result.push_str(from_utf8(v).unwrap());
        }
        for v in it {
            result.push_str(sep);
            result.push_str(from_utf8(v).unwrap());
        }
        result
    }

    pub fn raw_bytes(&self) -> &[u8] {
        self.inner.raw_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::slice::EscapeAscii;

    #[test]
    fn test_bungee() {
        let mut bungee = BungeeBytes::<FixedInt<u8>>::new();
        assert!(bungee.push(None, b"").is_none());
        let i1 = bungee.push(None, b"1234").unwrap();
        let i2 = bungee.push(None, b"5678910").unwrap();
        let i3 = bungee.push(Some(i1), b"value").unwrap();

        println!("{}", bungee.raw_bytes().escape_ascii());

        let (val, idx) = bungee.reverse_follow(i1);
        assert_eq!(val, b"1234");
        assert_eq!(idx, None);
        let (val, idx) = bungee.reverse_follow(i2);
        assert_eq!(val, b"5678910");
        assert_eq!(idx, None);
        let (val, idx) = bungee.reverse_follow(i3);
        assert_eq!(val, b"value");
        assert_eq!(idx, Some(i1));
        let (val, idx) = bungee.reverse_skip(i3);
        assert_eq!(val, b"value");
        assert_eq!(idx, Some(i2));
        let (val, idx) = bungee.reverse_skip(i2);
        assert_eq!(val, b"5678910");
        assert_eq!(idx, Some(i1));
        let (val, idx) = bungee.reverse_skip(i1);
        assert_eq!(val, b"1234");
        assert_eq!(idx, None);
    }
}
