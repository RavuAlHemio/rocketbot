pub mod commands;
pub mod errors;
pub mod interfaces;
pub mod macros;
pub mod message;
pub mod model;
pub mod sync;


use std::convert::TryInto;
use std::slice;

use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::Lazy;
use serde_json;


static EMPTY_MAP: Lazy<serde_json::Map<String, serde_json::Value>> = Lazy::new(|| serde_json::Map::new());


macro_rules! uint_conv {
    ($fn_name:ident, $target_type:ty) => {
        fn $fn_name(&self) -> Option<$target_type> {
            self.as_u64()
                .map(|u| u.try_into().ok()).flatten()
        }
    };
}


pub trait JsonValueExtensions {
    fn as_u8(&self) -> Option<u8>;
    fn as_u32(&self) -> Option<u32>;
    fn as_usize(&self) -> Option<usize>;
    fn entries(&self) -> Option<serde_json::map::Iter>;
    fn members(&self) -> Option<slice::Iter<serde_json::Value>>;
    fn has_key(&self, key: &str) -> bool;
    fn as_str_or_empty(&self) -> &str;

    fn entries_or_empty(&self) -> serde_json::map::Iter {
        match self.entries() {
            Some(i) => i,
            None => EMPTY_MAP.iter(),
        }
    }

    fn members_or_empty(&self) -> slice::Iter<serde_json::Value> {
        match self.members() {
            Some(i) => i,
            None => [].iter(),
        }
    }

    fn insert(&mut self, key: String, val: serde_json::Value) -> Option<serde_json::Value>;
}
impl JsonValueExtensions for serde_json::Value {
    uint_conv!(as_u8, u8);
    uint_conv!(as_u32, u32);
    uint_conv!(as_usize, usize);

    fn entries(&self) -> Option<serde_json::map::Iter> {
        self.as_object().map(|o| o.iter())
    }

    fn members(&self) -> Option<slice::Iter<serde_json::Value>> {
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

    fn insert(&mut self, key: String, val: serde_json::Value) -> Option<serde_json::Value> {
        if let serde_json::Value::Object(map) = self {
            map.insert(key, val)
        } else {
            panic!("this is not an object value")
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
    Utc.timestamp(timestamp / 1_000, timestamp_nsecs)
}
