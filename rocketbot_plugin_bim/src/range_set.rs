use std::collections::BTreeSet;
use std::collections::btree_set::Iter as BTreeSetIter;
use std::fmt;
use std::ops::Range;
use std::mem::replace;


// std::iter::Step is apparently being completely redesigned
// meh
pub trait QuasiStep {
    fn successor(&self) -> Self;
}
macro_rules! implement_quasi_step_plus_one {
    ($type:ty) => {
        impl QuasiStep for $type {
            fn successor(&self) -> Self {
                self + 1
            }
        }
    };
}
implement_quasi_step_plus_one!(u8);
implement_quasi_step_plus_one!(u16);
implement_quasi_step_plus_one!(u32);
implement_quasi_step_plus_one!(u64);
implement_quasi_step_plus_one!(u128);
implement_quasi_step_plus_one!(usize);
implement_quasi_step_plus_one!(i8);
implement_quasi_step_plus_one!(i16);
implement_quasi_step_plus_one!(i32);
implement_quasi_step_plus_one!(i64);
implement_quasi_step_plus_one!(i128);
implement_quasi_step_plus_one!(isize);


#[derive(Debug, Clone, Default, Eq, Hash, PartialEq)]
pub struct OrderableRange<T: Ord> {
    pub range: Range<T>,
}
impl<T: Ord> OrderableRange<T> {
    pub fn overlaps(&self, other: &OrderableRange<T>) -> bool {
        !(
            self.range.end <= other.range.start
            || other.range.end <= self.range.start
        )
    }

    #[allow(unused)]
    pub fn envelops(&self, other: &OrderableRange<T>) -> bool {
        self.range.start <= other.range.start
            && self.range.end >= other.range.end
    }

    pub fn is_empty(&self) -> bool {
        self.range.start >= self.range.end
    }
}
impl<T: Ord> PartialOrd for OrderableRange<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (&self.range.start, &self.range.end)
            .partial_cmp(&(&other.range.start, &other.range.end))
    }
}
impl<T: Ord> Ord for OrderableRange<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl<T: Ord> From<Range<T>> for OrderableRange<T> {
    fn from(range: Range<T>) -> Self {
        Self {
            range,
        }
    }
}
impl<T: fmt::Display + Ord> fmt::Display for OrderableRange<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.range.start, self.range.end)
    }
}


pub struct RangeSet<T: Clone + Ord + QuasiStep> {
    inner_set: BTreeSet<OrderableRange<T>>,
}
impl<T: Clone + Ord + QuasiStep> RangeSet<T> {
    pub fn new() -> Self {
        let inner_set = BTreeSet::new();
        Self {
            inner_set,
        }
    }

    pub fn ranges(&self) -> BTreeSetIter<'_, OrderableRange<T>> {
        self.inner_set.iter()
    }

    #[allow(unused)]
    pub fn elements(&self) -> RangeSetElementIter<'_, T> {
        RangeSetElementIter::new(&self)
    }

    pub fn insert(&mut self, value: T) {
        let successor = value.successor();
        let new_range = value..successor;
        self.insert_range(new_range);
    }

    pub fn insert_range(&mut self, range: Range<T>) {
        self.inner_set.insert(OrderableRange {
            range,
        });
        self.simplify();
    }

    #[allow(unused)]
    pub fn remove(&mut self, value: T) {
        let successor = value.successor();
        let new_range = value..successor;
        self.remove_range(new_range);
    }

    #[allow(unused)]
    pub fn remove_range(&mut self, range: Range<T>) {
        let mut new_set = BTreeSet::new();
        let range_to_remove = OrderableRange {
            range,
        };
        if range_to_remove.is_empty() {
            return;
        }

        for rg in self.inner_set.iter() {
            if !rg.overlaps(&range_to_remove) {
                // stays unchanged
                new_set.insert(rg.clone());
                continue;
            }

            //   in set: ################
            // removing:       ####
            //   result: ######    ######

            let left_remainder = OrderableRange {
                range: Range {
                    start: rg.range.start.clone(),
                    end: range_to_remove.range.start.clone(),
                },
            };
            let right_remainder = OrderableRange {
                range: Range {
                    start: range_to_remove.range.end.clone(),
                    end: rg.range.end.clone(),
                },
            };

            if !left_remainder.is_empty() {
                new_set.insert(left_remainder);
            }
            if !right_remainder.is_empty() {
                new_set.insert(right_remainder);
            }
        }
        self.inner_set = new_set;
    }

    fn simplify(&mut self) {
        // only keep non-empty ranges
        self.inner_set.retain(|rg| rg.range.start < rg.range.end);

        if self.inner_set.len() < 2 {
            // nothing more to do
            return;
        }

        let mut new_set = BTreeSet::new();
        let mut changed = false;
        let mut iterator = self.inner_set.iter();
        let mut prev_elem = iterator.next().expect("inner set is suddenly empty").clone();
        while let Some(elem) = iterator.next() {
            if prev_elem.range.end >= elem.range.start {
                // overlap; merge them
                changed = true;
                prev_elem = OrderableRange {
                    range: prev_elem.range.start.clone()..elem.range.end.clone(),
                };
            } else {
                // no overlap; add the previous element and remember the current one
                new_set.insert(prev_elem);
                prev_elem = elem.clone();
            }
        }
        new_set.insert(prev_elem);

        if changed {
            self.inner_set = new_set;
        }
    }
}

pub struct RangeSetElementIter<'a, T: Clone + Ord + QuasiStep> {
    outer_iter: BTreeSetIter<'a, OrderableRange<T>>,
    inner_iter: Option<OrderableRange<T>>,
}
impl<'a, T: Clone + Ord + QuasiStep> RangeSetElementIter<'a, T> {
    fn new(range_set: &'a RangeSet<T>) -> Self {
        let mut outer_iter = range_set.ranges();
        let inner_iter = outer_iter.next().map(|r| r.clone());
        Self {
            outer_iter,
            inner_iter,
        }
    }
}
impl<'a, T: Clone + Ord + QuasiStep> Iterator for RangeSetElementIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ii) = &mut self.inner_iter {
            if ii.range.start < ii.range.end {
                let successor = ii.range.start.successor();
                let ret = replace(&mut ii.range.start, successor);
                Some(ret)
            } else {
                // advance the outer iterator
                match self.outer_iter.next() {
                    None => {
                        self.inner_iter = None;
                        None
                    },
                    Some(rg) => {
                        // there are no empty iterators, so this must succeed
                        assert!(rg.range.start < rg.range.end);
                        let mut my_range = rg.clone();
                        let successor = my_range.range.start.successor();
                        let ret = replace(&mut my_range.range.start, successor);
                        self.inner_iter = Some(my_range);
                        Some(ret)
                    },
                }
            }
        } else {
            // iterator is empty
            None
        }
    }
}


#[cfg(test)]
mod tests {
    use super::{OrderableRange, RangeSet};
    use std::ops::Range;

    #[test]
    fn test_range_overlap() {
        fn ro(olap: bool, a: Range<usize>, b: Range<usize>) {
            let a_or: OrderableRange<usize> = a.into();
            let b_or: OrderableRange<usize> = b.into();
            assert_eq!(olap, a_or.overlaps(&b_or));
        }

        // ####
        //  ##
        ro(true, 0..5, 1..3);

        //   ####
        // ####
        ro(true, 2..6, 0..4);

        // ####
        //   ####
        ro(true, 0..3, 2..6);

        //  ##
        // ####
        ro(true, 1..3, 0..5);

        //    ##
        // ##
        ro(false, 3..5, 0..2);

        // ##
        //    ##
        ro(false, 0..2, 3..5);

        // near misses:
        ro(false, 0..2, 2..4);
        ro(false, 2..4, 0..2);
    }

    #[test]
    fn test_empty_set() {
        let rs: RangeSet<usize> = RangeSet::new();

        let mut range_iter = rs.ranges();
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_single_range_set() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(0..2);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..2).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_independent_sets() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(4..7);
        rs.insert_range(0..2);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..2).into()), range_iter.next());
        assert_eq!(Some(&(4..7).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(Some(4), elem_iter.next());
        assert_eq!(Some(5), elem_iter.next());
        assert_eq!(Some(6), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_merging() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(2..7);
        rs.insert_range(0..2);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..7).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(Some(2), elem_iter.next());
        assert_eq!(Some(3), elem_iter.next());
        assert_eq!(Some(4), elem_iter.next());
        assert_eq!(Some(5), elem_iter.next());
        assert_eq!(Some(6), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_merging_overlap() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(0..5);
        rs.insert_range(2..7);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..7).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(Some(2), elem_iter.next());
        assert_eq!(Some(3), elem_iter.next());
        assert_eq!(Some(4), elem_iter.next());
        assert_eq!(Some(5), elem_iter.next());
        assert_eq!(Some(6), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_uncontained_range() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(3..5);
        rs.remove_range(10..15);
        rs.remove_range(0..2);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(3..5).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(3), elem_iter.next());
        assert_eq!(Some(4), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_full_range() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(3..5);
        rs.remove_range(3..5);

        let mut range_iter = rs.ranges();
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_enveloping_range() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(3..5);
        rs.remove_range(0..10);

        let mut range_iter = rs.ranges();
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_touching_ranges() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(3..5);
        rs.remove_range(0..3);
        rs.remove_range(5..8);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(3..5).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(3), elem_iter.next());
        assert_eq!(Some(4), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_encroaching_ranges() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(3..8);
        rs.remove_range(0..5);
        rs.remove_range(7..12);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(5..7).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(5), elem_iter.next());
        assert_eq!(Some(6), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_contained_range() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(0..8);
        rs.remove_range(3..5);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..3).into()), range_iter.next());
        assert_eq!(Some(&(5..8).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(Some(2), elem_iter.next());
        assert_eq!(Some(5), elem_iter.next());
        assert_eq!(Some(6), elem_iter.next());
        assert_eq!(Some(7), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_two_contained_ranges() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(0..12);
        rs.remove_range(3..5);
        rs.remove_range(8..10);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..3).into()), range_iter.next());
        assert_eq!(Some(&(5..8).into()), range_iter.next());
        assert_eq!(Some(&(10..12).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(Some(2), elem_iter.next());
        assert_eq!(Some(5), elem_iter.next());
        assert_eq!(Some(6), elem_iter.next());
        assert_eq!(Some(7), elem_iter.next());
        assert_eq!(Some(10), elem_iter.next());
        assert_eq!(Some(11), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_encroaching_on_two_ranges() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(0..5);
        rs.insert_range(8..12);
        rs.remove_range(3..10);

        let mut range_iter = rs.ranges();
        assert_eq!(Some(&(0..3).into()), range_iter.next());
        assert_eq!(Some(&(10..12).into()), range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(Some(0), elem_iter.next());
        assert_eq!(Some(1), elem_iter.next());
        assert_eq!(Some(2), elem_iter.next());
        assert_eq!(Some(10), elem_iter.next());
        assert_eq!(Some(11), elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }

    #[test]
    fn test_set_remove_enveloping_two_ranges() {
        let mut rs: RangeSet<usize> = RangeSet::new();
        rs.insert_range(3..5);
        rs.insert_range(8..12);
        rs.remove_range(0..16);

        let mut range_iter = rs.ranges();
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());
        assert_eq!(None, range_iter.next());

        let mut elem_iter = rs.elements();
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
        assert_eq!(None, elem_iter.next());
    }
}
