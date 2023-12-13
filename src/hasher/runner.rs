use crossbeam::channel::{bounded, Receiver, Sender};
use std::any::Any;
use std::fs::File;
use std::iter::repeat_with;
use std::mem::size_of_val;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{available_parallelism, spawn, JoinHandle, Thread};
use std::{
    io,
    io::{ErrorKind, Read},
    ops::Deref,
    path::PathBuf,
    sync::Arc,
};

use crate::hasher::{Consumer, DataChunk, HashArray, HashEntry};
use crate::utils::{AveragePerTick, LendingStack, MeasureMemory};
use crossbeam::queue::ArrayQueue;
use digest::{Digest, FixedOutputReset};
use generic_array::GenericArray;
use parking_lot::{Condvar, Mutex};
use rayon::{ThreadPool, ThreadPoolBuilder};

pub struct ScanRunner {
    //buffer for reusing allocations
    scheduler: JoinHandle<()>,
    config: Arc<InnerConfig>,
}

struct Config<I: Iterator<Item = PathBuf>, F> {
    c: Arc<InnerConfig>,
    iter: I,
    consumer: Arc<F>,
}

struct InnerConfig {
    flag: AtomicBool,
    reader_pool: ThreadPool,
    worker_pool: ThreadPool,
    data_chunks: LendingStack<ChunkData>,
    permits: Arc<Permits>,
    max_permits: usize,
    read_bytes: Arc<AveragePerTick>,
    chan_bound: usize,
    chunk_size: usize, //init chunk size
}

fn pool_panic_handler(payload: Box<dyn Any + Send>) {
    println!("Rayon thread panicked \"{}\"", format_panic_msg(&payload));
}

fn format_panic_msg(payload: &Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast_ref::<&'static str>() {
        Some(msg) => msg.to_string(),
        None => match payload.downcast_ref::<String>() {
            Some(msg) => msg.to_string(),
            // Copy what rustc does in the default panic handler
            None => format!("{payload:?}"),
        },
    }
}

pub struct RunnerConfig {
    pub permits: usize,
    pub read_bytes_stats: Option<Arc<AveragePerTick>>,
    pub drive_type: DriveType,
    pub buffer_chunk_size: usize,
    pub max_buffer_chunks: usize,
    pub max_buffer_chunks_per_file: usize,
}

// todo, checking at runtime if file is on hdd or ssd
// link: https://devblogs.microsoft.com/oldnewthing/20201023-00/?p=104395
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DriveType {
    Ssd,
    Hdd,
    /// Warning, when specifying custom thread number, there should be at least same number of
    /// processing_threads as read_threads. If number of processing_threads is lower, in current
    /// implementation the runner tasks might starve, and halt if there is not enough buffers to use by all
    /// tasks.
    Custom {
        read_threads: usize,
        processing_threads: usize,
    },
}

impl RunnerConfig {
    pub const fn new(permits: usize, read_bytes_stats: Option<Arc<AveragePerTick>>) -> Self {
        Self {
            permits,
            read_bytes_stats,
            drive_type: DriveType::Ssd,
            buffer_chunk_size: 1024 * 256,
            max_buffer_chunks: 1024,
            max_buffer_chunks_per_file: 32,
        }
    }
    pub fn hdd(mut self) -> Self {
        self.drive_type = DriveType::Hdd;
        self
    }
}

impl ScanRunner {
    pub fn run<I, C: Consumer + Send + Sync + 'static>(files: I, consume: Arc<C>, cfg: RunnerConfig) -> Self
    where
        I: Iterator<Item = PathBuf> + Send + 'static,
    {
        let cpus = available_parallelism().map(|v| v.get()).unwrap_or(1);
        let (read_threads, hash_threads) = match cfg.drive_type {
            DriveType::Hdd => (1, cpus.min(4)), //parallel reads are not good for HDDs
            DriveType::Ssd => (cpus, cpus),     //read as much in parallel as reasonably available
            DriveType::Custom {
                read_threads,
                processing_threads,
            } => (read_threads.max(1), processing_threads.max(1)),
        };

        if read_threads > hash_threads {
            println!("Warning, configuration might halt the runner");
        }

        let c = Arc::new(InnerConfig {
            reader_pool: ThreadPoolBuilder::new()
                .num_threads(read_threads)
                .thread_name(|i| format!("reader-{i}"))
                .panic_handler(pool_panic_handler)
                .build()
                .unwrap(),
            worker_pool: ThreadPoolBuilder::new()
                .num_threads(hash_threads)
                .thread_name(|i| format!("worker-{i}"))
                .panic_handler(pool_panic_handler)
                .build()
                .unwrap(),
            chunk_size: cfg.buffer_chunk_size.max(16),
            chan_bound: cfg.max_buffer_chunks_per_file,
            flag: AtomicBool::new(true),
            read_bytes: cfg.read_bytes_stats.unwrap_or_default(),
            permits: Arc::new(Permits::new(cfg.permits)),
            max_permits: cfg.permits,
            data_chunks: LendingStack::new(repeat_with(|| ChunkData::zero()).take(cfg.max_buffer_chunks.max(1)).collect()),
        });

        let cfg = Config {
            c: c.clone(),
            iter: files,
            consumer: consume,
        };

        let handle = spawn(move || {
            Self::scheduler_run(cfg);
        });

        Self {
            config: c,
            scheduler: handle,
        }
    }

    pub fn signal_stop(&self) -> bool {
        self.config.flag.swap(false, Ordering::AcqRel)
    }
    pub fn is_finished(&self) -> bool {
        self.scheduler.is_finished()
    }
    pub fn stop(self) -> bool {
        if self.is_finished() {
            return false;
        }
        self.config.flag.store(false, Ordering::Release);
        self.scheduler.join();
        true
    }

    pub fn wait_for_finish(self) {
        self.scheduler.join();
    }

    fn scheduler_run<I, C>(mut cfg: Config<I, C>)
    where
        I: Iterator<Item = PathBuf>,
        C: Consumer + Send + Sync + 'static,
    {
        while cfg.c.flag.load(Ordering::Relaxed) {
            let Some(file) = cfg.iter.next() else {
                break;
            };
            let permit = cfg.c.permits.clone();
            permit.wait_for_permit();

            let (tx, rx) = bounded::<ChunkData>(cfg.c.chan_bound);
            let supply = cfg.c.data_chunks.clone();
            let size = cfg.c.chunk_size;
            let stat = cfg.c.read_bytes.clone();
            let file2 = file.clone();
            let consumer = cfg.consumer.clone();
            cfg.c.reader_pool.spawn_fifo(move || {
                let res = Self::read_file(&file, supply, tx, size, stat);
                if let Err(err) = res {
                    consumer.on_error(err, &file);
                }
            });
            let consumer = cfg.consumer.clone();
            let recycle = cfg.c.data_chunks.clone();
            cfg.c.worker_pool.spawn_fifo(move || {
                Self::process_file(file2, rx, recycle, &*consumer, permit);
            });
        }
        //wait for all permits to finish
        cfg.c.permits.wait_for_permits(cfg.c.max_permits);
    }

    fn read_file(
        path: &Path,
        supply: LendingStack<ChunkData>,
        dout: Sender<ChunkData>,
        chunk_size: usize,
        stats: Arc<AveragePerTick>,
    ) -> io::Result<()> {
        let mut file = File::open(path)?;
        loop {
            let mut chunk = supply.lend();
            if chunk.capacity() < chunk_size {
                chunk = ChunkData::new(chunk_size)
            }
            let should_continue = chunk.read_from(&mut file);
            //don't loose chunk if error occurs
            stats.append(chunk.len() as _);
            dout.send(chunk).unwrap(); //cant disconnect first
            match should_continue {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(err) => return Err(err),
            }
        }
    }
    fn process_file<C>(path: PathBuf, din: Receiver<ChunkData>, recycle: LendingStack<ChunkData>, consumer: &C, signal: Arc<Permits>)
    where
        C: Consumer,
    {
        let name = consumer.consume_name(&path);
        let mut hasher = consumer.start_file();

        while let Ok(chunk) = din.recv() {
            consumer.update_file(&mut hasher, &chunk);
            recycle.give_back(chunk);
        }
        consumer.finish_consume(name, hasher);
        signal.add_permit();
    }
}

pub struct ChunkData {
    array: Box<[u8]>,
    length: usize,
}

impl ChunkData {
    pub fn zero() -> Self {
        Self {
            array: Default::default(),
            length: 0,
        }
    }
    pub fn new(size: usize) -> Self {
        Self {
            array: vec![0u8; size].into_boxed_slice(),
            length: 0,
        }
    }

    /// Read as much bytes as possible to fill this chunk, discards old content, return true if chunk was read fully.
    pub fn read_from<R: Read>(&mut self, reader: &mut R) -> Result<bool, io::Error> {
        self.length = 0;
        let mut buf = &mut *self.array;
        while !buf.is_empty() {
            match reader.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                    self.length += n;
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(self.length == self.array.len())
    }
    pub fn capacity(&self) -> usize {
        self.array.len()
    }
    pub fn len(&self) -> usize {
        self.length
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

impl Deref for ChunkData {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.array[..self.length]
    }
}

impl MeasureMemory for ChunkData {
    fn memory_usage(&self) -> usize {
        size_of_val(&self.array[..])
    }
}

pub struct Permits {
    mutex: Mutex<usize>,
    cond: Condvar,
}

impl Permits {
    pub const fn new(permits: usize) -> Self {
        Self {
            mutex: Mutex::new(permits),
            cond: Condvar::new(),
        }
    }

    pub fn has_permits(&self) -> bool {
        self.permits_count() != 0
    }
    pub fn permits_count(&self) -> usize {
        *self.mutex.lock()
    }

    pub fn wait_for_permit(&self) {
        self.wait_for_permits(1);
    }
    pub fn wait_for_permits(&self, count: usize) {
        self.cond.wait_while(&mut self.mutex.lock(), |perm| {
            if let Some(rem) = perm.checked_sub(count) {
                *perm = rem;
                return false;
            }
            true
        });
    }

    pub fn add_permit(&self) {
        self.add_permits(1);
    }
    pub fn add_permits(&self, count: usize) {
        let mut lock = self.mutex.lock();
        if let Some(value) = lock.checked_add(count) {
            *lock = value;
            self.cond.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::hasher::runner::Permits;
    use std::sync::Arc;
    use std::thread::{available_parallelism, scope, sleep};
    use std::time::Duration;

    #[test]
    fn test_permits() {
        let perm = Permits::new(3);
        println!("{:?}", available_parallelism());

        scope(|c| {
            perm.wait_for_permit();
            perm.wait_for_permit();

            c.spawn(|| {
                sleep(Duration::from_millis(100));
                perm.add_permit();
                sleep(Duration::from_millis(100));
                perm.add_permit();
                sleep(Duration::from_millis(100));
                perm.add_permit();
                sleep(Duration::from_millis(100));
            });

            perm.wait_for_permit();
            perm.wait_for_permit();
            perm.wait_for_permit();
            perm.wait_for_permit();
        })
    }
}
