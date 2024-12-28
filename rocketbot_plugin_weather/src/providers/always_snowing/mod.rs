use std::fmt::Write;

use async_trait::async_trait;
use chrono::{Datelike, Local, Weekday};
use rand::{Rng, thread_rng};

use crate::interface::WeatherProvider;


pub(crate) struct AlwaysSnowingProvider;
impl AlwaysSnowingProvider {
    fn generate_believable_temperature_value() -> (i8, u8) {
        // before decimal point: -10..4 (0..14 - 10)
        // after decimal point: 0..10
        let mut rng = thread_rng();
        let before_decimal = rng.gen_range(0..14) - 10;
        let after_decimal = rng.gen_range(0..10);
        (before_decimal, after_decimal)
    }

    fn generate_believable_temperature() -> String {
        let (before_decimal, after_decimal) = Self::generate_believable_temperature_value();
        format!("{}.{}", before_decimal, after_decimal)
    }

    fn generate_believable_humidity() -> String {
        // 76..96
        let mut rng = thread_rng();
        let percent = rng.gen_range(76..96);
        format!("{}", percent)
    }

    fn generate_believable_temperature_range() -> String {
        let (one_before, one_after) = Self::generate_believable_temperature_value();
        let (other_before, other_after) = Self::generate_believable_temperature_value();
        if (one_before, one_after) < (other_before, other_after) {
            format!("{}.{}\u{2013}{}.{}", one_before, one_after, other_before, other_after)
        } else {
            format!("{}.{}\u{2013}{}.{}", other_before, other_after, one_before, one_after)
        }
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
        let general_temp = Self::generate_believable_temperature();
        let general_humidity = Self::generate_believable_humidity();

        let mut today = Local::now().date_naive();

        let mut ret = String::new();
        // claim we are OWM
        write!(ret, "OpenWeatherMap:").unwrap();
        write!(ret, "\nSnow, {} \u{B0}C, {}% humidity", general_temp, general_humidity).unwrap();
        write!(ret, "\nforecast:").unwrap();
        for _ in 0..5 {
            let temp_range = Self::generate_believable_temperature_range();
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
