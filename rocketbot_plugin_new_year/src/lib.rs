use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Local, TimeZone, Utc};
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::sync::RwLock;
use serde_json;
use tracing::error;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Config {
    channels: Vec<String>,
}


pub struct NewYearPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl NewYearPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut channels = Vec::new();
        for channel in config["channels"].members_or_empty() {
            let ch = match channel.as_str() {
                Some(c) => c,
                None => continue,
            };
            channels.push(ch.to_owned());
        }

        Ok(Config {
            channels,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for NewYearPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        // register timer in any case; channel list in config might change in the meantime
        let now_local = Local::now();
        let next_year = now_local.year()+1;
        let new_year_local = Local.with_ymd_and_hms(next_year, 1, 1, 0, 0, 0).unwrap();
        let new_year_utc = new_year_local.with_timezone(&Utc);

        let custom_data = serde_json::json!(["new_year", next_year]);
        my_interface.register_timer(new_year_utc, custom_data)
            .await;

        let config_lock = RwLock::new(
            "NewYearPlugin::channels",
            config_object,
        );

        NewYearPlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "new_year".to_owned()
    }

    async fn timer_elapsed(&self, custom_data: &serde_json::Value) {
        if !custom_data.is_array() {
            return;
        }
        if custom_data[0] != "new_year" {
            return;
        }
        let new_year = match custom_data[1].as_i64() {
            Some(y) => y,
            None => return,
        };

        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        for channel in &config_guard.channels {
            send_channel_message!(
                interface,
                channel,
                &format!("</{}>", new_year-1),
            ).await;
            send_channel_message!(
                interface,
                channel,
                &format!("<{}>", new_year),
            ).await;
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
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
