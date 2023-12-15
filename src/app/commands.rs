use crate::file::chunks::{HashesChunk, NamesChunk};
use crate::utils::{BungeeIndex, BungeeStr, ByteSize};
use crate::{DepthFileScanner, DigestConsumer, HashEntry, RunnerConfig, ScanRunner};
use parking_lot::Mutex;
use sha2::Sha256;
use std::mem::{replace, size_of_val};
use std::path::Path;
use std::sync::Arc;

pub fn snapshot_files(path: &Path) {
    let path_buffer = Arc::new(Mutex::new(BungeeStr::new()));
    let path_indexes = Arc::new(Mutex::new(Vec::new()));
    let paths = {
        let mut pb = path_buffer.lock_arc();
        let mut pi = path_indexes.lock_arc();
        DepthFileScanner::from_dir(path)
            .save_to_bungee(move |a, b| pb.push(a, b), |v, t| Some(v.to_string_lossy()))
            .into_iter()
            .filter_map(|(i, d, t)| Some((t.is_file().then_some(i).flatten()?, d, i)))
            .map(move |(i, d, _)| {
                pi.push(i);
                d.path()
            })
    };

    let mutex: Arc<Mutex<Vec<HashEntry<32, 32>>>> = Default::default();
    let cons = {
        let mutex = mutex.clone();
        Arc::new(DigestConsumer::<32, 32, Sha256, _>::new(move |value| mutex.lock().push(value)))
        // Arc::new(HashZeroChunksFinder {
        //     min_size: 16000,
        //     chunks: Default::default(),
        // })
    };
    let cfg = RunnerConfig::new(128, None);
    let runner = ScanRunner::run(paths, cons, cfg);
    runner.wait_for_finish();

    let vals = Arc::into_inner(mutex).expect("More than one mutex reference").into_inner();
    let idx = Arc::into_inner(path_indexes).expect("More than one mutex reference").into_inner();
    let paths = Arc::into_inner(path_buffer).expect("More than one mutex reference").into_inner();

    let mut hashes = HashesChunk::new_sha256(vals, false);
    hashes.sort();
    let restored = idx.iter().map(|&i| paths.path_of("/", i)).collect::<Vec<_>>();
    //println!("restored [{}]{restored:#?}", restored.len());
    let mut names = NamesChunk::new(paths, idx);

    let bytes = size_of_val(hashes.data.as_slice()) as f64 / (1024.0 * 1024.0);
    println!("Memory taken: {bytes:.3}Mb count:{}", hashes.data.len());

    println!("first: {:?}", hashes.data.first().unwrap());
    println!("last:  {:?}", hashes.data.last().unwrap());
}

#[derive(Default, Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FileCounts {
    pub dirs: u64,
    pub files: u64,
    pub symlinks: u64,
    pub total_size: ByteSize,
    pub empty_files: u64,
    pub errors: u64,
}

impl FileCounts {
    pub fn count_all_in(path: &Path) -> Self {
        let mut files = FileCounts::default();
        for (entry, typ) in DepthFileScanner::from_dir(path).into_iter() {
            if typ.is_file() {
                files.files += 1;
                match entry.metadata() {
                    Ok(meta) if meta.len() == 0 => files.empty_files += 1,
                    Ok(meta) => files.total_size += meta.len(),
                    Err(_err) => files.errors += 1,
                }
            } else if typ.is_dir() {
                files.dirs += 1;
            } else if typ.is_symlink() {
                files.symlinks += 1;
            }
        }
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_snapshot() {
        let path = Path::new(".");

        snapshot_files(path);
    }

    #[test]
    #[ignore]
    fn test_count_all() {
        let path = Path::new(".");
        let counts = FileCounts::count_all_in(path);
        println!("Counts: {counts:#?}");
    }
}
