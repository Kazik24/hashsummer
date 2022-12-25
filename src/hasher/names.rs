use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::collections::TryReserveError;
use std::io;
use std::io::{BufWriter, Read, Write};

pub struct FileNames {
    string: String,
}

#[derive(Copy, Clone)]
pub struct NameRefId {
    pos: u32,
    len: u32,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum PushError {
    LengthOverflow,
    SizeOverflow,
    Mem(TryReserveError),
}

pub trait NamesStorage {
    fn try_push(&mut self, name: &str) -> Result<NameRefId, PushError>;

    fn push(&mut self, name: &str) -> NameRefId {
        self.try_push(name).unwrap()
    }
    fn total_len(&self) -> usize;

    fn with_collected<I>(&mut self, it: I) -> WithCollected<I::IntoIter, Self>
    where
        Self: Sized,
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        WithCollected {
            parent: self,
            it: it.into_iter(),
        }
    }
}

impl FileNames {
    pub fn new() -> Self {
        Self { string: String::new() }
    }
    pub fn total_capacity(&self) -> usize {
        self.string.capacity()
    }

    pub fn get(&self, id: NameRefId) -> Option<&str> {
        let start: usize = id.pos.try_into().unwrap();
        let len: usize = id.len.try_into().unwrap();
        self.string.get(start..(start + len))
    }

    pub fn total_str(&self) -> &str {
        &self.string
    }
}

impl NamesStorage for FileNames {
    fn try_push(&mut self, name: &str) -> Result<NameRefId, PushError> {
        let len = name.len().try_into().map_err(|_| PushError::LengthOverflow)?;
        let pos = self.string.len().try_into().map_err(|_| PushError::SizeOverflow)?;
        let result = NameRefId { len, pos };
        self.string.try_reserve(name.len()).map_err(|v| PushError::Mem(v))?;
        self.string.push_str(name);
        Ok(result)
    }

    fn total_len(&self) -> usize {
        self.string.len()
    }
}

pub struct FlatedFileNames {
    data: BufWriter<DeflateEncoder<Vec<u8>>>,
    pos: usize,
}

impl FlatedFileNames {
    pub fn new(level: Compression) -> Self {
        Self {
            data: BufWriter::new(DeflateEncoder::new(Vec::new(), level)),
            pos: 0,
        }
    }

    pub fn current_compressed_len(&self) -> usize {
        self.data.get_ref().total_out() as _
    }

    pub fn finish(self) -> Vec<u8> {
        self.data.into_inner().unwrap().flush_finish().unwrap()
    }

    pub fn decompress(mut data: &[u8]) -> io::Result<FileNames> {
        let mut string = String::with_capacity(data.len() * 4); //assume some starting capacity
        let mut v = DeflateDecoder::new(&mut data);
        v.read_to_string(&mut string)?;
        Ok(FileNames { string })
    }
}

impl NamesStorage for FlatedFileNames {
    fn try_push(&mut self, name: &str) -> Result<NameRefId, PushError> {
        let len = name.len().try_into().map_err(|_| PushError::LengthOverflow)?;
        let pos = self.pos.try_into().map_err(|_| PushError::SizeOverflow)?;
        let result = NameRefId { len, pos };
        self.pos = self.pos.checked_add(name.len()).ok_or_else(|| PushError::SizeOverflow)?;
        self.data.write(name.as_bytes()).unwrap();
        Ok(result)
    }

    fn total_len(&self) -> usize {
        self.data.buffer().len() + self.data.get_ref().total_in() as usize
    }
}

pub struct WithCollected<'a, I, N: NamesStorage> {
    parent: &'a mut N,
    it: I,
}

impl<'a, I, N> Iterator for WithCollected<'a, I, N>
where
    I: Iterator,
    N: NamesStorage,
    I::Item: AsRef<str>,
{
    type Item = NameRefId;
    fn next(&mut self) -> Option<Self::Item> {
        let elem = self.it.next()?;
        Some(self.parent.push(elem.as_ref()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }
}
