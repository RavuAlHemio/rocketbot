use std::cmp::Ordering;
use std::fmt;
use std::ops::{Deref, DerefMut};


/// Compares two slices of digit characters.
fn compare_digit_slices(left: &[char], right: &[char]) -> Ordering {
    assert!(left.iter().all(|c| c.is_ascii_digit()));
    assert!(right.iter().all(|c| c.is_ascii_digit()));

    // strip leading zeroes
    let mut trimmed_left = left;
    while trimmed_left.len() > 0 && trimmed_left[0] == '0' {
        trimmed_left = &trimmed_left[1..];
    }
    let mut trimmed_right = right;
    while trimmed_right.len() > 0 && trimmed_right[0] == '0' {
        trimmed_right = &trimmed_right[1..];
    }

    // at this point, the longer number is guaranteed to be larger
    let trimmed_length_cmp = trimmed_left.len().cmp(&trimmed_right.len());
    if trimmed_length_cmp.is_ne() {
        return trimmed_length_cmp;
    }

    // the numbers are the same length; compare them character by character
    for (l, r) in left.iter().zip(right.iter()) {
        let char_cmp = l.cmp(r);
        if char_cmp.is_ne() {
            return char_cmp;
        }
    }

    // the numbers are numerically equal; return the shorter one as smaller
    // (i.e. compare the lengths of the original numbers)
    left.len().cmp(&right.len())
}

/// Returns a subslice containing all the elements at the beginning of `slice` for which `pred`
/// returns `true`.
///
/// Stops at the first element for which `pred` returns `false`; this element is not included in the
/// returned slice. If `pred` returns `true` for all elements in `slice`, returns a slice that is
/// identical to `slice`.
fn slice_prefix<T, P: FnMut(&T) -> bool>(slice: &[T], mut pred: P) -> &[T] {
    for i in 0..slice.len() {
        if !pred(&slice[i]) {
            return &slice[0..i];
        }
    }
    slice
}

/// Compares two strings, identifying sequences of digits and comparing them numerically.
pub fn natural_compare(left: &str, right: &str) -> Ordering {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();

    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left_chars.len() && right_index < right_chars.len() {
        // try taking digits first
        let left_digit_slice = slice_prefix(&left_chars[left_index..], |c| c.is_ascii_digit());
        let right_digit_slice = slice_prefix(&right_chars[right_index..], |c| c.is_ascii_digit());

        // in the mixed case, do an ASCIIbetical sort
        if (left_digit_slice.len() == 0) != (right_digit_slice.len() == 0) {
            return left_digit_slice.cmp(right_digit_slice);
        }

        if left_digit_slice.len() > 0 {
            assert!(right_digit_slice.len() > 0);

            // digits! compare them numerically!
            let digit_cmp = compare_digit_slices(left_digit_slice, right_digit_slice);
            if digit_cmp.is_ne() {
                return digit_cmp;
            }

            // they were the same; skip over them and keep going
            left_index += left_digit_slice.len();
            right_index += right_digit_slice.len();
        }

        // not digits! compare them ASCIIbetically!
        let left_nondigit_slice = slice_prefix(&left_chars[left_index..], |c| !c.is_ascii_digit());
        let right_nondigit_slice = slice_prefix(&right_chars[right_index..], |c| !c.is_ascii_digit());
        let non_digit_cmp = left_nondigit_slice.cmp(right_nondigit_slice);
        if non_digit_cmp.is_ne() {
            return non_digit_cmp;
        }

        // they were the same; skip over them and keep going
        left_index += left_nondigit_slice.len();
        right_index += right_nondigit_slice.len();
    }

    // all segments until now compared equal
    // compare by length
    left.len().cmp(&right.len())
}


/// A string that is subject to natural ordering rules ([`natural_compare`]).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct NatSortedString(String);
impl NatSortedString {
    /// Wraps the given [`String`] into a `NatSortedString`.
    pub const fn from_string(s: String) -> Self { Self(s) }

    /// Unwraps the `NatSortedString` into a [`String`].
    pub fn into_string(self) -> String { self.0 }
}
impl fmt::Display for NatSortedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl PartialOrd for NatSortedString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(natural_compare(&self.0, &other.0))
    }
}
impl Ord for NatSortedString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl Deref for NatSortedString {
    type Target = String;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for NatSortedString {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}
impl From<String> for NatSortedString {
    fn from(inner: String) -> Self { Self(inner) }
}

#[cfg(feature = "serde")]
impl serde::Serialize for NatSortedString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for NatSortedString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let inner = String::deserialize(deserializer)?;
        Ok(Self(inner))
    }
}


#[cfg(test)]
mod tests {
    use super::natural_compare;
    use std::cmp::Ordering;

    #[test]
    fn test_natural_compare() {
        assert_eq!(natural_compare("", ""), Ordering::Equal);
        assert_eq!(natural_compare("", "a"), Ordering::Less);
        assert_eq!(natural_compare("a", ""), Ordering::Greater);
        assert_eq!(natural_compare("", "4"), Ordering::Less);
        assert_eq!(natural_compare("4", ""), Ordering::Greater);
        assert_eq!(natural_compare("3", "12"), Ordering::Less);
        assert_eq!(natural_compare("12", "3"), Ordering::Greater);
        assert_eq!(natural_compare("abc3", "abc12"), Ordering::Less);
        assert_eq!(natural_compare("abc12", "abc3"), Ordering::Greater);
        assert_eq!(natural_compare("abc3def", "abc12def"), Ordering::Less);
        assert_eq!(natural_compare("abc12def", "abc3def"), Ordering::Greater);
        assert_eq!(natural_compare("3abc", "12abc"), Ordering::Less);
        assert_eq!(natural_compare("12abc", "3abc"), Ordering::Greater);
        assert_eq!(natural_compare("3abc", "3def"), Ordering::Less);
        assert_eq!(natural_compare("3def", "3abc"), Ordering::Greater);
        assert_eq!(natural_compare("3abc3", "3abc3"), Ordering::Equal);
        assert_eq!(natural_compare("abc3def", "abc3def"), Ordering::Equal);
    }
}
