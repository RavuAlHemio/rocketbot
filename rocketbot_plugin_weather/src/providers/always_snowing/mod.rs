mod temperature;


use std::fmt::Write;

use async_trait::async_trait;
use chrono::{Datelike, Local, Weekday};
use rand::RngExt;
use rand::rngs::ThreadRng;

use crate::interface::WeatherProvider;
use crate::providers::always_snowing::temperature::{GeneratedTemperature, TemperatureRange};


pub(crate) struct AlwaysSnowingProvider;
impl AlwaysSnowingProvider {
    fn generate_snowy_continental_humidity() -> String {
        // 76..96
        let mut rng: ThreadRng = rand::rng();
        let percent = rng.random_range(76..96);
        format!("{}", percent)
    }

    fn weekday_name(weekday: Weekday) -> &'static str {
        match weekday {
            Weekday::Mon => "Mo",
            Weekday::Tue => "Tu",
            Weekday::Wed => "We",
            Weekday::Thu => "Th",
            Weekday::Fri => "Fr",
            Weekday::Sat => "Sa",
            Weekday::Sun => "Su",
        }
    }
}
#[async_trait]
impl WeatherProvider for AlwaysSnowingProvider {
    async fn new(_config: serde_json::Value) -> Self {
        Self
    }

    async fn get_weather_description_for_special(&self, _special_string: &str) -> Option<String> {
        None
    }

    async fn get_weather_description_for_coordinates(&self, _latitude_deg_north: f64, _longitude_deg_east: f64) -> String {
        let general_temp = GeneratedTemperature::generate_snowy_continental();
        let general_humidity = Self::generate_snowy_continental_humidity();

        let mut today = Local::now().date_naive();

        let mut ret = String::new();
        // claim we are OWM
        write!(ret, "OpenWeatherMap:").unwrap();
        write!(ret, "\nSnow, {} \u{B0}C, {}% humidity", general_temp, general_humidity).unwrap();
        write!(ret, "\nforecast:").unwrap();
        for day_offset in 0..5 {
            let mut temp_range = TemperatureRange::generate_snowy_continental();

            // if this is today, ensure the current temperature fits into the range
            if day_offset == 0 {
                temp_range.ensure_contains(general_temp);
            }

            write!(
                ret,
                "\n*{}* {}.{:02}. Snow {} \u{B0}C",
                Self::weekday_name(today.weekday()),
                today.day(),
                today.month(),
                temp_range,
            ).unwrap();

            today = match today.succ_opt() {
                Some(t) => t,
                None => break,
            };
        }
        ret
    }
}
