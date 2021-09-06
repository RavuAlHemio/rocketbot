pub mod interface;
pub mod providers;


use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use log::warn;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_geonames::GeoNamesClient;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
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


pub struct WeatherPlugin {
    default_location: String,
    location_aliases: HashMap<String, String>,
    interface: Weak<dyn RocketBotInterface>,
    providers: Vec<Box<dyn WeatherProvider>>,
    geonames_client: GeoNamesClient,
}
impl WeatherPlugin {
    async fn handle_weather_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let show_loc_name = command.name == "weather" || command.name == "wetter";

        let mut location: &str = &command.rest;
        if location.len() == 0 {
            location = &self.default_location;
        }

        // lookup alias
        if let Some(loc) = self.location_aliases.get(location) {
            location = loc;
        }

        // try specials first
        let mut special_handled = false;
        for provider in &self.providers {
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
                self.geonames_client.get_first_reverse_geo(latitude, longitude).await
                    .ok()
            } else {
                None
            };
            (latitude, longitude, loc_name)
        } else {
            // find the location using GeoNames (Wunderground's geocoding is really bad)
            let loc = match self.geonames_client.get_first_geo_name(&location).await {
                Err(e) => {
                    warn!("GeoNames error: {}", e);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!(
                            "@{} GeoNames cannot find that location!",
                            channel_message.message.sender.username,
                        ),
                    ).await;
                    return;
                },
                Ok(l) => l,
            };

            let lat = match loc.latitude() {
                Ok(l) => l,
                Err(_) => return,
            };
            let lon = match loc.longitude() {
                Ok(l) => l,
                Err(_) => return,
            };

            (lat, lon, Some(loc.name_and_country_name()))
        };

        for provider in &self.providers {
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
}
#[async_trait]
impl RocketBotPlugin for WeatherPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        // register commands
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let weather_command = CommandDefinition::new(
            "weather".to_owned(),
            "weather".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}weather|{cpfx}lweather [LOCATION]".to_owned(),
            "Displays the current weather as well as a forecast for the given location.".to_owned(),
        );
        let lweather_command = weather_command.copy_named("lweather");
        let wetter_command = weather_command.copy_named("wetter");
        let owetter_command = weather_command.copy_named("owetter");
        my_interface.register_channel_command(&weather_command).await;
        my_interface.register_channel_command(&lweather_command).await;
        my_interface.register_channel_command(&wetter_command).await;
        my_interface.register_channel_command(&owetter_command).await;

        let default_location = config["default_location"]
            .as_str().expect("default_location is missing or not a string")
            .to_owned();

        let location_aliases: HashMap<String, String> = config["location_aliases"].entries()
            .expect("location_aliases is not an object")
            .map(|(k, v)| {
                let key = k.to_owned();
                let value = v.as_str().expect("location alias is not a string")
                    .to_owned();
                (key, value)
            })
            .collect();

        let mut providers = Vec::new();
        for provider_entry in config["providers"].members().expect("providers is not a list") {
            let name = provider_entry["name"]
                .as_str().expect("provider name missing or not representable as a string");
            let provider_config = provider_entry["config"].clone();

            let provider: Box<dyn WeatherProvider> = if name == "owm" {
                Box::new(crate::providers::owm::OpenWeatherMapProvider::new(provider_config).await)
            } else {
                panic!("unknown weather provider {:?}", name);
            };
            providers.push(provider);
        }

        let geonames_client = GeoNamesClient::new(&config["geonames"]);

        WeatherPlugin {
            default_location,
            location_aliases,
            interface,
            providers,
            geonames_client,
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
}
