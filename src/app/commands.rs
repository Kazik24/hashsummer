use crate::file::hashes_chunk::HashesChunk;
use crate::{DepthFileScanner, DigestConsumer, HashEntry, HashRunner, RunnerConfig};
use parking_lot::Mutex;
use sha2::Sha256;
use std::mem::{replace, size_of_val};
use std::path::Path;
use std::sync::Arc;

pub fn snapshot_files(path: &Path) {
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
    let cfg = RunnerConfig::new(128, None);
    let runner = HashRunner::run(paths.into_iter(), cons.clone(), cfg);
    runner.wait_for_finish();

    let mut vals = replace(&mut *mutex.lock(), Vec::new());
    drop(mutex);
    let mut vals = HashesChunk::new_sha256(vals, false);

    let bytes = size_of_val(vals.data.as_slice()) as f64 / (1024.0 * 1024.0);
    println!("Memory taken: {bytes:.3}Mb");

    vals.sort();
    println!("first: {:?}", vals.data.first().unwrap());
    println!("last:  {:?}", vals.data.last().unwrap());
}
