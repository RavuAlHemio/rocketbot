use std::io::Cursor;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::TimeDelta;
use http_body_util::BodyExt;
use hyper::StatusCode;
use regex::Regex;
use rocketbot_bim_common::CouplingMode;
use rocketbot_bim_common::ride_table::RideTableData;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::error;


#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    pub bot_username: String,
    pub my_username: String,

    #[serde(with = "rocketbot_interface::serde::serde_regex")]
    pub attachment_name_regex: Regex,

    #[serde(default = "Config::default_first_ride_emoji")]
    pub first_ride_emoji: Option<String>,

    #[serde(default = "Config::default_vehicle_taken_by_me_emoji")]
    pub vehicle_taken_by_me_emoji: Option<String>,

    #[serde(default = "Config::default_vehicle_taken_from_me_emoji")]
    pub vehicle_taken_from_me_emoji: Option<String>,

    #[serde(default = "Config::default_vehicle_taken_by_other_emoji")]
    pub vehicle_taken_by_other_emoji: Option<String>,

    #[serde(default = "Config::default_vehicle_remains_emoji")]
    pub vehicle_remains_emoji: Option<String>,

    #[serde(default = "Config::default_vehicle_remains_recently_emoji")]
    pub vehicle_remains_recently_emoji: Option<String>,
}
impl Config {
    fn default_first_ride_emoji() -> Option<String> { Some("tada".to_owned()) }
    fn default_vehicle_taken_by_me_emoji() -> Option<String> { Some("slight_smile".to_owned()) }
    fn default_vehicle_taken_from_me_emoji() -> Option<String> { Some("angry".to_owned()) }
    fn default_vehicle_taken_by_other_emoji() -> Option<String> { Some("thumbsup".to_owned()) }
    fn default_vehicle_remains_emoji() -> Option<String> { Some("recycle".to_owned()) }
    fn default_vehicle_remains_recently_emoji() -> Option<String> { Some("repeat_one".to_owned()) }
}


pub struct BimReactPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
#[async_trait]
impl RocketBotPlugin for BimReactPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config_value: serde_json::Value) -> Self {
        let config_object = match Self::load_config(config_value) {
            Some(co) => co,
            None => {
                panic!("failed to load configuration");
            },
        };
        let config = RwLock::new(
            "BimReactPlugin::config",
            config_object,
        );

        Self {
            interface,
            config,
        }
    }

    async fn plugin_name(&self) -> String { "bim_react".to_owned() }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            Some(i) => i,
            None => {
                error!("interface is gone");
                return;
            },
        };

        let config = self.config.read().await;

        // the message must be from the bot
        if channel_message.message.sender.username != config.bot_username {
            return;
        }

        // we need a PNG attachment that passes the filename check
        if channel_message.message.attachments.len() != 1 {
            return;
        }
        let attachment = &channel_message.message.attachments[0];
        if attachment.image_mime_type.as_deref() != Some("image/png") {
            return;
        }
        if !config.attachment_name_regex.is_match(&attachment.title) {
            return;
        }

        // download it
        if !attachment.title_link.starts_with("/") {
            return;
        }
        let Ok(download_response) = interface.obtain_http_resource(&attachment.title_link).await else { return };
        let (parts, body) = download_response.into_parts();
        if parts.status != StatusCode::OK {
            error!("obtaining attachment {:?} led to error code {}", attachment.title_link, parts.status);
            return;
        }
        let attachment_bytes = match body.collect().await {
            Ok(b) => b.to_bytes().to_vec(),
            Err(e) => {
                error!("error obtaining bytes from response for attachment {:?}: {}", attachment.title_link, e);
                return;
            },
        };
        let attachment_cursor = Cursor::new(attachment_bytes);

        // try to load the PNG
        let decoder = png::Decoder::new(attachment_cursor);
        let reader = match decoder.read_info() {
            Ok(i) => i,
            Err(e) => {
                error!("failed to decode PNG header for attachment {:?}: {}", attachment.title_link, e);
                return;
            },
        };
        let info = reader.info();
        for itxt_chunk in &info.utf8_text {
            if itxt_chunk.keyword != "bimride" {
                continue;
            }
            let bimride_text = match itxt_chunk.get_text() {
                Ok(bt) => bt,
                Err(e) => {
                    error!("failed to decode bimride text for attachment {:?}: {}", attachment.title_link, e);
                    return;
                },
            };

            // attempt to deserialize as JSON-encoded RideTableData
            let bimride: RideTableData = match serde_json::from_str(&bimride_text) {
                Ok(br) => br,
                Err(e) => {
                    error!("failed to deserialize bimride JSON {:?} for attachment {:?}: {}", bimride_text, attachment.title_link, e);
                    return;
                },
            };

            // decide with which emoji to respond
            let ridden_vehicles = bimride.vehicles.iter().filter(|v| v.coupling_mode == CouplingMode::Ridden);
            let mut first_ever_vehicles = 0;
            let mut my_taken_vehicles = 0;
            let mut taken_from_me_vehicles = 0;
            let mut other_taken_vehicles = 0;
            let mut other_recently_same_vehicles = 0;
            let mut other_same_vehicles = 0;
            for vehicle in ridden_vehicles {
                if vehicle.is_first_highlighted_ride_overall() {
                    // first ride ever
                    first_ever_vehicles += 1;
                } else if vehicle.belongs_to_rider_highlighted() {
                    // same rider rides again
                    let is_recent = bimride.relative_time
                        .map(|ride_time| ride_time - vehicle.my_highlighted_last().unwrap().timestamp())
                        .map(|delta| delta < TimeDelta::hours(24))
                        .unwrap_or(false);
                    if is_recent {
                        other_recently_same_vehicles += 1;
                    } else {
                        other_same_vehicles += 1;
                    }
                } else {
                    // vehicle changed hands
                    if bimride.rider_username == config.my_username {
                        // to me!
                        my_taken_vehicles += 1;
                    } else if vehicle.last_highlighted_rider().is_specific_somebody_else(&config.my_username) {
                        taken_from_me_vehicles += 1;
                    } else {
                        other_taken_vehicles += 1;
                    }
                }
            }

            // respond
            let response_emoji_opt = if first_ever_vehicles > 0 {
                config.first_ride_emoji.as_deref()
            } else if my_taken_vehicles > 0 {
                config.vehicle_taken_by_me_emoji.as_deref()
            } else if taken_from_me_vehicles > 0 {
                config.vehicle_taken_from_me_emoji.as_deref()
            } else if other_taken_vehicles > 0 {
                config.vehicle_taken_by_other_emoji.as_deref()
            } else if other_recently_same_vehicles > 0 {
                config.vehicle_remains_recently_emoji.as_deref()
            } else if other_same_vehicles > 0 {
                config.vehicle_remains_emoji.as_deref()
            } else {
                None
            };
            if let Some(emoji) = response_emoji_opt {
                interface.add_reaction(
                    &channel_message.message.id,
                    emoji,
                ).await;
            }
        }
    }
}
impl BimReactPlugin {
    fn load_config(config: serde_json::Value) -> Option<Config> {
        match serde_json::from_value(config) {
            Ok(c) => Some(c),
            Err(e) => {
                error!("failed to load config: {}", e);
                None
            },
        }
    }
}
