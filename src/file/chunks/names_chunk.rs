use crate::utils::{BungeeIndex, BungeeStr, MeasureMemory};
use std::mem::size_of;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct NamesChunk {
    bungee: BungeeStr,
    indexes: Vec<BungeeIndex>,
}

pub struct InfoChunk {}

impl NamesChunk {
    pub fn new(bungee: BungeeStr, indexes: Vec<BungeeIndex>) -> Self {
        Self { bungee, indexes }
    }
}

impl MeasureMemory for NamesChunk {
    fn memory_usage(&self) -> usize {
        (self.indexes.capacity() * size_of::<BungeeIndex>()) + self.bungee.memory_usage()
    }
}
