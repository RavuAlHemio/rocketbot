use async_trait::async_trait;
use bytes::Buf;
use chrono::{DateTime, Duration, Local, Utc};
use log::error;
use num_rational::Rational64;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

use crate::interface::VitalsReader;


trait BeepeeMeasurement {
    fn id(&self) -> i64;
    fn timestamp(&self) -> DateTime<Local>;
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct BloodPressureEntry {
    pub id: i64,

    #[serde(rename = "zoned_timestamp", with = "serde_date_string")]
    pub timestamp: DateTime<Local>,

    pub systolic_mmhg: i32,

    pub diastolic_mmhg: i32,

    pub pulse_bpm: i32,

    pub spo2_percent: Option<i32>,
}
impl BeepeeMeasurement for BloodPressureEntry {
    fn id(&self) -> i64 { self.id }
    fn timestamp(&self) -> DateTime<Local> { self.timestamp }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct BodyMassEntry {
    pub id: i64,

    #[serde(rename = "zoned_timestamp", with = "serde_date_string")]
    pub timestamp: DateTime<Local>,

    #[serde(with = "serde_rational")]
    pub mass_kg: Rational64,

    #[serde(with = "serde_opt_rational")]
    pub bmi: Option<Rational64>,
}
impl BeepeeMeasurement for BodyMassEntry {
    fn id(&self) -> i64 { self.id }
    fn timestamp(&self) -> DateTime<Local> { self.timestamp }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct BodyTemperatureEntry {
    pub id: i64,

    #[serde(rename = "zoned_timestamp", with = "serde_date_string")]
    pub timestamp: DateTime<Local>,

    #[serde(with = "serde_i64_string")]
    pub location_id: i64,

    #[serde(with = "serde_rational")]
    pub temperature_celsius: Rational64,
}
impl BeepeeMeasurement for BodyTemperatureEntry {
    fn id(&self) -> i64 { self.id }
    fn timestamp(&self) -> DateTime<Local> { self.timestamp }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct BloodSugarEntry {
    pub id: i64,

    #[serde(rename = "zoned_timestamp", with = "serde_date_string")]
    pub timestamp: DateTime<Local>,

    #[serde(with = "serde_rational")]
    pub sugar_mmol_per_l: Rational64,

    #[serde(with = "serde_rational")]
    pub sugar_mg_per_dl: Rational64,
}
impl BeepeeMeasurement for BloodSugarEntry {
    fn id(&self) -> i64 { self.id }
    fn timestamp(&self) -> DateTime<Local> { self.timestamp }
}


pub(crate) enum MeasurementResult<T> {
    NoUri,
    FetchFailed,
    NoMeasurement,
    Measurement(T),
}


fn phrase_join<S: AsRef<str>>(items: &[S], general_glue: &str, final_glue: &str) -> String {
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


pub(crate) struct BeepeeReader {
    bp_uri: Option<String>,
    mass_uri: Option<String>,
    temp_uri: Option<String>,
    sugar_uri: Option<String>,
    cutoff_days: Option<i64>,
}
impl BeepeeReader {
    async fn get_json<T: BeepeeMeasurement + DeserializeOwned>(&self, what: &str, uri: &str) -> MeasurementResult<T> {
        let response = match reqwest::get(uri).await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching {} from {:?}: {}", what, uri, e);
                return MeasurementResult::FetchFailed;
            },
        };
        let bytes = match response.bytes().await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching {} bytes from {:?}: {}", what, uri, e);
                return MeasurementResult::FetchFailed;
            },
        };
        let mut entries: Vec<T> = match serde_json::from_reader(bytes.reader()) {
            Ok(t) => t,
            Err(e) => {
                error!("error parsing {} JSON from {:?}: {}", what, uri, e);
                return MeasurementResult::FetchFailed;
            },
        };

        // filter entries
        let now = Utc::now();
        let cutoff_timestamp = self.cutoff_days.map(|d| now - Duration::days(d));
        entries.retain(|e| e.timestamp() <= now);
        entries.retain(|e| cutoff_timestamp.map(|cots| e.timestamp() >= cots).unwrap_or(true));
        entries.sort_unstable_by_key(|e| e.timestamp());
        match entries.pop() {
            Some(e) => MeasurementResult::Measurement(e),
            None => MeasurementResult::NoMeasurement,
        }
    }

    async fn get_bp(&self) -> MeasurementResult<String> {
        let bp_uri = match &self.bp_uri {
            Some(u) => u.as_str(),
            None => return MeasurementResult::NoUri,
        };
        let newest: BloodPressureEntry = match self.get_json("blood pressure", bp_uri).await {
            MeasurementResult::Measurement(m) => m,
            MeasurementResult::NoUri => return MeasurementResult::NoUri,
            MeasurementResult::FetchFailed => return MeasurementResult::FetchFailed,
            MeasurementResult::NoMeasurement => return MeasurementResult::NoMeasurement,
        };

        let spo2_piece = if let Some(spo2) = newest.spo2_percent {
            format!(" with {}% SpO\u{2082}", spo2)
        } else {
            String::new()
        };
        MeasurementResult::Measurement(format!(
            "{}/{} mmHg at {} bpm{} at {}",
            newest.systolic_mmhg, newest.diastolic_mmhg, newest.pulse_bpm,
            spo2_piece,
            newest.timestamp.format("%Y-%m-%d %H:%M:%S"),
        ))
    }

    async fn get_mass(&self) -> MeasurementResult<String> {
        let mass_uri = match &self.mass_uri {
            Some(u) => u.as_str(),
            None => return MeasurementResult::NoUri,
        };
        let newest: BodyMassEntry = match self.get_json("body mass", mass_uri).await {
            MeasurementResult::Measurement(m) => m,
            MeasurementResult::NoUri => return MeasurementResult::NoUri,
            MeasurementResult::FetchFailed => return MeasurementResult::FetchFailed,
            MeasurementResult::NoMeasurement => return MeasurementResult::NoMeasurement,
        };

        let bmi_piece = if let Some(bmi) = newest.bmi {
            format!(" (BMI {:.01})", (*bmi.numer() as f64) / (*bmi.denom() as f64))
        } else {
            String::new()
        };
        let kg: f64 = (*newest.mass_kg.numer() as f64) / (*newest.mass_kg.denom() as f64);
        MeasurementResult::Measurement(format!(
            "{:.01} kg{} at {}",
            kg, bmi_piece, newest.timestamp.format("%Y-%m-%d %H:%M:%S"),
        ))
    }

    async fn get_temperature(&self) -> MeasurementResult<String> {
        let temp_uri = match &self.temp_uri {
            Some(u) => u.as_str(),
            None => return MeasurementResult::NoUri,
        };
        let newest: BodyTemperatureEntry = match self.get_json("body temperature", temp_uri).await {
            MeasurementResult::Measurement(m) => m,
            MeasurementResult::NoUri => return MeasurementResult::NoUri,
            MeasurementResult::FetchFailed => return MeasurementResult::FetchFailed,
            MeasurementResult::NoMeasurement => return MeasurementResult::NoMeasurement,
        };

        let celsius: f64 = (*newest.temperature_celsius.numer() as f64) / (*newest.temperature_celsius.denom() as f64);
        MeasurementResult::Measurement(format!(
            "{:.01} °C at {}",
            celsius, newest.timestamp.format("%Y-%m-%d %H:%M:%S"),
        ))
    }

    async fn get_blood_sugar(&self) -> MeasurementResult<String> {
        let sugar_uri = match &self.sugar_uri {
            Some(u) => u.as_str(),
            None => return MeasurementResult::NoUri,
        };
        let newest: BloodSugarEntry = match self.get_json("blood sugar", sugar_uri).await {
            MeasurementResult::Measurement(m) => m,
            MeasurementResult::NoUri => return MeasurementResult::NoUri,
            MeasurementResult::FetchFailed => return MeasurementResult::FetchFailed,
            MeasurementResult::NoMeasurement => return MeasurementResult::NoMeasurement,
        };

        let mg_per_dl = (*newest.sugar_mg_per_dl.numer() as f64) / (*newest.sugar_mg_per_dl.denom() as f64);
        let mmol_per_l = (*newest.sugar_mmol_per_l.numer() as f64) / (*newest.sugar_mmol_per_l.denom() as f64);
        MeasurementResult::Measurement(format!(
            "{:.01} mg/dl ({:.01} mmol/l) at {}",
            mg_per_dl, mmol_per_l, newest.timestamp.format("%Y-%m-%d %H:%M:%S"),
        ))
    }

    fn config_str(config: &serde_json::Value, key: &str) -> Option<String> {
        if config[key].is_null() {
            None
        } else {
            match config[key].as_str() {
                None => panic!("{} not a string", key),
                Some(k) => Some(k.to_owned()),
            }
        }
    }
}
#[async_trait]
impl VitalsReader for BeepeeReader {
    async fn new(config: &serde_json::Value) -> Self {
        let bp_uri = Self::config_str(config, "bp_uri");
        let mass_uri = Self::config_str(config, "mass_uri");
        let temp_uri = Self::config_str(config, "temp_uri");
        let sugar_uri = Self::config_str(config, "sugar_uri");

        let cutoff_days = if config["cutoff_days"].is_null() {
            None
        } else {
            Some(config["cutoff_days"].as_i64().expect("cutoff_days not i64"))
        };

        Self {
            bp_uri,
            mass_uri,
            temp_uri,
            sugar_uri,
            cutoff_days,
        }
    }

    async fn read(&self) -> Option<String> {
        let mut pieces: Vec<String> = Vec::new();
        let mut failures: Vec<&str> = Vec::new();
        let mut empties: Vec<&str> = Vec::new();
        match self.get_bp().await {
            MeasurementResult::NoUri => {},
            MeasurementResult::Measurement(m) => pieces.push(m),
            MeasurementResult::FetchFailed => failures.push("blood pressure"),
            MeasurementResult::NoMeasurement => empties.push("blood pressure"),
        }
        match self.get_mass().await {
            MeasurementResult::NoUri => {},
            MeasurementResult::Measurement(m) => pieces.push(m),
            MeasurementResult::FetchFailed => failures.push("body mass"),
            MeasurementResult::NoMeasurement => empties.push("body mass"),
        }
        match self.get_temperature().await {
            MeasurementResult::NoUri => {},
            MeasurementResult::Measurement(m) => pieces.push(m),
            MeasurementResult::FetchFailed => failures.push("body temperature"),
            MeasurementResult::NoMeasurement => empties.push("body temperature"),
        }
        match self.get_blood_sugar().await {
            MeasurementResult::NoUri => {},
            MeasurementResult::Measurement(m) => pieces.push(m),
            MeasurementResult::FetchFailed => failures.push("blood sugar"),
            MeasurementResult::NoMeasurement => empties.push("blood sugar"),
        }

        if failures.len() > 0 {
            pieces.push(format!("failed to obtain {}", phrase_join(&failures, ", ", " and ")));
        }
        if empties.len() > 0 {
            pieces.push(format!("no recent measurements of {}", phrase_join(&empties, ", ", " or ")));
        }

        if pieces.len() == 0 {
            None
        } else {
            Some(pieces.join("\n"))
        }
    }
}


mod serde_date_string {
    use chrono::{DateTime, Local};
    use serde::{Deserialize, Deserializer, Serializer};

    const DATE_FORMAT: &'static str = "%Y-%m-%d %H:%M:%S %z";

    pub fn serialize<S: Serializer>(timestamp: &DateTime<Local>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(timestamp.format(DATE_FORMAT).to_string().as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Local>, D::Error> {
        let string = String::deserialize(deserializer)?;
        DateTime::parse_from_str(&string, DATE_FORMAT)
            .map(|dtfo| dtfo.with_timezone(&Local))
            .map_err(|e| serde::de::Error::custom(e))
    }
}


mod serde_i64_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(value.to_string().as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        let string = String::deserialize(deserializer)?;
        string.parse()
            .map_err(|e| serde::de::Error::custom(e))
    }
}


mod serde_rational {
    use num_rational::Rational64;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(ratio: &Rational64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(ratio.to_string().as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Rational64, D::Error> {
        let string = String::deserialize(deserializer)?;
        string.parse()
            .map_err(|e| serde::de::Error::custom(e))
    }
}


mod serde_opt_rational {
    use num_rational::Rational64;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(ratio: &Option<Rational64>, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(r) = ratio {
            serializer.serialize_some(r.to_string().as_str())
        } else {
            serializer.serialize_none()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Rational64>, D::Error> {
        let string_opt: Option<String> = Option::deserialize(deserializer)?;
        match string_opt {
            Some(string) => {
                let rat = string.parse()
                    .map_err(|e| serde::de::Error::custom(e))?;
                Ok(Some(rat))
            },
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::phrase_join;

    fn do_phrase_join_test(expected: &str, pieces: &[&str]) {
        assert_eq!(expected, phrase_join(pieces, ", ", " and "));
    }

    #[test]
    fn test_phrase_join() {
        do_phrase_join_test("", &[]);
        do_phrase_join_test("one", &["one"]);
        do_phrase_join_test("one and two", &["one", "two"]);
        do_phrase_join_test("one, two and three", &["one", "two", "three"]);
        do_phrase_join_test("one, two, three and four", &["one", "two", "three", "four"]);
    }
}
