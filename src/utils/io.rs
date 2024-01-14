use std::io;
use std::io::{ErrorKind, Read};

pub fn with_counted_read<R: Read, T, E: From<io::Error>>(
    read: &mut R,
    count: &mut u64,
    func: impl FnOnce(&mut dyn Read) -> Result<T, E>,
) -> Result<T, E> {
    struct StreamCountWrapper<'a, R>(&'a mut R, &'a mut u64, bool);
    impl<R: Read> Read for StreamCountWrapper<'_, R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let res = self.0.read(buf);
            match &res {
                Ok(count) => *self.1 += *count as u64,
                Err(err) if err.kind() != ErrorKind::Interrupted => self.2 = true, //register error
                _ => {}
            }
            res
        }
    }
    //count how many bytes was read from stream
    let mut wrapper = StreamCountWrapper(read, count, false);
    let result = func(&mut wrapper)?;
    if wrapper.2 {
        //if there was unpropagated error, raise it here.
        return Err(io::Error::new(ErrorKind::Other, "IO Error was ignored by file codec").into());
    }
    Ok(result)
}
