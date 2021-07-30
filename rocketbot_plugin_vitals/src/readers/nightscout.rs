use async_trait::async_trait;
use bytes::Buf;
use chrono::{DateTime, Local, Utc};
use log::error;
use serde::{Deserialize, Serialize};

use crate::interface::VitalsReader;


const MMOL_PER_L_IN_MG_PER_DL_GLUCOSE: f64 = 0.0555;


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct SensorGlucoseValueEntry {
    #[serde(rename = "id_")]
    pub id: String,

    pub device: String,

    #[serde(rename = "dateString", with = "serde_date_string")]
    pub timestamp: DateTime<Utc>,

    #[serde(rename = "sgv")]
    pub sensor_glucose_value: f64,

    pub delta: f64,

    pub direction: String,

    #[serde(rename = "type")]
    pub value_type: String,

    pub filtered: i64,

    pub unfiltered: i64,

    #[serde(rename = "rssi")]
    pub received_signal_strength_indication: i32,

    pub noise: i32,

    #[serde(rename = "sysTime", with = "serde_date_string")]
    pub system_time: DateTime<Utc>,
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct MeanBloodGlucoseEntry {
    #[serde(rename = "id_")]
    pub id: String,

    pub device: String,

    #[serde(rename = "dateString", with = "serde_date_string")]
    pub timestamp: DateTime<Utc>,

    #[serde(rename = "mbg")]
    pub mean_blood_glucose: f64,

    #[serde(rename = "sysTime", with = "serde_date_string")]
    pub system_time: DateTime<Utc>,
}


pub(crate) struct NightscoutReader {
    sgv_uri: Option<String>,
    mbg_uri: Option<String>,
}
impl NightscoutReader {
    async fn get_sgv(&self) -> Option<String> {
        let sgv_uri = match &self.sgv_uri {
            Some(u) => u.as_str(),
            None => return None,
        };
        let sgv_response = match reqwest::get(sgv_uri).await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching SGV from {:?}: {}", sgv_uri, e);
                return None;
            }
        };
        let sgv_bytes = match sgv_response.bytes().await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching SGV bytes from {:?}: {}", sgv_uri, e);
                return None;
            }
        };
        let mut sgvs: Vec<SensorGlucoseValueEntry> = match serde_json::from_reader(sgv_bytes.reader()) {
            Ok(e) => e,
            Err(e) => {
                error!("error parsing SGV JSON from {:?}: {}", sgv_uri, e);
                return None;
            }
        };

        sgvs.sort_unstable_by_key(|sgv| sgv.timestamp);
        let now = Utc::now();
        let newest = sgvs.iter()
            .filter(|e| e.timestamp <= now)
            .last();

        match newest {
            None => Some("no sensor entry found".to_owned()),
            Some(n) => {
                let local_time = n.timestamp.with_timezone(&Local);
                // FIXME: always mg/dl?
                let mmol_l = n.sensor_glucose_value * MMOL_PER_L_IN_MG_PER_DL_GLUCOSE;
                Some(format!(
                    "sensor: {:.0} mg/dL ({:.2} mmol/L) at {}",
                    n.sensor_glucose_value, mmol_l, local_time.format("%Y-%m-%d %H:%M:%S"),
                ))
            },
        }
    }

    async fn get_mbg(&self) -> Option<String> {
        let mbg_uri = match &self.mbg_uri {
            Some(u) => u.as_str(),
            None => return None,
        };
        let mbg_response = match reqwest::get(mbg_uri).await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching MBG from {:?}: {}", mbg_uri, e);
                return None;
            }
        };
        let mbg_bytes = match mbg_response.bytes().await {
            Ok(r) => r,
            Err(e) => {
                error!("error fetching MBG bytes from {:?}: {}", mbg_uri, e);
                return None;
            }
        };
        let mut mbgs: Vec<MeanBloodGlucoseEntry> = match serde_json::from_reader(mbg_bytes.reader()) {
            Ok(e) => e,
            Err(e) => {
                error!("error parsing MBG JSON from {:?}: {}", mbg_uri, e);
                return None;
            }
        };

        mbgs.sort_unstable_by_key(|mbg| mbg.timestamp);
        let now = Utc::now();
        let newest = mbgs.iter()
            .filter(|e| e.timestamp <= now)
            .last();

        match newest {
            None => Some("no poke entry found".to_owned()),
            Some(n) => {
                let local_time = n.timestamp.with_timezone(&Local);
                // FIXME: always mg/dl?
                let mmol_l = n.mean_blood_glucose * MMOL_PER_L_IN_MG_PER_DL_GLUCOSE;
                Some(format!(
                    "last poke: {:.0} mg/dL ({:.2} mmol/L) at {}",
                    n.mean_blood_glucose, mmol_l, local_time.format("%Y-%m-%d %H:%M:%S"),
                ))
            },
        }
    }
}
#[async_trait]
impl VitalsReader for NightscoutReader {
    async fn new(config: &serde_json::Value) -> Self {
        let sgv_uri = if config["sgv_uri"].is_null() {
            None
        } else {
            Some(config["sgv_uri"].as_str().expect("sgv_uri not a string").to_owned())
        };
        let mbg_uri = if config["mbg_uri"].is_null() {
            None
        } else {
            Some(config["mbg_uri"].as_str().expect("mbg_uri not a string").to_owned())
        };

        Self {
            sgv_uri,
            mbg_uri,
        }
    }

    async fn read(&self) -> Option<String> {
        let mut pieces: Vec<String> = Vec::new();
        if let Some(v) = self.get_sgv().await {
            pieces.push(v);
        }
        if let Some(v) = self.get_mbg().await {
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
    use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
    use serde::{Deserialize, Deserializer, Serializer};

    const DATE_FORMAT: &'static str = "%Y-%m-%dT%H:%M:%S%.3fZ";

    pub fn serialize<S: Serializer>(timestamp: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(timestamp.format(DATE_FORMAT).to_string().as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
        let string = String::deserialize(deserializer)?;
        NaiveDateTime::parse_from_str(&string, DATE_FORMAT)
            .map(|ndt| Utc.from_local_datetime(&ndt).latest().unwrap())
            .map_err(|e| serde::de::Error::custom(e))
    }
}
