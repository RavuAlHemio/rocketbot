use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


pub struct BelchPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for BelchPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        BelchPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "belch".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let raw_message = match &channel_message.message.raw {
            Some(m) => m,
            None => return,
        };
        if raw_message == "!burp" {
            let interface = match self.interface.upgrade() {
                None => return,
                Some(i) => i,
            };
            send_channel_message!(interface, &channel_message.channel.name, "_belches loudly_")
                .await;
        }
    }
}
