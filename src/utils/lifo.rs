use crate::utils::MeasureMemory;
use parking_lot::{Condvar, Mutex};
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::sync::Arc;

pub struct LendingStack<T> {
    inner: Arc<InnerLifo<T>>,
}

impl<T> LendingStack<T> {
    pub fn new(mut elements: Vec<T>) -> Self {
        elements.shrink_to_fit();
        Self {
            inner: Arc::new(InnerLifo {
                max: elements.len(),
                elements: Mutex::new(elements),
                condvar: Condvar::new(),
            }),
        }
    }

    pub fn try_lend(&self) -> Option<T> {
        self.inner.elements.lock().pop()
    }

    pub fn lend(&self) -> T {
        self.inner.lend()
    }
    pub fn give_back(&self, value: T) -> Option<T> {
        self.inner.give_back(value)
    }
}

impl<T> Clone for LendingStack<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: MeasureMemory> MeasureMemory for LendingStack<T> {
    fn memory_usage(&self) -> usize {
        let m = self.inner.elements.lock();
        m.iter().map(|v| v.memory_usage()).sum::<usize>() + (m.capacity() * size_of::<T>())
    }
}

struct InnerLifo<T> {
    elements: Mutex<Vec<T>>,
    condvar: Condvar,
    max: usize,
}

impl<T> InnerLifo<T> {
    pub fn lend(&self) -> T {
        let mut lock = self.elements.lock();
        loop {
            if let Some(v) = lock.pop() {
                return v;
            }
            self.condvar.wait(&mut lock);
        }
    }

    /// Return lent value to this collection, if collection is full, the value will be returned as option.
    pub fn give_back(&self, value: T) -> Option<T> {
        let mut lock = self.elements.lock();
        if lock.len() >= self.max {
            return Some(value);
        }
        lock.push(value);
        drop(lock);
        self.condvar.notify_one(); //wake just one thread, cause we've given back only one element.
        None
    }
}
