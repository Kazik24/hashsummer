mod bungee;

pub use bungee::*;

pub trait MeasureMemory {
    fn memory_usage(&self) -> usize;
}
