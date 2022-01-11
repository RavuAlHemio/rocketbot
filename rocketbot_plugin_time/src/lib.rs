use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Timelike, Utc};
use chrono_tz::TZ_VARIANTS;
use log::error;
use rocketbot_geocoding::Geocoder;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotPlugin, RocketBotInterface};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


pub struct TimePlugin {
    interface: Weak<dyn RocketBotInterface>,
    geocoder: Geocoder,
    default_location: Option<String>,
}
impl TimePlugin {
    async fn channel_command_time(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut location = command.rest.trim();
        if location.len() == 0 {
            if let Some(dl) = &self.default_location {
                location = dl.as_str();
            } else {
                return;
            }
        }

        // geocode the location
        let loc = match self.geocoder.geocode(location).await {
            Ok(l) => l,
            Err(e) => {
                for (i, provider_error) in e.iter().enumerate() {
                    error!("error geocoding {:?} with provider {}: {}", location, i, provider_error);
                }
                return;
            }
        };

        let timezone_name = match self.geocoder.reverse_geocode_timezone(loc.coordinates).await {
            Ok(tz) => tz,
            Err(e) => {
                for (i, provider_error) in e.iter().enumerate() {
                    error!("error reverse-geocoding timezone for {:?} with provider {}: {}", loc.coordinates, i, provider_error);
                }
                return;
            },
        };

        // find the timezone
        let timezone_opt = TZ_VARIANTS.iter()
            .filter(|tz| tz.name() == timezone_name)
            .nth(0);
        let timezone = match timezone_opt {
            Some(tz) => tz,
            None => {
                error!("no timezone {:?} found", timezone_name);
                return;
            },
        };

        let night_owl_time =
            command.flags.contains("n") || command.flags.contains("not");

        // calculate the time
        let time = Utc::now().with_timezone(timezone);
        let (y, m, d, h, min, s) = if night_owl_time && time.hour() < 4 {
            // previous day, later hour
            let prev_day = time.date().pred();

            (
                prev_day.year(),
                prev_day.month(),
                prev_day.day(),
                time.hour() + 24,
                time.minute(),
                time.second(),
            )
        } else {
            (time.year(), time.month(), time.day(), time.hour(), time.minute(), time.second())
        };

        // custom handling of negative years to ensure we always have four digits
        // (otherwise {:04} prints a minus and three digits)
        let (minus, abs_y) = if y < 0 {
            ("-", -y)
        } else {
            ("", y)
        };
        let response = format!(
            "The time in {} is {}{:04}-{:02}-{:02} {:02}:{:02}:{:02}.",
            loc.place,
            minus, abs_y, m, d, h, min, s,
        );

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for TimePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let default_location = if config["default_location"].is_null() {
            None
        } else {
            Some(config["default_location"].as_str().expect("default_location not a string").to_owned())
        };

        let geocoder = Geocoder::new(&config["geocoding"]).await;
        if !geocoder.supports_timezones().await {
            panic!("the configured geocoding provider does not support timezones");
        }

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "time".to_owned(),
                "time".to_owned(),
                "{cpfx}time [-r] LOCATION".to_owned(),
                "Shows the current time at the given location.".to_owned(),
            )
                .add_flag("not")
                .add_flag("n")
                .build()
        ).await;

        Self {
            interface,
            geocoder,
            default_location,
        }
    }

    async fn plugin_name(&self) -> String {
        "time".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "time" {
            self.channel_command_time(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "time" {
            Some(include_str!("../help/time.md").to_owned())
        } else {
            None
        }
    }
}
