use crate::hasher::HashEntry;
use std::fs::File;
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::mem::size_of;
use std::path::Path;

pub const VERSION: (u8, u8, u8) = (0, 0, 1);

pub struct SumFileHeader {
    array: [u8; 64],
}

impl Default for SumFileHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl SumFileHeader {
    pub fn new() -> Self {
        Self { array: todo!() }
    }
}

#[derive(Copy, Clone)]
pub struct Flags(u8);

pub fn write_hash_array<W: Write, const A: usize, const B: usize>(writer: &mut W, array: &[HashEntry<A, B>]) -> std::io::Result<()> {
    for v in array {
        writer.write_all(v.id.get_ref())?;
        writer.write_all(v.data.get_ref())?;
    }
    Ok(())
}

pub fn read_hash_array<R: Read, const A: usize, const B: usize>(
    reader: &mut R,
    array: &mut Vec<HashEntry<A, B>>,
    count: Option<usize>,
) -> std::io::Result<usize> {
    let mut cr = 0;
    let mut entry = HashEntry::zero();
    let mut to_read = count.unwrap_or(usize::MAX);
    while to_read != 0 {
        to_read -= 1;
        match reader.read_exact(entry.id.get_mut()) {
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            v => v?,
        }
        match reader.read_exact(entry.data.get_mut()) {
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            v => v?,
        }
        array.push(entry);
        cr += 1;
    }
    Ok(cr)
}

pub fn write_vec_bytes(path: impl AsRef<Path>, array: &[HashEntry<32, 32>]) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::options().write(true).truncate(true).create(true).open(path)?);
    write_hash_array(&mut file, array)?;
    file.flush()?;
    Ok(())
}

pub fn read_vec_bytes(path: impl AsRef<Path>) -> std::io::Result<Vec<HashEntry<32, 32>>> {
    let mut file = BufReader::new(File::open(path)?);
    let len = file.get_ref().metadata()?.len();
    let count = len / size_of::<HashEntry<32, 32>>() as u64;
    let mut array = Vec::with_capacity((count as usize).min(1024 * 1024));
    read_hash_array(&mut file, &mut array, None)?;
    Ok(array)
}
