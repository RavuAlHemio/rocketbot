pub mod serde_regex {
    use regex::Regex;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error as DeError;

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Regex, D::Error> {
        let s = String::deserialize(deserializer)?;
        Regex::new(&s)
            .map_err(DeError::custom)
    }

    pub fn serialize<S: Serializer>(regex: &Regex, serializer: S) -> Result<S::Ok, S::Error> {
        regex.as_str()
            .serialize(serializer)
    }
}

pub mod serde_opt_regex {
    use regex::Regex;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error as DeError;

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Regex>, D::Error> {
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            None => Ok(None),
            Some(so) => match Regex::new(&so) {
                Ok(r) => Ok(Some(r)),
                Err(e) => Err(DeError::custom(e)),
            },
        }
    }

    pub fn serialize<S: Serializer>(regex: &Option<Regex>, serializer: S) -> Result<S::Ok, S::Error> {
        regex
            .as_ref()
            .map(|r| r.as_str())
            .serialize(serializer)
    }
}
