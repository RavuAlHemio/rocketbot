pub(crate) mod serde_f64_as_string {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error;

    pub(crate) fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        value.to_string()
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        let f64_string = String::deserialize(deserializer)?;
        f64_string.parse()
            .map_err(|_| D::Error::custom("parsing failed"))
    }
}
