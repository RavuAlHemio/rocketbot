pub(crate) mod serde_opt_big_decimal {
    use bigdecimal::BigDecimal;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error as DeError;

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<BigDecimal>, D::Error> {
        let string_opt: Option<String> = Option::deserialize(deserializer)?;
        match string_opt {
            Some(string) => {
                string.parse()
                    .map(Some)
                    .map_err(DeError::custom)
            },
            None => Ok(None)
        }
    }

    pub fn serialize<S: Serializer>(value: &Option<BigDecimal>, serializer: S) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .map(|v| v.to_string())
            .serialize(serializer)
    }
}
