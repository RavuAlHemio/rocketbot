pub mod interface;
pub mod providers;


use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use log::{error, warn};
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_geocoding::{Geocoder, GeoCoordinates};
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;

use crate::interface::WeatherProvider;


static LAT_LON_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    concat!(
        "^",
        "\\s*",
        "(?P<Latitude>[0-9]+(?:[.][0-9]*)?)",
        ",",
        "\\s*",
        "(?P<Longitude>[0-9]+(?:[.][0-9]*)?)",
        "\\s*",
        "$",
    )
).expect("regex parsed successfully"));


struct Config {
    default_location: String,
    location_aliases: HashMap<String, String>,
    providers: Vec<Box<dyn WeatherProvider>>,
    geocoder: Geocoder,
}


pub struct WeatherPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl WeatherPlugin {
    async fn handle_weather_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let show_loc_name = command.name == "weather" || command.name == "wetter";

        let config_guard = self.config.read().await;

        let mut location: &str = &command.rest;
        if location.len() == 0 {
            location = &config_guard.default_location;
        }

        // lookup alias
        if let Some(loc) = config_guard.location_aliases.get(location) {
            location = loc;
        }

        // try specials first
        let mut special_handled = false;
        for provider in &config_guard.providers {
            if let Some(weather) = provider.get_weather_description_for_special(location).await {
                self.output_weather(&channel_message, None, &weather).await;
                special_handled = true;
            }
        }
        if special_handled {
            return;
        }

        // geocode
        let (latitude, longitude, loc_name) = if let Some(caps) = LAT_LON_REGEX.captures(location) {
            let latitude: f64 = caps
                .name("Latitude").expect("matching latitude failed")
                .as_str()
                .parse().expect("parsing latitude failed");
            let longitude: f64 = caps
                .name("Longitude").expect("matching longitude failed")
                .as_str()
                .parse().expect("parsing longitude failed");
            let loc_name = if show_loc_name {
                config_guard.geocoder.reverse_geocode(GeoCoordinates::new(latitude, longitude)).await
                    .ok()
            } else {
                None
            };
            (latitude, longitude, loc_name)
        } else {
            // find the location using a different geocoder (Wunderground's geocoding is really bad)
            let loc = match config_guard.geocoder.geocode(&location).await {
                Err(errors) => {
                    for e in errors {
                        warn!("geocoding error: {}", e);
                    }
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!(
                            "@{} Cannot find that location!",
                            channel_message.message.sender.username,
                        ),
                    ).await;
                    return;
                },
                Ok(l) => l,
            };

            (loc.coordinates.latitude_deg, loc.coordinates.longitude_deg, Some(loc.place))
        };

        for provider in &config_guard.providers {
            let weather = provider
                .get_weather_description_for_coordinates(latitude, longitude).await;
            self.output_weather(
                channel_message,
                if show_loc_name { loc_name.as_deref() } else { None },
                &weather,
            ).await
        }
    }

    async fn output_weather(&self, channel_message: &ChannelMessage, location: Option<&str>, weather: &str) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if let Some(loc) = location {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("@{} {}: {}", channel_message.message.sender.username, loc, weather),
            ).await;
        } else {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("@{} {}", channel_message.message.sender.username, weather),
            ).await;
        }
    }

    async fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let default_location = config["default_location"]
            .as_str().ok_or("default_location is missing or not a string")?
            .to_owned();

        let location_alias_entries = config["location_aliases"]
            .entries().ok_or("location_aliases is not an object")?;
        let mut location_aliases: HashMap<String, String> = HashMap::new();
        for (k, v) in location_alias_entries {
            let key = k.to_owned();
            let value = v
                .as_str().ok_or("location alias is not a string")?
                .to_owned();
            location_aliases.insert(key, value);
        }

        let mut providers = Vec::new();
        for provider_entry in config["providers"].members().ok_or("providers is not a list")? {
            let name = provider_entry["name"]
                .as_str().ok_or("provider name missing or not representable as a string")?;
            let provider_config = provider_entry["config"].clone();

            let provider: Box<dyn WeatherProvider> = if name == "owm" {
                Box::new(crate::providers::owm::OpenWeatherMapProvider::new(provider_config).await)
            } else {
                error!("unknown weather provider {:?}", name);
                return Err("unknown weather provider");
            };
            providers.push(provider);
        }

        let geocoder = Geocoder::new(&config["geocoding"]).await?;

        Ok(Config {
            default_location,
            location_aliases,
            providers,
            geocoder,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for WeatherPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        // register commands
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config).await
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "WeatherPlugin::config",
            config_object,
        );

        let weather_command = CommandDefinitionBuilder::new(
            "weather",
            "weather",
            "{cpfx}weather|{cpfx}lweather [LOCATION]",
            "Displays the current weather as well as a forecast for the given location.",
        )
            .build();
        let lweather_command = weather_command.copy_named("lweather");
        let wetter_command = weather_command.copy_named("wetter");
        let owetter_command = weather_command.copy_named("owetter");
        my_interface.register_channel_command(&weather_command).await;
        my_interface.register_channel_command(&lweather_command).await;
        my_interface.register_channel_command(&wetter_command).await;
        my_interface.register_channel_command(&owetter_command).await;

        WeatherPlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "weather".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "weather" || command.name == "lweather" || command.name == "wetter" || command.name == "owetter" {
            self.handle_weather_command(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "weather" || command_name == "lweather" || command_name == "wetter" || command_name == "owetter" {
            Some(include_str!("../help/weather.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config).await {
            Ok(c) => {
                let mut config_guard = self.config.write().await;
                *config_guard = c;
                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
        }
    }
}
