use crate::file::AnyBlock::Names;
use crate::file::HashesChunk;
use crate::file::NamesChunk;
use crate::utils::{BungeeIndex, BungeeStr};
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
            .filter(|v| v.2.is_file())
            .map(move |(i, d, _)| {
                if let Some(i) = i {
                    pi.push(i);
                }
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
    let runner = ScanRunner::run(paths.into_iter(), cons, cfg);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_snapshot() {
        let path = Path::new(".");

        snapshot_files(path);
    }
}
