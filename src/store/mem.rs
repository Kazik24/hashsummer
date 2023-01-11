use crate::store::{DiffingIter, HashStore};
use crate::{HashArray, HashEntry};
use std::iter::Copied;
use std::slice::Iter;

type Entry = HashEntry<32, 32>;

pub struct MemHashStore {
    entries: Vec<Entry>,
    sorted_by_id: bool, //true - by id, false - by data
}

impl HashStore for MemHashStore {
    type OwnIter<'a> = Copied<Iter<'a, Entry>>;
    type RefIter<'a> = Iter<'a, Entry>;

    fn sorted_ref_iter(&self) -> Self::RefIter<'_> {
        self.entries.iter()
    }
    fn sorted_iter(&self) -> Self::OwnIter<'_> {
        self.sorted_ref_iter().copied()
    }

    fn is_owned_only(&self) -> bool {
        false
    }
}

impl FromIterator<Entry> for MemHashStore {
    fn from_iter<T: IntoIterator<Item = Entry>>(iter: T) -> Self {
        let mut v = iter.into_iter().collect::<Vec<_>>();
        v.sort_unstable();
        Self {
            entries: v,
            sorted_by_id: true,
        }
    }
}

impl<'a> FromIterator<&'a Entry> for MemHashStore {
    fn from_iter<T: IntoIterator<Item = &'a Entry>>(iter: T) -> Self {
        let mut v = iter.into_iter().copied().collect::<Vec<_>>();
        v.sort_unstable();
        Self {
            entries: v,
            sorted_by_id: true,
        }
    }
}

impl MemHashStore {
    pub fn diff_with_new<'a>(&'a self, new: &'a Self) -> DiffingIter<Iter<'a, Entry>, Iter<'a, Entry>> {
        DiffingIter::new(self.sorted_ref_iter(), new.sorted_ref_iter())
    }

    pub fn find_by_id(&self, id: &HashArray<32>) -> Option<&Entry> {
        match self.entries.binary_search_by_key(id, |v| v.id) {
            Ok(index) => Some(&self.entries[index]),
            Err(_) => None,
        }
    }
}
