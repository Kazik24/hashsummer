use crate::file::StdHashArray;
use std::io;
use std::io::{ErrorKind, Read};

pub fn read_first_data_chunk<R: Read>(read: &mut R) -> io::Result<Option<StdHashArray>> {
    let mut header_array = StdHashArray::zero();
    match read.read_exact(header_array.as_bytes_mut()) {
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
        Ok(()) => Ok(Some(header_array)),
    }
}
