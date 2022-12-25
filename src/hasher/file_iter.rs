use compress::bwt::*;
use flate2::Compression;
use std::fs::{read_dir, DirEntry, ReadDir};
use std::io::Write;
use std::path::Path;

pub struct DepthFileIter {
    stack: Vec<ReadDir>,
}

impl DepthFileIter {
    pub fn from_dir<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Ok(Self {
            stack: vec![read_dir(path)?],
        })
    }
}

pub fn depth_first_files<P: AsRef<Path>>(path: P) -> std::io::Result<DepthFileIter> {
    DepthFileIter::from_dir(path)
}

impl Iterator for DepthFileIter {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        'outer: loop {
            let iter = self.stack.last_mut()?;
            while let Some(elem) = iter.next() {
                let Ok(elem) = elem else { continue; };
                let Ok(fty) = elem.file_type() else { continue; };
                if fty.is_dir() {
                    if let Ok(iter) = read_dir(elem.path()) {
                        self.stack.push(iter);
                        continue 'outer;
                    }
                }
                return Some(elem);
            }
            self.stack.pop();
        }
    }
}

fn compress_text(text: &[u8]) -> Vec<u8> {
    // let mut enc = Encoder::new(Vec::new(),4 << 20);
    // enc.write(text);
    // let vec: Vec<u8> = enc.finish().0;
    let mut zip = flate2::write::DeflateEncoder::new(Vec::new(), Compression::best());
    zip.write(text).unwrap();
    zip.flush_finish().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hasher::names::{FileNames, FlatedFileNames, NamesStorage};
    use crate::hasher::runner::HashRunner;
    use crate::hasher::sum_file::{read_vec_bytes, write_vec_bytes};
    use crate::hasher::{Consumer, HashEntry};
    use flate2::Compression;
    use parking_lot::Mutex;
    use sha2::Sha256;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::BufWriter;
    use std::mem::size_of_val;
    use std::sync::Arc;
    use std::thread::sleep;
    use std::time::Duration;
    use std::{
        mem::size_of,
        path::{Path, PathBuf},
    };

    use super::*;

    #[test]
    fn test_list_files() {
        let path = Path::new(".");

        let paths = depth_first_files(path).unwrap().map(|d| d.path().to_string_lossy().into_owned());
        let mut names = FlatedFileNames::new(Compression::best());
        let ids = names.with_collected(paths).collect::<Vec<_>>();
        println!("Count: {}", ids.len());
        //println!("Paths: {paths:?}");
        // let bytes = paths.iter().map(|v| v.capacity()).sum::<usize>();
        // let bytes = bytes + paths.len() * size_of::<PathBuf>();
        println!("Bytes used: {}", names.total_len());
        //println!("Name: {}", names.get(ids[3]).unwrap());

        //let comp = compress_text(names.total_str().as_bytes());

        let comp = names.finish();

        println!("Compressed length: {}", comp.len());
    }

    #[test]
    fn test_runner() {
        let path = Path::new("test_files");

        println!("Scanning path: {:?}", path);
        let paths = depth_first_files(path).unwrap().map(|d| d.path());

        #[derive(Default)]
        struct Cons(Mutex<Vec<HashEntry<32, 32>>>);
        impl Consumer for Cons {
            fn consume(&self, value: HashEntry<32, 32>) {
                self.0.lock().push(value);
            }
        }
        let cons = Arc::new(Cons::default());

        let runner = HashRunner::run::<_, Sha256, _>(paths, cons.clone(), 128);
        //sleep(Duration::from_secs(5));
        runner.wait_for_finish();

        let mut vals = cons.0.lock();
        println!("Vals: {}", vals.len());
        let bytes = size_of_val(vals.as_slice()) as f64 / (1024.0 * 1024.0);
        println!("Memory taken: {bytes:.3}Mb");

        vals.sort_unstable_by(|a, b| a.cmp(b));
        println!("first: {:?}", vals.first().unwrap());
        println!("last:  {:?}", vals.last().unwrap());
        write_vec_bytes("hashes.hsum", &vals);
    }

    #[test]
    fn test_read_sum_file() {
        let path = Path::new("tdev1.raw.hsum");
        //let path = Path::new("tmf1.raw.hsum");

        let vals = read_vec_bytes(path).unwrap();

        println!("first: {:?}", vals.first().unwrap());
        println!("last:  {:?}", vals.last().unwrap());

        let mut dupes = HashMap::new();
        for e in &vals {
            dupes.entry(e.data).or_insert_with(Vec::new).push(e.id);
        }
        let mut dupes = dupes.into_iter().filter(|(_, v)| v.len() > 1).collect::<Vec<_>>();
        dupes.sort_unstable();
        println!("Duplicates: {}", dupes.len());
        let dc = dupes.iter().map(|(_, v)| v.len()).collect::<Vec<_>>();
        println!("Sizes: [max: {}] {:?}", dc.iter().reduce(|a, b| a.max(b)).unwrap_or(&0), dc);
        // for (data, names) in dupes {
        //     println!("data: {data:?} names: {names:?}");
        // }
    }
}
