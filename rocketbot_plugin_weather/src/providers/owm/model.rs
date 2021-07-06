use serde::{Deserialize, Serialize};


#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Main {
    #[serde(rename = "temp")]
    pub temperature_kelvin: f64,

    #[serde(rename = "pressure")]
    pub pressure_hectopascal: f64,

    #[serde(rename = "humidity")]
    pub humidity_percent: f64,

    #[serde(rename = "temp_min")]
    pub min_temp_kelvin: f64,

    #[serde(rename = "temp_max")]
    pub max_temp_kelvin: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct Weather {
    pub id: u64,
    pub main: String,
    pub description: String,
    pub icon: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct WeatherState {
    #[serde(rename = "weather")]
    pub weathers: Vec<Weather>,

    pub main: Main,

    pub name: Option<String>,

    #[serde(rename = "dt")]
    pub unix_timestamp: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct TemperatureObject {
    #[serde(rename = "max")]
    pub maximum_value_celsius: f64,

    #[serde(rename = "min")]
    pub minimum_value_celsius: f64,

    #[serde(rename = "average")]
    pub average_value_celsius: f64,

    #[serde(rename = "weight")]
    pub weight_value: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct HumidityObject {
    #[serde(rename = "average")]
    pub average_value_percent: f64,

    #[serde(rename = "weight")]
    pub weight_value: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct StationReading {
    #[serde(rename = "type")]
    pub station_type: String,

    #[serde(rename = "date")]
    pub unix_timestamp: i64,

    pub station_id: String,

    #[serde(rename = "temp")]
    pub temperature: TemperatureObject,

    pub humidity: HumidityObject,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Forecast {
    #[serde(rename = "list")]
    pub weather_states: Vec<WeatherState>,
}
