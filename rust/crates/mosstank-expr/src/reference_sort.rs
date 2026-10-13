//! The sort the reference's listsort runs its comparison under: the
//! depth-limited quicksort of the runtime it is hosted in, with a heapsort
//! once the depth runs out. It is not stable and calls the comparison in a
//! fixed order, so a profile sees the same order of equal items and the
//! same last pair in `$1` and `$2` as it does there. A list of fewer than
//! two items is not compared at all.

use crate::error::{ExpressionError, Fault};
use crate::value::ExpressionValue;

const DEPTH_LIMIT: i32 = 32;

struct Sorter<'k, F> {
    keys: &'k mut [ExpressionValue],
    comparison: F,
}

/// The index walked off the end of the list (an inconsistent comparison).
struct OutOfRange;

enum Stop {
    Fault(Fault),
    OutOfRange,
}

impl From<Fault> for Stop {
    fn from(fault: Fault) -> Self {
        Self::Fault(fault)
    }
}

impl From<OutOfRange> for Stop {
    fn from(_: OutOfRange) -> Self {
        Self::OutOfRange
    }
}

/// Sorts `keys` with `comparison`.
///
/// # Errors
/// A comparison that contradicts itself (an argument error, as .NET's);
/// an expression error or argument error from the comparison passes
/// through; any other error becomes .NET's "Failed to compare two elements
/// in the array.".
pub fn sort<F>(keys: &mut [ExpressionValue], comparison: F) -> Result<(), Fault>
where
    F: FnMut(&ExpressionValue, &ExpressionValue) -> Result<i32, Fault>,
{
    if keys.len() < 2 {
        return Ok(());
    }
    let right = keys.len() - 1;
    let mut sorter = Sorter { keys, comparison };
    match sorter.quick_sort(0, right, DEPTH_LIMIT) {
        Ok(()) => Ok(()),
        Err(Stop::OutOfRange) => Err(Fault::argument(
            "Unable to sort because the comparison returns inconsistent results. Either a value does not compare equal to itself, or one value repeatedly compared to another value yields different results.",
        )),
        Err(Stop::Fault(fault)) => Err(match fault {
            Fault::Expression(ExpressionError::Cancelled) => fault,
            Fault::Expression(_) | Fault::Runtime { argument: true, .. } => fault,
            Fault::Runtime { .. } => Fault::runtime("Failed to compare two elements in the array."),
        }),
    }
}

impl<F> Sorter<'_, F>
where
    F: FnMut(&ExpressionValue, &ExpressionValue) -> Result<i32, Fault>,
{
    fn key(&self, index: isize) -> Result<ExpressionValue, OutOfRange> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.keys.get(index))
            .cloned()
            .ok_or(OutOfRange)
    }

    fn compare(&mut self, a: &ExpressionValue, b: &ExpressionValue) -> Result<i32, Stop> {
        Ok((self.comparison)(a, b)?)
    }

    fn quick_sort(&mut self, left: usize, right: usize, mut depth_limit: i32) -> Result<(), Stop> {
        let (mut left, mut right) = (
            isize::try_from(left).unwrap_or(isize::MAX),
            isize::try_from(right).unwrap_or(isize::MAX),
        );
        loop {
            if depth_limit == 0 {
                return self.heapsort(left, right);
            }
            let mut i = left;
            let mut j = right;
            // Order the low, middle and high items first; the middle one is
            // the pivot.
            let middle = i + ((j - i) >> 1);
            self.swap_if_greater(i, middle)?;
            self.swap_if_greater(i, j)?;
            self.swap_if_greater(middle, j)?;
            let pivot = self.key(middle)?;
            loop {
                loop {
                    let item = self.key(i)?;
                    if self.compare(&item, &pivot)? < 0 {
                        i += 1;
                    } else {
                        break;
                    }
                }
                loop {
                    let item = self.key(j)?;
                    if self.compare(&pivot, &item)? < 0 {
                        j -= 1;
                    } else {
                        break;
                    }
                }
                if i > j {
                    break;
                }
                if i < j {
                    self.swap(i, j);
                }
                i += 1;
                j -= 1;
                if i > j {
                    break;
                }
            }
            // The smaller side by recursion, the larger by the next pass;
            // both see the reduced depth.
            depth_limit -= 1;
            if j - left <= right - i {
                if left < j {
                    self.quick_sort(left.cast_unsigned(), j.cast_unsigned(), depth_limit)?;
                }
                left = i;
            } else {
                if i < right {
                    self.quick_sort(i.cast_unsigned(), right.cast_unsigned(), depth_limit)?;
                }
                right = j;
            }
            if left >= right {
                return Ok(());
            }
        }
    }

    fn swap(&mut self, a: isize, b: isize) {
        if a != b {
            self.keys.swap(a.cast_unsigned(), b.cast_unsigned());
        }
    }

    fn swap_if_greater(&mut self, a: isize, b: isize) -> Result<(), Stop> {
        if a != b {
            let (x, y) = (self.key(a)?, self.key(b)?);
            if self.compare(&x, &y)? > 0 {
                self.swap(a, b);
            }
        }
        Ok(())
    }

    fn heapsort(&mut self, lo: isize, hi: isize) -> Result<(), Stop> {
        let n = hi - lo + 1;
        let mut i = n / 2;
        while i >= 1 {
            self.down_heap(i, n, lo)?;
            i -= 1;
        }
        let mut i = n;
        while i > 1 {
            self.swap(lo, lo + i - 1);
            self.down_heap(1, i - 1, lo)?;
            i -= 1;
        }
        Ok(())
    }

    fn down_heap(&mut self, mut i: isize, n: isize, lo: isize) -> Result<(), Stop> {
        let d = self.key(lo + i - 1)?;
        while i <= n / 2 {
            let mut child = 2 * i;
            if child < n {
                let (a, b) = (self.key(lo + child - 1)?, self.key(lo + child)?);
                if self.compare(&a, &b)? < 0 {
                    child += 1;
                }
            }
            let c = self.key(lo + child - 1)?;
            if self.compare(&d, &c)? >= 0 {
                break;
            }
            self.keys[(lo + i - 1).cast_unsigned()] = c;
            i = child;
        }
        self.keys[(lo + i - 1).cast_unsigned()] = d;
        Ok(())
    }
}
