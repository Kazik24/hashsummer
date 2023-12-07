mod bungee;
mod cursor;
mod sort;

pub use bungee::*;
pub use sort::*;
use std::cmp::min;
use std::iter::repeat_with;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub trait MeasureMemory {
    fn memory_usage(&self) -> usize;
}

/// Struct for averaging a number over a perioid of time with moving average.
/// Eg. appending number of bytes read, and ticking with one second interval will result in
/// average of bytes read per second
#[derive(Default)]
pub struct AveragePerTick {
    current: AtomicU64,
    ticks: Box<[AtomicU64]>,
    index: AtomicUsize,
}

impl AveragePerTick {
    pub fn new(window: usize) -> Self {
        Self {
            current: AtomicU64::new(0),
            ticks: repeat_with(|| AtomicU64::new(0))
                .take(window)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            index: AtomicUsize::new(0),
        }
    }

    pub fn append(&self, value: u64) {
        self.current.fetch_add(value, Ordering::Relaxed);
    }

    pub fn get_avg(&self) -> u64 {
        let index = self.index.load(Ordering::Acquire);
        let slice = &self.ticks[..min(self.ticks.len(), index)];
        if slice.is_empty() {
            return 0;
        }
        let mut sum = 0;
        for v in slice.iter().map(|v| v.load(Ordering::Relaxed)) {
            sum += v as u128;
        }
        let avg = sum / slice.len() as u128;
        avg as u64
    }

    pub fn tick(&self) {
        let collected = self.current.swap(0, Ordering::Relaxed);
        let idx = self.index.fetch_add(1, Ordering::Relaxed);
        self.ticks[idx % self.ticks.len()].store(collected, Ordering::Relaxed);
    }

    pub fn tick_and_get_avg(&self) -> u64 {
        self.tick();
        self.get_avg()
    }

    pub fn reset(&self) {
        self.current.store(0, Ordering::Relaxed);
        self.index.store(0, Ordering::Relaxed);
        for v in self.ticks.iter() {
            v.store(0, Ordering::Relaxed);
        }
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
                c.append(1);
            }

            while start.elapsed() < Duration::from_secs(15) {
                c.append(1);
                let s = Instant::now();
                while s.elapsed() <= Duration::from_millis(1) {
                    yield_now()
                }
            }
        });

        for i in 0..200 {
            calc.tick();
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
        assert_eq!(calc.tick_and_get_avg(), 10);
        assert_eq!(calc.tick_and_get_avg(), 5);
        assert_eq!(calc.tick_and_get_avg(), 3);
        assert_eq!(calc.tick_and_get_avg(), 2);
        for _ in 0..6 {
            calc.tick();
        }
        assert_eq!(calc.get_avg(), 1);
        assert_eq!(calc.tick_and_get_avg(), 0);
        calc.append(100);
        for _ in 0..10 {
            assert_eq!(calc.tick_and_get_avg(), 10);
        }
        assert_eq!(calc.tick_and_get_avg(), 0);
    }
}
