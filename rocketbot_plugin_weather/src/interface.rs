use std::fmt;

use async_trait::async_trait;
use json::JsonValue;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WeatherError {
    message: String,
}
impl WeatherError {
    pub fn new(message: String) -> WeatherError {
        WeatherError { message }
    }

    pub fn new_str(message: &str) -> WeatherError {
        WeatherError::new(message.into())
    }
}
impl fmt::Display for WeatherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for WeatherError {
}


#[async_trait]
pub trait WeatherProvider : Send + Sync {
    async fn new(config: JsonValue) -> Self where Self: Sized;
    async fn get_weather_description_for_special(&self, special_string: &str) -> Option<String>;
    async fn get_weather_description_for_coordinates(&self, latitude_deg_north: f64, longitude_deg_east: f64) -> String;
}
