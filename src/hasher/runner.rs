use crossbeam::channel::{bounded, Receiver, Sender};
use std::any::Any;
use std::fs::File;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{spawn, JoinHandle, Thread};
use std::{
    io,
    io::{ErrorKind, Read},
    ops::Deref,
    path::PathBuf,
    sync::Arc,
};

use crate::hasher::{Consumer, DataChunk, HashArray, HashEntry};
use crossbeam::queue::ArrayQueue;
use digest::{Digest, FixedOutputReset};
use generic_array::GenericArray;
use parking_lot::{Condvar, Mutex};
use rayon::{ThreadPool, ThreadPoolBuilder};

pub struct HashRunner {
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
    data_chunks_supply: Receiver<ChunkData>,
    data_chunks_recycle: Sender<ChunkData>,
    permits: Arc<Permits>,
    max_permits: usize,
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

impl HashRunner {
    pub fn run<I, C: Consumer + Send + Sync + 'static>(files: I, consume: Arc<C>, permits: usize) -> Self
    where
        I: Iterator<Item = PathBuf> + Send + 'static,
    {
        let buffers = 1024;
        let (ctx, crx) = bounded(buffers);
        for _ in 0..buffers {
            ctx.try_send(ChunkData::zero()).unwrap();
        }

        let c = Arc::new(InnerConfig {
            worker_pool: ThreadPoolBuilder::new()
                .num_threads(16)
                .panic_handler(pool_panic_handler)
                .build()
                .unwrap(),
            reader_pool: ThreadPoolBuilder::new()
                .num_threads(16)
                .panic_handler(pool_panic_handler)
                .build()
                .unwrap(),
            chunk_size: 1024 * 256,
            chan_bound: 32,
            flag: AtomicBool::new(true),
            permits: Arc::new(Permits::new(permits)),
            max_permits: permits,
            data_chunks_supply: crx,
            data_chunks_recycle: ctx,
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
            let Some(file) = cfg.iter.next() else { break; };
            let permit = cfg.c.permits.clone();
            permit.wait_for_permit();

            let (tx, rx) = bounded::<ChunkData>(cfg.c.chan_bound);
            let supply = cfg.c.data_chunks_supply.clone();
            let size = cfg.c.chunk_size;
            let file2 = file.clone();
            cfg.c.reader_pool.spawn_fifo(move || {
                let res = Self::read_file(file, supply, tx, size);
                if let Err(res) = res {
                    //todo save file errors in some list
                    println!("Error reading file {res}");
                }
            });
            let consumer = cfg.consumer.clone();
            let recycle = cfg.c.data_chunks_recycle.clone();
            cfg.c.worker_pool.spawn_fifo(move || {
                Self::process_file(file2, rx, recycle, &*consumer, permit);
            });
        }
        //wait for all permits to finish
        cfg.c.permits.wait_for_permits(cfg.c.max_permits);
    }

    fn read_file(path: PathBuf, supply: Receiver<ChunkData>, dout: Sender<ChunkData>, chunk_size: usize) -> io::Result<()> {
        let mut file = File::open(path)?;
        loop {
            let mut chunk = supply.recv().unwrap(); //cant disconnect ever
            if chunk.capacity() < chunk_size {
                chunk = ChunkData::new(chunk_size)
            }
            let end = !chunk.read_from(&mut file)?; //todo error handling
            dout.send(chunk).unwrap(); //cant disconnect first
            if end {
                return Ok(());
            }
        }
    }

    fn process_file<C>(path: PathBuf, din: Receiver<ChunkData>, recycle: Sender<ChunkData>, consumer: &C, signal: Arc<Permits>)
    where
        C: Consumer,
    {
        let name = consumer.consume_name(&path);
        let mut hasher = consumer.start_file();

        while let Ok(chunk) = din.recv() {
            consumer.update_file(&mut hasher, &chunk);
            recycle.send(chunk).unwrap();
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

    /// Read as much bytes as possible to fill this chunk, discards old content
    pub fn read_from<R: Read>(&mut self, reader: &mut R) -> Result<bool, std::io::Error> {
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
    use std::thread::{scope, sleep};
    use std::time::Duration;

    #[test]
    fn test_permits() {
        let perm = Permits::new(3);

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
