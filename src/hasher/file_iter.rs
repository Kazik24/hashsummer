use crate::utils::{BungeeIndex, BungeeStr};
use compress::bwt::*;
use flate2::Compression;
use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::fmt::{Display, Formatter};
use std::fs::{read_dir, DirEntry, FileType, ReadDir};
use std::io::Write;
use std::iter::once;
use std::path::{Path, PathBuf};

pub struct DepthFileScanner {
    root: PathBuf,
    current: Vec<OsString>,
    stack: Vec<ReadDir>,
}

impl DepthFileScanner {
    //todo multi root
    pub fn from_dir<P: AsRef<Path>>(path: P) -> Self {
        Self {
            root: path.as_ref().to_path_buf(),
            current: Vec::new(),
            stack: read_dir(path).ok().into_iter().collect(),
        }
    }

    pub fn reset_from_dir<P: AsRef<Path>>(&mut self, path: P) {
        self.root = path.as_ref().to_path_buf();
        self.current.clear();
        self.stack.clear();
        self.stack.extend(read_dir(path).ok());
    }

    pub fn into_iter(self) -> impl Iterator<Item = (DirEntry, FileType)> {
        struct Iter(DepthFileScanner);
        impl Iterator for Iter {
            type Item = (DirEntry, FileType);

            fn next(&mut self) -> Option<Self::Item> {
                self.0.next_file().map(|f| (f.entry, f.file_type))
            }
        }
        Iter(self)
    }

    pub fn iter(&mut self) -> impl Iterator<Item = (DirEntry, FileType)> + '_ {
        struct Iter<'a>(&'a mut DepthFileScanner);
        impl Iterator for Iter<'_> {
            type Item = (DirEntry, FileType);

            fn next(&mut self) -> Option<Self::Item> {
                self.0.next_file().map(|f| (f.entry, f.file_type))
            }
        }
        Iter(self)
    }

    pub fn save_to_bungee<F>(self, bungee: &mut BungeeStr, conv: F) -> SaveToBungee<F>
    where
        F: FnMut(&OsStr) -> Option<Cow<'_, str>>,
    {
        SaveToBungee {
            it: self,
            bungee,
            dirs: Vec::new(),
            name_convert: conv,
        }
    }
}

#[derive(Debug)]
pub struct FileEntry<'a> {
    /// root directory of file scanning (or one of roots)
    pub root: &'a Path,
    /// list of names in path before the name of this entry, excluding root
    pub before_name: &'a [OsString],
    /// if this entry is directory, then this field is a name of that directory
    pub dir_name: Option<&'a OsStr>,
    /// Os file type
    pub file_type: FileType,
    /// Os directory entry
    pub entry: DirEntry,
}

impl Display for FileEntry<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let path = self.path_without_root("/");
        if f.alternate() {
            write!(f, "{}", if self.is_dir() { "D:" } else { "F:" })?;
        }
        write!(f, "{}", path.to_string_lossy())
    }
}

impl FileEntry<'_> {
    pub fn get_name(&self) -> Cow<'_, OsStr> {
        match self.dir_name {
            Some(v) => Cow::Borrowed(v),
            None => Cow::Owned(self.entry.file_name()),
        }
    }

    pub fn path_without_root(&self, separator: impl AsRef<OsStr>) -> OsString {
        let separator = separator.as_ref();
        let mut s = OsString::new();
        let name = self.get_name();
        let mut path = self.before_name.iter().map(|v| v.as_os_str()).chain(once(name.as_ref()));
        if let Some(p) = path.next() {
            s.push(p);
        }
        for part in path {
            s.push(separator);
            s.push(part);
        }
        s
    }
    pub fn is_dir(&self) -> bool {
        self.dir_name.is_some()
    }
}

pub trait FileScanner {
    fn next_file(&mut self) -> Option<FileEntry>;
}

impl FileScanner for DepthFileScanner {
    fn next_file(&mut self) -> Option<FileEntry> {
        loop {
            let iter = self.stack.last_mut()?;
            for entry in iter {
                let Ok(entry) = entry else { continue; };
                let Ok(file_type) = entry.file_type() else { continue; };
                let mut dir_name = None;
                let before_name = if file_type.is_dir() {
                    if let Ok(iter) = read_dir(entry.path()) {
                        self.current.push(entry.file_name());
                        self.stack.push(iter);
                        dir_name = self.current.last().map(|v| v.as_os_str());
                        &self.current[..(self.current.len() - 1)]
                    } else {
                        self.current.as_slice()
                    }
                } else {
                    self.current.as_slice()
                };

                return Some(FileEntry {
                    root: &self.root,
                    before_name,
                    dir_name,
                    file_type,
                    entry,
                });
            }
            self.stack.pop();
            self.current.pop();
        }
    }
}

pub fn depth_first_files<P: AsRef<Path>>(path: P) -> DepthFileScanner {
    DepthFileScanner::from_dir(path)
}

pub struct SaveToBungee<'a, F> {
    it: DepthFileScanner,
    dirs: Vec<Option<BungeeIndex>>,
    bungee: &'a mut BungeeStr,
    name_convert: F,
}

impl<F> Iterator for SaveToBungee<'_, F>
where
    F: FnMut(&OsStr) -> Option<Cow<'_, str>>,
{
    type Item = (Option<BungeeIndex>, DirEntry, FileType);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let iter = self.it.stack.last_mut()?;
            let prev = self.dirs.last().copied().flatten();
            for elem in iter {
                let Ok(elem) = elem else { continue; };
                let Ok(fty) = elem.file_type() else { continue; };
                let name = elem.file_name();
                let Some(name) = (self.name_convert)(&name) else { continue; };
                let value = self.bungee.push(prev, name.as_ref());
                if fty.is_dir() {
                    if let Ok(iter) = read_dir(elem.path()) {
                        self.it.stack.push(iter);
                        self.dirs.push(value);
                    }
                }
                return Some((value, elem, fty));
            }
            self.it.stack.pop();
            self.dirs.pop();
        }
    }
}

fn compress_text(text: &[u8], use_burrows_wheeler: bool) -> Vec<u8> {
    let transform = if use_burrows_wheeler {
        let mut enc = Encoder::new(Vec::new(), 4 << 20);
        enc.write_all(text);
        Some(enc.finish().0)
    } else {
        None
    };
    let text = transform.as_deref().unwrap_or(text);
    let mut zip = flate2::write::DeflateEncoder::new(Vec::new(), Compression::best());
    zip.write_all(text).unwrap();
    zip.flush_finish().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::hashes_chunk::{HashesChunk, HashesIterChunk, SortOrder};
    use crate::store::{compress_sorted_entries, DiffResult, DiffingIter};
    use crate::utils::MeasureMemory;
    use crate::*;
    use digest::Digest;
    use flate2::Compression;
    use generic_array::GenericArray;
    use itertools::Itertools;
    use parking_lot::Mutex;
    use sha2::Sha256;
    use std::collections::{HashMap, HashSet};
    use std::fs::File;
    use std::io::BufWriter;
    use std::mem::{replace, size_of_val};
    use std::sync::Arc;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use std::{
        mem::size_of,
        path::{Path, PathBuf},
    };

    use super::*;

    #[test]
    fn test_list_files() {
        let path = Path::new(".");

        let mut scanner = DepthFileScanner::from_dir(path);
        let paths = scanner
            .iter()
            .filter(|(_, ty)| ty.is_file())
            .map(|(d, _)| d.path().to_string_lossy().into_owned());
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
    fn test_file_scanner() {
        let path = Path::new(".");
        let mut scanner = DepthFileScanner::from_dir(path);
        while let Some(entry) = scanner.next_file() {
            println!("{entry}");
        }
    }

    #[test]
    fn test_runner() {
        let path = Path::new("test_files");

        println!("Scanning path: {:?}", path);
        let paths = DepthFileScanner::from_dir(path)
            .into_iter()
            .filter(|(_, ty)| ty.is_file())
            .map(|(d, _)| d.path());

        let mutex: Arc<Mutex<Vec<HashEntry<32, 32>>>> = Default::default();
        let cons = {
            let mutex = mutex.clone();
            Arc::new(DigestConsumer::<32, 32, Sha256, _>::new(move |value| mutex.lock().push(value)))
            // Arc::new(HashZeroChunksFinder {
            //     min_size: 16000,
            //     chunks: Default::default(),
            // })
        };

        let runner = HashRunner::run(paths.into_iter(), cons.clone(), 128);
        //sleep(Duration::from_secs(5));
        runner.wait_for_finish();

        //println!("Found zero chunks in: {:#?}", cons.chunks.lock());

        let mut vals = mutex.lock();
        let mut vals = HashesChunk::new_sha256(replace(&mut *vals, Vec::new()), false);
        println!(
            "Processed: {}, total MB: {:.3} ({} bytes)",
            vals.data.len(),
            (cons.get_total_bytes() as f64) / (1024.0 * 1024.0),
            cons.get_total_bytes()
        );
        let bytes = size_of_val(vals.data.as_slice()) as f64 / (1024.0 * 1024.0);
        println!("Memory taken: {bytes:.3}Mb");

        vals.sort();
        println!("first: {:?}", vals.data.first().unwrap());
        println!("last:  {:?}", vals.data.last().unwrap());

        // let path = Path::new("hashes.hsum");
        // vals.write(&mut File::options().write(true).truncate(true).create(true).open(path).unwrap()).unwrap();
        // println!("Empty hash: {:?}", EMPTY_SHA256);
        // for val in vals.iter() {
        //     println!("{val:?}");
        // }
    }

    fn convert_to_hash_chunk_file(in_path: &Path, out_path: &Path) -> std::io::Result<()> {
        let data = read_vec_bytes(in_path)?;
        let mut hash = HashesChunk::new_sha256(data, false);
        hash.verify_update_sorted();
        let mut file = File::options().write(true).truncate(true).create(true).open(out_path)?;
        hash.write(&mut file)
    }

    #[test]
    fn test_read_sum_file() {
        let path = Path::new("tdev1.raw.hsum");
        //let path = Path::new("tmf1.raw.hsum");

        let vals = {
            let start = Instant::now();
            let mut file = File::open(path).unwrap();
            let h = HashesChunk::read(&mut file).unwrap();
            println!("Reading hash block: {:.3?}", start.elapsed());
            h
        };

        assert_eq!(vals.sort == SortOrder::SortedByName, vals.verify_sorted());

        println!(
            "Is sorted: {:?}, Name hash: {:?}, Data hash: {:?}",
            vals.sort, vals.name_hash, vals.data_hash
        );

        println!("first: {:?}", vals.data.first().unwrap());
        println!("last:  {:?}", vals.data.last().unwrap());

        let start = Instant::now();
        let mut dupes = HashMap::new();
        let mut empty = Vec::new();
        for e in &vals.data {
            if e.data == EMPTY_SHA256 {
                empty.push(e);
            }
            dupes.entry(e.data).or_insert_with(Vec::new).push(e.id);
        }

        let mut top_bits = HashMap::new();
        for a in &vals.data {
            let val = a.id.top_bits();
            let v = top_bits.entry(val >> 32).or_insert(0);
            *v += 1;
        }
        let same_top = top_bits.into_iter().filter(|v| v.1 > 1).map(|v| v.0).collect::<Vec<_>>();
        println!("Same top bits: {:?}", same_top);

        let mut dupes = dupes.into_iter().filter(|(_, v)| v.len() > 1).collect::<Vec<_>>();
        dupes.sort_unstable();
        println!("Empty files: {}", empty.len());
        println!("Duplicates: {}", dupes.len());
        let dc = dupes.iter().map(|(_, v)| v.len()).collect::<Vec<_>>();
        println!("Sizes: [max: {}] {:?}", dc.iter().reduce(|a, b| a.max(b)).unwrap_or(&0), dc);
        // for (data, names) in dupes {
        //     println!("data: {data:?} names: {names:?}");
        // }

        println!("Calc time {:.3?}", start.elapsed());
        let mut compressed = Vec::new();
        compress_sorted_entries(vals.data.iter().copied(), vals.data.len() as _, |v| &v.id, &mut compressed).unwrap();

        println!("compressed size: {}", compressed.len())
    }

    #[test]
    fn test_save_bungee() {
        let path = Path::new(".");

        println!("Scanning path: {:?}", path);
        let mut bungee = BungeeStr::new();
        let mut path_len = 0;
        let paths = DepthFileScanner::from_dir(path)
            .save_to_bungee(&mut bungee, |n| Some(n.to_string_lossy()))
            .inspect(|v| path_len += v.1.path().as_os_str().len())
            .filter_map(|(i, _, ty)| ty.is_file().then_some(i).flatten())
            .collect::<Vec<_>>();

        println!("Paths: ({}){:?}", paths.len(), paths);
        let names = paths.iter().map(|v| bungee.path_of("/", *v)).collect::<Vec<_>>();
        println!("Recovered paths: {:#?}", names);
        println!("Bungee size: {}", bungee.raw_bytes().len());

        let compressed = compress_text(bungee.raw_bytes(), false);
        println!("Bungee size after compression: {}", compressed.len());
        println!("total paths len: {}", path_len);

        let mut avg_sum = 0.0;
        for p in paths.iter().copied() {
            let (sum, count) = bungee
                .reverse_follow_iter(p)
                .tuple_windows::<(_, _)>()
                .map(|(p, n)| p.1.index.get() - n.1.index.get())
                .fold((0, 0), |mut s, v| {
                    s.0 += v;
                    s.1 += 1;
                    s
                });
            if count != 0 {
                let avg = sum as f64 / count as f64;
                avg_sum += avg;
            }
        }
        println!("Average path distances: {:.3}", avg_sum / paths.len() as f64);
    }

    fn file_names_hashed(path: impl AsRef<Path>) -> (BungeeStr, Vec<(BungeeIndex, HashArray<32>)>) {
        let mut bungee = BungeeStr::new();
        let files = depth_first_files(path)
            .save_to_bungee(&mut bungee, |n| Some(n.to_string_lossy()))
            .filter_map(|(i, e, ty)| Some((ty.is_file().then_some(i).flatten()?, e)))
            .map(|(i, entry)| {
                let mut array = HashArray::zero();
                let mut hasher = Sha256::new_with_prefix(entry.path().to_string_lossy().as_bytes());
                hasher.finalize_into(GenericArray::from_mut_slice(array.get_mut()));
                (i, array)
            })
            .collect::<Vec<_>>();
        println!("Bungee size: {}", bungee.raw_bytes().len());
        (bungee, files)
    }

    #[test]
    pub fn test_diff() {
        let f1 = Path::new("tmf1.raw.hsum");
        let f2 = Path::new("new_tmf1.hsum");
        let org_path = Path::new(".");

        let h1 = HashesChunk::read(&mut File::open(f1).unwrap()).unwrap();
        let h2 = HashesChunk::read(&mut File::open(f2).unwrap()).unwrap();
        //let h2 = HashesIterChunk::new(File::open(f2).unwrap()).unwrap();

        let (bungee, mut files) = file_names_hashed(org_path);
        let files = files.into_iter().map(|(a, b)| (b, a)).collect::<HashMap<_, _>>();

        let old = h1.data.iter();
        let new = h2.data.iter();
        println!("old size: {}, new size: {}, files len: {}", old.len(), new.len(), files.len());
        let diff = DiffingIter::new(old, new);
        let changed = diff.filter(|v| !matches!(v, DiffResult::Same(..))).collect::<Vec<_>>();
        println!("Changes: {}", changed.len());

        //println!("{changed:#?}");

        for ch in changed {
            let name_hash = ch.get_name();

            let Some(index) = files.get(name_hash) else {
                println!("{:?} file not found {:?}", ch.diff_type(), name_hash);
                continue;
            };
            let recovered_path = bungee.path_of("/", *index);
            println!("Path for {:?} file: {}", ch.diff_type(), recovered_path);
        }

        let mem = h1.memory_usage() + h2.memory_usage() + bungee.memory_usage() + files.len() * size_of::<(BungeeIndex, HashArray<32>)>();
        println!("Approx mem: {:.03}Mb", mem as f64 / (1024.0 * 1024.0));
    }
}
