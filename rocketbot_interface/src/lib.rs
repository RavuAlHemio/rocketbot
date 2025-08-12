pub mod clown;
pub mod commands;
pub mod errors;
pub mod interfaces;
pub mod macros;
pub mod message;
pub mod model;
pub mod serde;
pub mod sync;


use std::fmt;
use std::slice;

use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::Lazy;
use serde_json;
use tracing;


static EMPTY_MAP: Lazy<serde_json::Map<String, serde_json::Value>> = Lazy::new(|| serde_json::Map::new());


macro_rules! uint_conv {
    ($fn_name:ident, $target_type:ty) => {
        fn $fn_name(&self) -> Option<$target_type> {
            self.as_u64()
                .map(|u| u.try_into().ok()).flatten()
        }
    };
}


/// Adds convenience functions for working with JSON values.
pub trait JsonValueExtensions {
    /// Attempts to interpret the JSON value as an unsigned 8-bit value.
    ///
    /// Returns `Some(_)` if the value is a [number][serde_json::Value::Number] and fits into `u8`;
    /// returns `None` otherwise.
    fn as_u8(&self) -> Option<u8>;

    /// Attempts to interpret the JSON value as an unsigned 32-bit value.
    ///
    /// Returns `Some(_)` if the value is a [number][serde_json::Value::Number] and fits into `u32`;
    /// returns `None` otherwise.
    fn as_u32(&self) -> Option<u32>;

    /// Attempts to interpret the JSON value as an unsigned pointer-sized value.
    ///
    /// Returns `Some(_)` if the value is a [number][serde_json::Value::Number] and fits into
    /// `usize`; returns `None` otherwise.
    fn as_usize(&self) -> Option<usize>;

    /// Attempts to interpret the JSON value as an object and return an iterator of its entries.
    ///
    /// Returns `Some(_)` (an iterator over the object's entries) if the value is an
    /// [object][serde_json::Value::Object]; returns `None` otherwise.
    fn entries(&self) -> Option<serde_json::map::Iter<'_>>;

    /// Attempts to interpret the JSON value as a list and return an iterator of its members.
    ///
    /// Returns `Some(_)` (an iterator over the list's members) if the value is a
    /// [list][serde_json::Value::Array]; returns `None` otherwise.
    fn members(&self) -> Option<slice::Iter<'_, serde_json::Value>>;

    /// Attempts to interpret the JSON value as an object and return whether it contains the given
    /// key.
    ///
    /// Returns `true` if the value is an [object][serde_json::Value::Object] and contains the given
    /// key; returns `false` otherwise.
    fn has_key(&self, key: &str) -> bool;

    /// Attempts to interpret the JSON value as a string and return its value or an empty string.
    ///
    /// If the value is a [string][serde_json::Value::String], returns a reference to this string;
    /// otherwise, returns `""`.
    fn as_str_or_empty(&self) -> &str;

    /// Attempts to interpret the JSON value as an unsigned 64-bit value, falling back on a default
    /// value if it is missing.
    ///
    /// Returns `Some(_)` if the value is a [number][serde_json::Value::Number] and fits into
    /// `u64`. Returns `Some(default)` if the value is null or missing. Returns `None` if the value
    /// is present but of a different type.
    fn as_u64_or_strict(&self, default: u64) -> Option<u64>;

    /// Attempts to interpret the JSON value as an object and return an iterator over its entries,
    /// or returns an iterator over an empty JSON object.
    ///
    /// If the value is an [object][serde_json::Value::Object], returns an iterator over the entries
    /// of this object; otherwise, returns an iterator over the entries of an empty object (an empty
    /// iterator).
    fn entries_or_empty(&self) -> serde_json::map::Iter<'_> {
        match self.entries() {
            Some(i) => i,
            None => EMPTY_MAP.iter(),
        }
    }

    /// Attempts to interpret the JSON value as a list and return an iterator over its members,
    /// or returns an iterator over an empty JSON list.
    ///
    /// If the value is a [list][serde_json::Value::Array], returns an iterator over the members of
    /// this array; otherwise, returns an iterator over the entries of an empty object (an empty
    /// iterator).
    fn members_or_empty(&self) -> slice::Iter<'_, serde_json::Value> {
        match self.members() {
            Some(i) => i,
            None => [].iter(),
        }
    }

    /// Attempts to interpret the JSON value as an object and return an iterator over its entries or
    /// an empty iterator for null values.
    ///
    /// If the value is an [object][serde_json::Value::Object], returns the iterator over the
    /// entries of this object; if the value is [null][serde_json::Value::Null], returns an iterator
    /// over the entries of an empty object (an empty iterator); otherwise, i.e. if the value has
    /// any other type, returns `None`.
    fn entries_or_empty_strict(&self) -> Option<serde_json::map::Iter<'_>>;

    /// Attempts to interpret the JSON value as a list and return an iterator over its entries or
    /// an empty iterator for null values.
    ///
    /// If the value is a [list][serde_json::Value::Array], returns the iterator over the members
    /// of this list; if the value is [null][serde_json::Value::Null], returns an iterator over the
    /// entries of an empty list (an empty iterator); otherwise, i.e. if the value has any other
    /// type, returns `None`.
    fn members_or_empty_strict(&self) -> Option<slice::Iter<'_, serde_json::Value>>;

    /// Inserts the given key and value into this object value. Panics if this is not an object
    /// value.
    fn insert(&mut self, key: String, val: serde_json::Value) -> Option<serde_json::Value>;
}
impl JsonValueExtensions for serde_json::Value {
    uint_conv!(as_u8, u8);
    uint_conv!(as_u32, u32);
    uint_conv!(as_usize, usize);

    fn entries(&self) -> Option<serde_json::map::Iter<'_>> {
        self.as_object().map(|o| o.iter())
    }

    fn members(&self) -> Option<slice::Iter<'_, serde_json::Value>> {
        self.as_array().map(|o| o.iter())
    }

    fn has_key(&self, key: &str) -> bool {
        match self.as_object() {
            Some(o) => o.contains_key(key),
            None => false,
        }
    }

    fn as_str_or_empty(&self) -> &str {
        self.as_str().unwrap_or("")
    }

    fn as_u64_or_strict(&self, default: u64) -> Option<u64> {
        if self.is_null() {
            Some(default)
        } else {
            self.as_u64()
        }
    }

    fn entries_or_empty_strict(&self) -> Option<serde_json::map::Iter<'_>> {
        if self.is_null() {
            Some(EMPTY_MAP.iter())
        } else {
            self.entries()
        }
    }

    fn members_or_empty_strict(&self) -> Option<slice::Iter<'_, serde_json::Value>> {
        if self.is_null() {
            Some([].iter())
        } else {
            self.members()
        }
    }

    fn insert(&mut self, key: String, val: serde_json::Value) -> Option<serde_json::Value> {
        if let serde_json::Value::Object(map) = self {
            map.insert(key, val)
        } else {
            panic!("this is not an object value")
        }
    }
}


/// Add convenience functions to [`Result`] types.
pub trait ResultExtensions<T, E> {
    /// If `self` is [`Ok(_)`], returns `self`. If self is [`Err(e)`], logs `error_message` and `e`
    /// using [`log::error!`] and returns [`Err(error_message)`].
    fn or_msg(self, error_message: &'static str) -> Result<T, &'static str>;
}
impl<T, E: fmt::Display> ResultExtensions<T, E> for Result<T, E> {
    fn or_msg(self, error_message: &'static str) -> Result<T, &'static str> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => {
                tracing::error!("{}: {}", error_message, e);
                Err(error_message)
            },
        }
    }
}


pub fn is_sorted<T: Ord, I: Iterator<Item = T>>(mut iterator: I) -> bool {
    let mut prev = match iterator.next() {
        None => return true, // vacuous truth
        Some(p) => p,
    };

    while let Some(next) = iterator.next() {
        if prev > next {
            return false;
        }
        prev = next;
    }

    true
}

pub fn is_sorted_no_dupes<T: Ord, I: Iterator<Item = T>>(mut iterator: I) -> bool {
    let mut prev = match iterator.next() {
        None => return true, // vacuous truth
        Some(p) => p,
    };

    while let Some(next) = iterator.next() {
        if prev >= next {
            return false;
        }
        prev = next;
    }

    true
}

pub fn rocketchat_timestamp_to_datetime(timestamp: i64) -> DateTime<Utc> {
    let timestamp_nsecs: u32 = ((timestamp % 1_000) * 1_000_000).try_into().unwrap();
    Utc.timestamp_opt(timestamp / 1_000, timestamp_nsecs).single().unwrap()
}

pub fn phrase_join<S: AsRef<str>>(items: &[S], general_glue: &str, final_glue: &str) -> String {
    let mut ret = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            if i < items.len() - 1 {
                ret.push_str(general_glue);
            } else {
                ret.push_str(final_glue);
            }
        }
        ret.push_str(item.as_ref());
    }
    ret
}

pub fn add_thousands_separators(separate_me: &mut String, separator: &str) {
    if separator.len() == 0 {
        return;
    }
    if separate_me.len() < 4 {
        return;
    }

    let mut i = separate_me.len() - 3;
    loop {
        separate_me.insert_str(i, separator);
        if i < 4 {
            break;
        }
        i -= 3;
    }
}

#[cfg(test)]
mod tests {
    use super::add_thousands_separators;

    #[test]
    fn test_add_thousands_separators() {
        fn tat(separate_me: &str, separator: &str, expected: &str) {
            let mut separate_me_owned = separate_me.to_owned();
            add_thousands_separators(&mut separate_me_owned, separator);
            assert_eq!(separate_me_owned.as_str(), expected);
        }

        tat("", ",", "");
        tat("", "'", "");

        tat("1", ",", "1");
        tat("12", ",", "12");
        tat("123", ",", "123");

        tat("1234", ",", "1,234");
        tat("1234", "'", "1'234");
        tat("1234", "\\,", "1\\,234");
        tat("1234", "argh", "1argh234");

        tat("123456", ",", "123,456");
        tat("123456", "'", "123'456");
        tat("123456", "\\,", "123\\,456");
        tat("123456", "argh", "123argh456");

        tat("1234567", ",", "1,234,567");
        tat("1234567", "'", "1'234'567");
        tat("1234567", "\\,", "1\\,234\\,567");
        tat("1234567", "argh", "1argh234argh567");

        tat("123456789", ",", "123,456,789");
        tat("123456789", "'", "123'456'789");
        tat("123456789", "\\,", "123\\,456\\,789");
        tat("123456789", "argh", "123argh456argh789");

        tat("1234567890", ",", "1,234,567,890");
        tat("1234567890", "'", "1'234'567'890");
        tat("1234567890", "\\,", "1\\,234\\,567\\,890");
        tat("1234567890", "argh", "1argh234argh567argh890");
    }
}
