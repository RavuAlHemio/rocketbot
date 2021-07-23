mod model;


use std::collections::{BTreeSet, BTreeMap};

use async_trait::async_trait;
use bytes::Buf;
use chrono::{Date, Datelike, DateTime, Duration, TimeZone, Utc, Weekday};
use log::{debug, error};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::sync::Mutex;
use serde::de::DeserializeOwned;
use serde_json;

use crate::interface::{WeatherError, WeatherProvider};
use crate::providers::owm::model::{Forecast, StationReading, WeatherState};


static WEATHER_STATION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^owm:ws:(?P<id>[0-9a-f]+)$"
).expect("failed to compile regex"));
const ERROR_TEXT: &'static str = "An error occurred.";


fn kelvin_to_celsius(kelvin: f64) -> f64 {
    kelvin - 273.15
}

const fn weekday_to_short(wd: Weekday) -> &'static str {
    match wd {
        Weekday::Mon => "Mo",
        Weekday::Tue => "Tu",
        Weekday::Wed => "We",
        Weekday::Thu => "Th",
        Weekday::Fri => "Fr",
        Weekday::Sat => "Sa",
        Weekday::Sun => "Su",
    }
}

fn time_tuple(value: i64, singular: &str, plural: &str) -> (i64, String) {
    (value, format!("{} {}", value, if value == 1 { singular } else { plural }))
}

fn format_duration(mut duration: Duration) -> String {
    let mut ago = false;
    if duration < Duration::zero() {
        duration = -duration;
        ago = true;
    }

    if duration < Duration::seconds(1) {
        return "now".into();
    }

    let mut o_tempora_o_mores: Vec<(i64, String)> = vec![
        time_tuple(duration.num_days(), "day", "days"),
        time_tuple(duration.num_hours() % 24, "hour", "hours"),
        time_tuple(duration.num_minutes() % 60, "minute", "minutes"),
        time_tuple(duration.num_seconds() % 60, "second", "seconds"),
    ];

    // remove the empty large units
    while o_tempora_o_mores.len() > 0 && o_tempora_o_mores[0].0 == 0 {
        o_tempora_o_mores.remove(0);
    }

    // show two consecutive units at most
    while o_tempora_o_mores.len() > 2 {
        o_tempora_o_mores.remove(o_tempora_o_mores.len() - 1);
    }

    // delete the second unit if it is zero
    if o_tempora_o_mores.len() > 1 && o_tempora_o_mores[0].0 == 0 {
        o_tempora_o_mores.remove(1);
    }

    // fun!
    let joint_vec: Vec<String> = o_tempora_o_mores
        .iter()
        .map(|otom| otom.1.clone())
        .collect();
    let joint = joint_vec.join(" ");

    if ago {
        format!("{} ago", joint)
    } else {
        format!("in {}", joint)
    }
}


#[derive(Clone, Debug, PartialEq)]
struct ForecastSummary {
    pub min_temp_kelvin: f64,
    pub max_temp_kelvin: f64,
    pub weather_states: Vec<String>,
}
impl ForecastSummary {
    fn new(
        min_temp_kelvin: f64,
        max_temp_kelvin: f64,
        weather_states: Vec<String>,
    ) -> ForecastSummary {
        ForecastSummary {
            min_temp_kelvin,
            max_temp_kelvin,
            weather_states,
        }
    }

    fn summarize_forecast(forecast: &Forecast) -> BTreeMap<Date<Utc>, ForecastSummary> {
        let mut ret: BTreeMap<Date<Utc>, ForecastSummary> = BTreeMap::new();
        for weather_state in &forecast.weather_states {
            let timestamp = Utc.timestamp(weather_state.unix_timestamp, 0);
            let date = timestamp.date();

            let this_max_kelvin = weather_state.main.max_temp_kelvin;
            let this_min_kelvin = weather_state.main.min_temp_kelvin;

            let mut weather_states: Vec<String> = Vec::new();
            for weather in &weather_state.weathers {
                weather_states.push(weather.main.clone());
            }

            ret.entry(date)
                .and_modify(|e| {
                    e.max_temp_kelvin = e.max_temp_kelvin.max(this_max_kelvin);
                    e.min_temp_kelvin = e.min_temp_kelvin.min(this_min_kelvin);

                    for weather_state in weather_states.drain(..) {
                        if !e.weather_states.contains(&weather_state) {
                            e.weather_states.push(weather_state);
                        }
                    }
                })
                .or_insert_with(|| {
                    ForecastSummary::new(
                        this_min_kelvin,
                        this_max_kelvin,
                        weather_states,
                    )
                });

        }
        ret
    }
}


pub(crate) struct OpenWeatherMapProvider {
    api_key: String,
    max_calls_per_minute: Option<usize>,
    weather_station_look_back_minutes: i64,
    last_queries: Mutex<BTreeSet<DateTime<Utc>>>,
    http_client: Mutex<reqwest::Client>,
}
impl OpenWeatherMapProvider {
    async fn check_cooldown_enough(&self, required_count: usize) -> bool {
        if let Some(mcpm) = self.max_calls_per_minute {
            let now = Utc::now();
            let minute_ago = now - Duration::minutes(1);

            let mut last_guard = self.last_queries
                .lock().await;
            last_guard.retain(|call_time| call_time >= &minute_ago);

            last_guard.len() + required_count <= mcpm
        } else {
            true
        }
    }

    async fn register_for_cooldown(&self) {
        if let Some(_) = self.max_calls_per_minute {
            let mut last_guard = self.last_queries
                .lock().await;
            last_guard.insert(Utc::now());
        }
    }

    async fn get_and_populate_json<T: DeserializeOwned>(&self, uri: &str) -> Result<T, WeatherError> {
        debug!("obtaining weather data from {}", uri);

        let client_guard = self.http_client
            .lock().await;
        let response = client_guard
            .get(uri)
            .send().await.map_err(|e| WeatherError::new(format!("failed to perform request to {}: {}", uri, e)))?;
        self.register_for_cooldown().await;
        if response.status() != reqwest::StatusCode::OK {
            return Err(WeatherError::new(format!("request to {} failed with status {}", uri, response.status())));
        }

        let bytes = response
            .bytes().await.map_err(|e| WeatherError::new(format!("failed to obtain bytes of request to {}: {}", uri, e)))?;
        let bytes_reader = bytes.reader();
        let deserialized: T = serde_json::from_reader(bytes_reader)
            .map_err(|e| WeatherError::new(format!("failed to parse JSON from result of request to {}: {}", uri, e)))?;

        Ok(deserialized)
    }

    async fn get_weather_description_for_weather_station(&self, weather_station_id: &str) -> String {
        let now_time = Utc::now().timestamp();
        let lookback_time = now_time - (self.weather_station_look_back_minutes * 60);

        let weather_uri = format!(
            "https://api.openweathermap.org/data/3.0/measurements?station_id={}&type=m&limit=10&from={}&to={}&appid={}",
            weather_station_id, lookback_time, now_time, self.api_key,
        );
        let mut readings: Vec<StationReading> = match self.get_and_populate_json(&weather_uri).await {
            Ok(cw) => cw,
            Err(e) => {
                error!("error obtaining weather station readings: {}", e);
                return ERROR_TEXT.to_owned();
            },
        };

        if readings.len() == 0 {
            return "OpenWeatherMap returned no readings for this weather station!".into();
        }

        let mut ret = String::new();
        readings.sort_unstable_by_key(|sr| -sr.unix_timestamp);
        let newest_reading: &StationReading = &readings[0];

        // current temperature
        ret.push_str(&format!(
            "{:.1} \u{B0}C",
            newest_reading.temperature.average_value_celsius,
        ));

        // current humidity
        if ret.len() > 0 {
            ret.push_str(", ");
        }
        ret.push_str(&format!(
            "{:.0}% humidity",
            newest_reading.humidity.average_value_percent,
        ));

        // append time info
        let time_diff = Utc.timestamp(newest_reading.unix_timestamp, 0) - Utc::now();
        ret.push_str(&format!(" ({})", format_duration(time_diff)));

        format!("OpenWeatherMap: {}", ret)
    }
}
#[async_trait]
impl WeatherProvider for OpenWeatherMapProvider {
    async fn new(config: serde_json::Value) -> Self {
        let api_key = config["api_key"]
            .as_str().expect("api_key is either missing or not a string")
            .to_owned();
        let max_calls_per_minute = if config["max_calls_per_minute"].is_null() {
            None
        } else {
            Some(
                config["max_calls_per_minute"]
                    .as_usize().expect("max_calls_per_minute is either missing or not representable as usize")
            )
        };
        let weather_station_look_back_minutes = if config["weather_station_look_back_minutes"].is_null() {
            8*60
        } else {
            config["weather_station_look_back_minutes"]
                .as_i64().expect("weather_station_look_back_minutes is not representable as usize")
        };
        let last_queries = Mutex::new(
            "OpenWeatherMapProvider::last_queries",
            BTreeSet::new(),
        );
        let http_client = Mutex::new(
            "OpenWeatherMapProvider::http_client",
            reqwest::Client::new(),
        );

        OpenWeatherMapProvider {
            api_key,
            max_calls_per_minute,
            weather_station_look_back_minutes,
            last_queries,
            http_client,
        }
    }

    async fn get_weather_description_for_special(&self, special_string: &str) -> Option<String> {
        if let Some(caps) = WEATHER_STATION_REGEX.captures(special_string) {
            let ws_id = caps
                .name("id").expect("weather station ID missing")
                .as_str();
            Some(self.get_weather_description_for_weather_station(ws_id).await)
        } else {
            None
        }
    }

    async fn get_weather_description_for_coordinates(&self, latitude_deg_north: f64, longitude_deg_east: f64) -> String {
        if !self.check_cooldown_enough(2).await {
            return "OpenWeatherMap is on cooldown. :(".into();
        }

        let weather_uri = format!(
            "https://api.openweathermap.org/data/2.5/weather?lat={}&lon={}&appid={}",
            latitude_deg_north, longitude_deg_east, self.api_key,
        );
        let current_weather: WeatherState = match self.get_and_populate_json(&weather_uri).await {
            Ok(cw) => cw,
            Err(e) => {
                error!("failed to obtain weather for lat={} lon={}: {}", latitude_deg_north, longitude_deg_east, e);
                return ERROR_TEXT.to_owned();
            },
        };

        let forecast_uri = format!(
            "https://api.openweathermap.org/data/2.5/forecast?lat={}&lon={}&appid={}",
            latitude_deg_north, longitude_deg_east, self.api_key,
        );
        let forecast: Forecast = match self.get_and_populate_json(&forecast_uri).await {
            Ok(f) => f,
            Err(e) => {
                error!("failed to obtain forecast for lat={} lon={}: {}", latitude_deg_north, longitude_deg_east, e);
                return ERROR_TEXT.to_owned();
            },
        };

        let mut ret = String::new();

        // weather status
        if let Some(first_weather) = current_weather.weathers.first() {
            ret.push_str(&first_weather.main);
        }

        // current temperature
        if ret.len() > 0 {
            ret.push_str(", ");
        }
        ret.push_str(&format!(
            "{:.1} \u{B0}C", kelvin_to_celsius(current_weather.main.temperature_kelvin),
        ));

        // current humidity
        if ret.len() > 0 {
            ret.push_str(", ");
        }
        ret.push_str(&format!(
            "{:.0}% humidity", current_weather.main.humidity_percent,
        ));

        if forecast.weather_states.len() > 0 {
            if ret.len() > 0 {
                ret.push_str("\n");
            }
            ret.push_str("forecast:\n");

            let summarized = ForecastSummary::summarize_forecast(&forecast);
            let forecast_list: Vec<String> = summarized
                .iter()
                .map(|(d, fs)| format!(
                    "*{}* {}.{:02}. {} {:.1}\u{2013}{:.1} \u{B0}C",
                    weekday_to_short(d.weekday()),
                    d.day(),
                    d.month(),
                    fs.weather_states.join("/"),
                    kelvin_to_celsius(fs.min_temp_kelvin),
                    kelvin_to_celsius(fs.max_temp_kelvin),
                ))
                .collect();
            let forecast_string = forecast_list.join("\n");
            ret.push_str(&forecast_string);
        }

        format!("OpenWeatherMap:\n{}", ret)
    }
}
