mod bungee;
mod cursor;
mod io;
mod lifo;
mod size;
mod sort;

pub use bungee::*;
pub use io::*;
pub use lifo::*;
use parking_lot::RwLock;
pub use size::*;
pub use sort::*;
use std::cmp::min;
use std::iter::repeat_with;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub trait MeasureMemory {
    fn memory_usage(&self) -> usize;

    fn total_memory_usage(&self) -> usize
    where
        Self: Sized,
    {
        self.memory_usage() + size_of::<Self>()
    }
}

/// Struct for averaging a number over a period of time with moving average.
/// Eg. appending number of bytes read, and ticking with one second interval will result in
/// average of bytes read per second
#[derive(Default)]
pub struct AveragePerTick {
    current: AtomicU64,
    ticks: RwLock<MovingAvg>,
}

#[derive(Default)]
struct MovingAvg {
    array: Vec<u64>,
    index: usize,
}

impl MovingAvg {
    fn get_avg(&self) -> u64 {
        let slice = &self.array[..min(self.array.len(), self.index)];
        if slice.is_empty() {
            return 0;
        }
        let sum = slice.iter().fold(0, |acc, v| acc + *v as u128);
        let avg = sum / slice.len() as u128;
        avg as u64
    }

    fn append_sample(&mut self, collected: u64) {
        if self.array.is_empty() {
            return;
        }
        let size = self.array.len();
        let size2 = size * 2 - 1;
        let idx = self.index;
        if idx >= size2 {
            self.index = size;
        } else {
            self.index = idx + 1;
        }
        self.array[idx % size] = collected;
    }
}

impl AveragePerTick {
    pub fn new(window: usize) -> Self {
        assert!(window > 0);
        Self {
            current: AtomicU64::new(0),
            ticks: RwLock::new(MovingAvg {
                index: 0,
                array: vec![0; window],
            }),
        }
    }

    pub const fn empty() -> Self {
        Self {
            current: AtomicU64::new(0),
            ticks: RwLock::new(MovingAvg {
                index: 0,
                array: Vec::new(),
            }),
        }
    }

    pub fn reset_window_size(&self, window: usize) {
        let mut lock = self.ticks.write();
        lock.index = 0;
        lock.array = vec![0; window];
    }

    #[inline]
    pub fn append_one(&self) {
        self.append(1);
    }
    #[inline]
    pub fn append(&self, value: u64) {
        self.current.fetch_add(value, Ordering::Relaxed);
    }

    /// Get average total appended value per tick,
    /// If you want to get average value per second and tick rate is 10 ticks/sec, you can multiply
    /// result of this function by 10 to get average per second, and this average will have refresh
    /// rate of 10 times/sec
    pub fn get_avg(&self) -> u64 {
        self.ticks.read().get_avg()
    }

    pub fn sample_now(&self) {
        let collected = self.current.swap(0, Ordering::Relaxed);
        self.ticks.write().append_sample(collected);
    }

    pub fn sample_and_get_avg(&self) -> u64 {
        let collected = self.current.swap(0, Ordering::Relaxed);
        let mut lock = self.ticks.write();
        lock.append_sample(collected);
        lock.get_avg()
    }

    pub fn reset(&self) {
        self.current.store(0, Ordering::Relaxed);
        let mut lock = self.ticks.write();
        lock.index = 0;
        lock.array.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread::{sleep, spawn, yield_now};
    use std::time::{Duration, Instant};

    #[test]
    fn test_average_per_tick() {
        let calc = Arc::new(AveragePerTick::new(10));

        let c = calc.clone();
        spawn(move || {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                c.append_one();
            }

            while start.elapsed() < Duration::from_secs(15) {
                c.append_one();
                let s = Instant::now();
                while s.elapsed() <= Duration::from_millis(1) {
                    yield_now()
                }
            }
        });

        for i in 0..200 {
            calc.sample_now();
            let val = calc.get_avg();
            println!("tick {i:>3}, value: {val}");
            sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn test_deterministic_average() {
        let calc = AveragePerTick::new(10);
        calc.append(10);
        assert_eq!(calc.get_avg(), 0);
        assert_eq!(calc.sample_and_get_avg(), 10);
        assert_eq!(calc.sample_and_get_avg(), 5);
        assert_eq!(calc.sample_and_get_avg(), 3);
        assert_eq!(calc.sample_and_get_avg(), 2);
        for _ in 0..6 {
            calc.sample_now();
        }
        assert_eq!(calc.get_avg(), 1);
        assert_eq!(calc.sample_and_get_avg(), 0);
        calc.append(100);
        for _ in 0..10 {
            assert_eq!(calc.sample_and_get_avg(), 10);
        }
        assert_eq!(calc.sample_and_get_avg(), 0);
    }
}
