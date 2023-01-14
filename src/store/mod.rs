mod mem;
mod str_convert;

pub use mem::*;
pub use str_convert::*;

use crate::store::DiffResult::Removed;
use crate::store::DiffType::Added;
use crate::{HashArray, HashEntry};
use itertools::*;
pub use mem::*;
use std::cmp::Ordering;
use std::iter::once;
use std::mem::replace;

pub trait HashStore {
    type OwnIter<'a>: DoubleEndedIterator<Item = HashEntry<32, 32>>
    where
        Self: 'a;

    type RefIter<'a>: DoubleEndedIterator<Item = &'a HashEntry<32, 32>>
    where
        Self: 'a;

    fn sorted_ref_iter(&self) -> Self::RefIter<'_>;

    fn sorted_iter(&self) -> Self::OwnIter<'_>;

    fn is_owned_only(&self) -> bool;
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DiffResult<E> {
    Added(E),
    Removed(E),
    Changed(E, E),
    Same(E),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DiffType {
    Added,
    Removed,
    Changed,
    Same,
}

impl<E> DiffResult<E> {
    pub fn diff_type(&self) -> DiffType {
        match self {
            Self::Added(_) => DiffType::Added,
            Self::Removed(_) => DiffType::Removed,
            Self::Changed(_, _) => DiffType::Changed,
            Self::Same(_) => DiffType::Same,
        }
    }
}

pub trait NamedValue {
    type Name: Ord;
    type Value: PartialEq;

    fn get_name(&self) -> &Self::Name;
    fn get_value(&self) -> &Self::Value;
}

impl<const A: usize, const B: usize> NamedValue for HashEntry<A, B> {
    type Name = HashArray<A>;
    type Value = HashArray<B>;

    fn get_name(&self) -> &Self::Name {
        &self.id
    }

    fn get_value(&self) -> &Self::Value {
        &self.data
    }
}

impl<T: NamedValue> NamedValue for &'_ T {
    type Name = T::Name;
    type Value = T::Value;

    fn get_name(&self) -> &Self::Name {
        T::get_name(self)
    }
    fn get_value(&self) -> &Self::Value {
        T::get_value(self)
    }
}

pub struct DiffingIter<O, N>
where
    O: Iterator,
    N: Iterator<Item = O::Item>,
    O::Item: NamedValue,
{
    old: O,
    new: N,
    curr_old: Option<O::Item>,
    curr_new: Option<N::Item>,
}

impl<O, N> DiffingIter<O, N>
where
    O: Iterator,
    N: Iterator<Item = O::Item>,
    O::Item: NamedValue,
{
    pub fn new(mut old: O, mut new: N) -> Self {
        Self {
            curr_old: old.next(),
            curr_new: new.next(),
            old,
            new,
        }
    }
}

impl<O, N> Iterator for DiffingIter<O, N>
where
    O: Iterator,
    N: Iterator<Item = O::Item>,
    O::Item: NamedValue,
{
    type Item = DiffResult<O::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        use DiffResult::*;
        let (o, n) = match (&self.curr_old, &self.curr_new) {
            (None, Some(_)) => {
                return replace(&mut self.curr_new, self.new.next()).map(Added);
            }
            (Some(_), None) => {
                return replace(&mut self.curr_old, self.old.next()).map(Removed);
            }
            (None, None) => return None,
            (Some(o), Some(n)) => (o, n),
        };
        //compare entries
        match o.get_name().cmp(n.get_name()) {
            Ordering::Equal => {
                if o.get_value() == n.get_value() {
                    self.curr_new = self.new.next();
                    replace(&mut self.curr_old, self.old.next()).map(Same)
                } else {
                    let o = replace(&mut self.curr_old, self.old.next());
                    let n = replace(&mut self.curr_new, self.new.next());
                    Some(Changed(o.unwrap(), n.unwrap()))
                }
            }
            Ordering::Greater => {
                //old is greater than new, advance new and return values as Added()
                replace(&mut self.curr_new, self.new.next()).map(Added)
            }
            Ordering::Less => {
                //old is lower than new, advance old and return values as Removed()
                replace(&mut self.curr_old, self.old.next()).map(Removed)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (a_lower, a_upper) = self.old.size_hint();
        let (b_lower, b_upper) = self.new.size_hint();

        let lower = a_lower.max(b_lower);

        let upper = match (a_upper, b_upper) {
            (Some(x), Some(y)) => Some(x.max(y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (None, None) => None,
        };

        (lower, upper)
    }
}

impl<O, N> ExactSizeIterator for DiffingIter<O, N>
where
    O: ExactSizeIterator,
    N: ExactSizeIterator<Item = O::Item>,
    O::Item: NamedValue,
{
}

#[cfg(test)]
mod tests {
    use crate::store::{DiffResult, DiffingIter};
    use crate::{HashArray, HashEntry};

    fn mock_entry(id: &str, data: &str) -> HashEntry<32, 32> {
        HashEntry {
            id: HashArray::parse_fill_zero(id),
            data: HashArray::parse_fill_zero(data),
        }
    }

    #[test]
    fn test_diff_same() {
        let arr = &[
            mock_entry("01", "11"),
            mock_entry("02", "12"),
            mock_entry("03", "13"),
            mock_entry("04", "14"),
            mock_entry("05", "15"),
        ];

        let v = DiffingIter::new(arr.iter(), arr.iter()).collect::<Vec<_>>();

        assert!(v.iter().zip(arr).all(|(a, b)| a == &DiffResult::Same(b)));
    }

    #[test]
    fn test_diff() {
        let a = &[
            mock_entry("01", "11"),
            mock_entry("02", "12"),
            mock_entry("03", "13"),
            mock_entry("04", "14"),
            mock_entry("05", "15"),
        ];

        let b = &[
            mock_entry("01", "11"),
            mock_entry("02", "12"),
            mock_entry("03", "13"),
            mock_entry("04", "14"),
            mock_entry("05", "15"),
        ];

        let v = DiffingIter::new(a.iter(), b.iter()).collect::<Vec<_>>();

        //assert!(v.iter().zip(arr).all(|(a, b)| a == &DiffResult::Same(b)));
    }
}
