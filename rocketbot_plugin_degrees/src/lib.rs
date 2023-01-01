mod slice_image;


use std::io::Cursor;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use regex::Regex;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachmentBuilder};
use serde_json;
use tokio::sync::RwLock;

use crate::slice_image::SliceImage;


pub struct DegreesPlugin {
    interface: Weak<dyn RocketBotInterface>,
    matcher_regex: RwLock<Regex>,
}
#[async_trait]
impl RocketBotPlugin for DegreesPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let matcher_regex_str = config["matcher_regex"].as_str()
            .expect("matcher_regex is not a string");
        let matcher_regex = RwLock::new(Regex::new(matcher_regex_str)
            .expect("invalid matcher_regex"));

        Self {
            interface,
            matcher_regex,
        }
    }

    async fn plugin_name(&self) -> String {
        "degrees".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let body = match &channel_message.message.raw {
            None => return,
            Some(b) => b,
        };

        let (cap_names, caps) = {
            let matcher_regex_guard = self.matcher_regex
                .read().await;
            let caps = match matcher_regex_guard.captures(body) {
                Some(c) => c,
                None => return,
            };
            let cap_names: Vec<String> = matcher_regex_guard.capture_names()
                .filter_map(|cn| cn)
                .map(|cn| cn.to_owned())
                .collect();
            (cap_names, caps)
        };

        // find first successful capture whose name ends with "_value"
        let degs_cap_opt = cap_names.into_iter()
            .filter(|cn| cn.ends_with("_value"))
            .filter_map(|cn| caps.name(&cn))
            .nth(0);
        let degs_cap = match degs_cap_opt {
            Some(dc) => dc,
            None => return,
        };
        let degs: f64 = match degs_cap.as_str().parse() {
            Ok(d) => d,
            Err(_) => return,
        };

        // render the degrees
        let mut slice_image = SliceImage::new(500, 500);
        slice_image.draw_angle_deg(degs);

        // serialize into bytes
        let mut slice_png = Vec::new();
        {
            let slice_cursor = Cursor::new(&mut slice_png);
            slice_image.to_png(slice_cursor);
        }

        let attachment = Attachment::new(
            slice_png,
            "degrees".to_owned(),
            "image/png".to_owned(),
            None,
        );
        let outgoing_message = OutgoingMessageWithAttachmentBuilder::new(attachment)
            .build();

        interface
            .send_channel_message_with_attachment(&channel_message.channel.name, outgoing_message)
            .await;
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let matcher_regex_str = match new_config["matcher_regex"].as_str() {
            Some(mrs) => mrs,
            None => {
                error!("matcher_regex is not a string");
                return false;
            },
        };
        let matcher_regex = match Regex::new(matcher_regex_str) {
            Ok(mr) => mr,
            Err(e) => {
                error!("invalid matcher_regex: {}", e);
                return false;
            },
        };

        {
            let mut matcher_regex_guard = self.matcher_regex
                .write().await;
            *matcher_regex_guard = matcher_regex;
        }

        true
    }
}
