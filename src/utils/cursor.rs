use std::collections::VecDeque;

/// trait reducing overhead of virtual iterators
pub trait ReadCursor<T> {
    fn next_items(&mut self, consumer: &mut Consumer<T>) -> bool;
}

pub struct Consumer<T> {
    items: VecDeque<T>,
    max_count: usize,
}

impl<T> Consumer<T> {
    pub fn with_size(size: usize) -> Self {
        Self {
            items: VecDeque::new(),
            max_count: size,
        }
    }

    pub fn max_size(&self) -> usize {
        self.max_count
    }
    pub fn push(&mut self, item: T) {
        self.items.push_back(item);
    }

    pub fn remaining(&self) -> usize {
        self.max_count - self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn pop_first(&mut self) -> Option<T> {
        self.items.pop_front()
    }
}

pub struct IterWrapper<T> {
    iter: T,
}

pub struct ReadCursorIter<T, C> {
    cursor: C,
    consumer: Consumer<T>,
    has_more: bool,
}

impl<T, C> ReadCursorIter<T, C>
where
    C: ReadCursor<T>,
{
    pub fn with_buffer(cursor: C, size: usize) -> Self {
        Self {
            cursor,
            consumer: Consumer::with_size(size),
            has_more: true,
        }
    }
}
impl<I: Iterator> ReadCursor<I::Item> for IterWrapper<I> {
    fn next_items(&mut self, consumer: &mut Consumer<I::Item>) -> bool {
        let mut count = consumer.remaining();
        for _ in 0..count {
            match self.iter.next() {
                Some(v) => consumer.push(v),
                None => return true,
            }
        }
        false
    }
}

impl<T, C> Iterator for ReadCursorIter<T, C>
where
    C: ReadCursor<T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(v) = self.consumer.pop_first() {
                return Some(v);
            }
            if !self.has_more {
                return None;
            }
            self.has_more = self.cursor.next_items(&mut self.consumer);
        }
    }
}
