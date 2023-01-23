use itertools::Itertools;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::mem::swap;
use std::slice::IterMut;

///find where data starts to be unsorted
pub fn find_sort_split_index<T>(array: &[T], mut compare: impl FnMut(&T, &T) -> Ordering) -> usize {
    array
        .iter()
        .tuple_windows::<(_, _)>()
        .position(|(a, b)| compare(a, b) == Ordering::Greater)
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn merge_sort_arrays<T>(prev: &mut [T], next: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering)
where
    T: Debug,
{
    let mut prev_array = prev;
    let mut next_array = next;
    let mut pi = 0;
    let mut ni = 0;

    loop {
        let (prev, next) = match (prev_array.get_mut(pi), next_array.get_mut(ni)) {
            (Some(p), Some(n)) => (p, n),
            (Some(_), None) => {
                (next_array, prev_array) = prev_array.split_at_mut(pi);
                if prev_array.is_empty() || next_array.is_empty() {
                    return;
                }
                pi = 0;
                ni = 0;
                continue;
            }
            (None, Some(_)) => {
                (prev_array, next_array) = next_array.split_at_mut(ni);
                if prev_array.is_empty() || next_array.is_empty() {
                    return;
                }
                pi = 0;
                ni = 0;
                continue;
            }
            (None, None) => break,
        };
        match compare(prev, next) {
            Ordering::Equal | Ordering::Less => {
                //dont swap anything, only advance prev_iter
                //println!("leq: {prev:?} {next:?}");
                pi += 1;
            }
            Ordering::Greater => {
                //println!("gr: {prev:?} {next:?}");
                swap(prev, next);
                ni += 1;
            }
        }
    }
}

///run from start of array, and merge first two sorted parts of elements into one sorted part
fn merge_sort_in_place<'a, A, B, T>(mut prev: A, mut next: B, mut compare: impl FnMut(&T, &T) -> Ordering)
where
    A: IntoIterator<Item = &'a mut T>,
    B: IntoIterator<Item = &'a mut T>,
    T: Debug + 'a,
{
    let mut prev_iter = prev.into_iter();
    let mut next_iter = next.into_iter();
    let mut curr_prev = prev_iter.next();
    let mut curr_next = next_iter.next();

    #[inline]
    fn bubble<'b, I, E: 'b>(value: &mut E, mut iter: I, mut compare: impl FnMut(&E, &E) -> Ordering)
    where
        I: Iterator<Item = &'b mut E>,
        E: Debug,
    {
        for elem in iter {
            if compare(value, elem) == Ordering::Greater {
                swap(value, elem);
            } else {
                break;
            }
        }
    }

    loop {
        let (prev, next) = match (curr_prev.as_deref_mut(), curr_next.as_deref_mut()) {
            (Some(p), Some(n)) => (p, n),
            (Some(v), None) => {
                bubble(v, prev_iter, compare);
                break;
            }
            (None, Some(v)) => {
                bubble(v, next_iter, compare);
                break;
            }
            (None, None) => break,
        };
        match compare(prev, next) {
            Ordering::Equal | Ordering::Less => {
                //dont swap anything, only advance prev_iter
                curr_prev = prev_iter.next();
            }
            Ordering::Greater => {
                swap(prev, next);
                curr_next = next_iter.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;

    #[test]
    fn test_merge_sort() {
        let mut rng = &mut StdRng::seed_from_u64(1235);

        for _ in 0..1 {
            println!("*****");
            let size = rng.gen_range(10..20);
            let mut array = vec![0u16; size].into_boxed_slice();
            array.iter_mut().enumerate().for_each(|(i, v)| *v = i as _);
            array.shuffle(rng);
            // let mut array = vec![1,2,3,4,5,6,7,8,9,10].into_boxed_slice();
            // array.shuffle(&mut rng);
            let split = rng.gen_range(0..size);
            let (left, right) = array.split_at_mut(split);
            left.sort_unstable();
            right.sort_unstable();

            println!("left: {:?}\nright: {:?}", left, right);

            merge_sort_arrays(left, right, u16::cmp);

            println!("array: {:?}", array);
            assert!(array.split_at(split).0.iter().enumerate().all(|(i, v)| *v == i as _));
            //assert!(array.split_at_mut(split).0.windows(2).all(|w| w[0].cmp(&w[1]) != Ordering::Greater));
        }
    }
}
