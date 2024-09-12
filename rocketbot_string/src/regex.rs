use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

use regex::Regex;


/// Regex with creature comforts (Eq, Ord, Deserialize, Serialize).
#[derive(Clone, Debug)]
pub struct EnjoyableRegex(Regex);
impl EnjoyableRegex {
    /// Wraps the given [`Regex`] into a `EnjoyableRegex`.
    pub const fn from_regex(r: Regex) -> Self { Self(r) }

    /// Unwraps the `EnjoyableRegex` into a [`Regex`].
    pub fn into_regex(self) -> Regex { self.0 }
}
impl fmt::Display for EnjoyableRegex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl PartialEq for EnjoyableRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}
impl Eq for EnjoyableRegex {}
impl Hash for EnjoyableRegex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}
impl PartialOrd for EnjoyableRegex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for EnjoyableRegex {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}
impl Deref for EnjoyableRegex {
    type Target = Regex;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for EnjoyableRegex {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}
impl From<Regex> for EnjoyableRegex {
    fn from(inner: Regex) -> Self { Self(inner) }
}

#[cfg(feature = "serde")]
impl serde::Serialize for EnjoyableRegex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_str().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for EnjoyableRegex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let string = String::deserialize(deserializer)?;
        let regex = Regex::new(&string)
            .map_err(|e| D::Error::custom(e))?;
        Ok(Self(regex))
    }
}
