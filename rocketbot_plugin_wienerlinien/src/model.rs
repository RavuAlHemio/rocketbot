use serde::{Serialize, Deserialize};


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct StoppingPoint {
    #[serde(rename = "StopID")] pub stop_id: u32,
    #[serde(rename = "DIVA")] pub diva_id: Option<u32>,
    #[serde(rename = "StopText")] pub name: String,
    #[serde(rename = "Latitude")] pub latitude: Option<f64>,
    #[serde(rename = "Longitude")] pub longitude: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct MonitorWrapper {
    pub data: MonitorData,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct MonitorData {
    pub monitors: Vec<Monitor>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Monitor {
    #[serde(rename = "locationStop")] pub location_stop: LocationStop,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct LocationStop {
    pub properties: StopProperties,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct StopProperties {
    pub attributes: StopAttributes,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct StopAttributes {
    pub rbl: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Line {
    pub name: String,
    pub towards: String,
    #[serde(rename = "barrierFree")] pub barrier_free: bool,
    #[serde(rename = "realtimeSupported")] pub realtime_supported: bool,
    #[serde(rename = "trafficjam")] pub traffic_jam: bool,
    #[serde(rename = "departures")] pub departure_data: DepartureData,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct DepartureData {
    #[serde(rename = "departure")] pub departures: Vec<Departure>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Departure {
    #[serde(rename = "departureTime")] pub departure_time: DepartureTime,
    #[serde(rename = "vehicle")] pub vehicle: Option<Vehicle>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct DepartureTime {
    #[serde(rename = "timePlanned")] pub time_planned: String,
    #[serde(rename = "timeReal")] pub time_real: Option<String>,
    #[serde(rename = "countdown")] pub countdown: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Vehicle {
    pub name: String,
    pub towards: String,
    #[serde(rename = "barrierFree")] pub barrier_free: bool,
    #[serde(rename = "realtimeSupported")] pub realtime_supported: bool,
    #[serde(rename = "trafficjam")] pub traffic_jam: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct DepartureLine {
    pub line_name: String,
    pub target_station: String,
    pub departures: Vec<DepartureTimeEntry>,
}
impl DepartureLine {
    pub fn new(
        line_name: String,
        target_station: String,
        departures: Vec<DepartureTimeEntry>,
    ) -> Self {
        Self {
            line_name,
            target_station,
            departures,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct DepartureTimeEntry {
    pub countdown: u64,
    pub accessible: bool,
    pub realtime: bool,
    pub traffic_jam: bool,
}
impl DepartureTimeEntry {
    pub fn new(
        countdown: u64,
        accessible: bool,
        realtime: bool,
        traffic_jam: bool,
    ) -> Self {
        Self {
            countdown,
            accessible,
            realtime,
            traffic_jam,
        }
    }
}
