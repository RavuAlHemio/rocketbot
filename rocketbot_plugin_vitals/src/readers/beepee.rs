use async_trait::async_trait;
use bytes::Buf;
use chrono::{DateTime, Local, Utc};
use log::error;
use num_rational::Rational64;
use serde::{Deserialize, Serialize};

use crate::interface::VitalsReader;


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct BloodPressureEntry {
    pub id: i64,

    #[serde(rename = "zoned_timestamp", with = "serde_date_string")]
    pub timestamp: DateTime<Local>,

    pub systolic: i32,

    pub diastolic: i32,

    pub pulse: i32,

    pub spo2: Option<i32>,
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct BodyMassEntry {
    pub id: i64,

    #[serde(rename = "zoned_timestamp", with = "serde_date_string")]
    pub timestamp: DateTime<Local>,

    #[serde(with = "serde_rational")]
    pub mass: Rational64,
}


pub(crate) struct BeepeeReader {
    bp_uri: Option<String>,
    mass_uri: Option<String>,
}
impl BeepeeReader {
    async fn get_bp(&self) -> Option<String> {
        let bp_uri = match &self.bp_uri {
            Some(u) => u.as_str(),
            None => return None,
        };
        let bp_response = match reqwest::get(bp_uri).await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching blood pressure from {:?}: {}", bp_uri, e);
                return None;
            }
        };
        let bp_bytes = match bp_response.bytes().await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching blood pressure bytes from {:?}: {}", bp_uri, e);
                return None;
            }
        };
        let mut bps: Vec<BloodPressureEntry> = match serde_json::from_reader(bp_bytes.reader()) {
            Ok(e) => e,
            Err(e) => {
                error!("error parsing blood pressure JSON from {:?}: {}", bp_uri, e);
                return None;
            }
        };

        bps.sort_unstable_by_key(|bp| bp.timestamp);
        let now = Utc::now();
        let newest = bps.iter()
            .filter(|e| e.timestamp <= now)
            .last();

        match newest {
            None => Some("no blood pressure measurement found".to_owned()),
            Some(n) => {
                let spo2_piece = if let Some(spo2) = n.spo2 {
                    format!(" with {}% SpO\u{2082}", spo2)
                } else {
                    String::new()
                };
                Some(format!(
                    "{}/{} mmHg at {} bpm{} at {}",
                    n.systolic, n.diastolic, n.pulse, spo2_piece, n.timestamp.format("%Y-%m-%d %H:%M:%S"),
                ))
            },
        }
    }

    async fn get_mass(&self) -> Option<String> {
        let mass_uri = match &self.mass_uri {
            Some(u) => u.as_str(),
            None => return None,
        };
        let mass_response = match reqwest::get(mass_uri).await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching body mass from {:?}: {}", mass_uri, e);
                return None;
            }
        };
        let mass_bytes = match mass_response.bytes().await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching body mass bytes from {:?}: {}", mass_uri, e);
                return None;
            }
        };
        let mut masses: Vec<BodyMassEntry> = match serde_json::from_reader(mass_bytes.reader()) {
            Ok(e) => e,
            Err(e) => {
                error!("error parsing body mass JSON from {:?}: {}", mass_uri, e);
                return None;
            }
        };

        masses.sort_unstable_by_key(|mass| mass.timestamp);
        let now = Utc::now();
        let newest = masses.iter()
            .filter(|e| e.timestamp <= now)
            .last();

        match newest {
            None => Some("no mass measurement found".to_owned()),
            Some(n) => {
                let kg: f64 = (*n.mass.numer() as f64) / (*n.mass.denom() as f64);
                Some(format!(
                    "{:.02} kg at {}",
                    kg, n.timestamp.format("%Y-%m-%d %H:%M:%S"),
                ))
            },
        }
    }
}
#[async_trait]
impl VitalsReader for BeepeeReader {
    async fn new(config: &serde_json::Value) -> Self {
        let bp_uri = if config["bp_uri"].is_null() {
            None
        } else {
            Some(config["bp_uri"].as_str().expect("bp_uri not a string").to_owned())
        };
        let mass_uri = if config["mass_uri"].is_null() {
            None
        } else {
            Some(config["mass_uri"].as_str().expect("mass_uri not a string").to_owned())
        };

        Self {
            bp_uri,
            mass_uri,
        }
    }

    async fn read(&self) -> Option<String> {
        let mut pieces: Vec<String> = Vec::new();
        if let Some(v) = self.get_bp().await {
            pieces.push(v);
        }
        if let Some(v) = self.get_mass().await {
            pieces.push(v);
        }

        if pieces.len() == 0 {
            None
        } else {
            Some(pieces.join("; "))
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
