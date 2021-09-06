use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Local, TimeZone, Utc};
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use serde_json;


pub struct NewYearPlugin {
    interface: Weak<dyn RocketBotInterface>,
    channels: Vec<String>,
}
#[async_trait]
impl RocketBotPlugin for NewYearPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut channels = Vec::new();
        for channel in config["channels"].members_or_empty() {
            let ch = match channel.as_str() {
                Some(c) => c,
                None => continue,
            };
            channels.push(ch.to_owned());
        }

        if channels.len() > 0 {
            let now_local = Local::now();
            let next_year = now_local.year()+1;
            let new_year_local = Local
                .ymd(next_year, 1, 1)
                .and_hms(0, 0, 0);
            let new_year_utc = new_year_local.with_timezone(&Utc);

            let custom_data = serde_json::json!(["new_year", next_year]);
            my_interface.register_timer(new_year_utc, custom_data)
                .await;
        }

        NewYearPlugin {
            interface,
            channels,
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

        for channel in &self.channels {
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
}
