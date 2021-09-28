use std::sync::Weak;
use chrono::{Duration, Local};

use async_trait::async_trait;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


pub struct SeriousModePlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for SeriousModePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "srs".to_owned(),
                "serious_mode".to_owned(),
                "{cpfx}srs".to_owned(),
                "Activates Serious Mode for some amount of time.".to_owned(),
            )
                .build()
        ).await;

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "unsrs".to_owned(),
                "serious_mode".to_owned(),
                "{cpfx}unsrs".to_owned(),
                "Deactivates Serious Mode.".to_owned(),
            )
                .build()
        ).await;

        Self {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "serious_mode".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name != "srs" && command.name != "unsrs" {
            return;
        }

        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut current_srs_value = interface
            .obtain_behavior_flags().await
            .remove("srs").unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        let until = Local::now() + Duration::seconds(60 * 60);

        {
            let current_srs_object = current_srs_value
                .as_object_mut().expect("srs behavior flag is not a JSON object");

            if command.name == "srs" {
                current_srs_object.insert(channel_message.channel.id.clone(), serde_json::json!(until.timestamp()));
            } else if command.name == "unsrs" {
                current_srs_object.remove(&channel_message.channel.id);
            } else {
                unreachable!();
            }
        }

        interface.set_behavior_flag("srs", &current_srs_value).await;

        if command.name == "srs" {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("Serious Mode has been activated for {} until {}.", channel_message.channel.name, until.format("%Y-%m-%d %H:%M:%S")),
            ).await;
        } else if command.name == "unsrs" {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("Serious Mode has been deactivated for {}.", channel_message.channel.name),
            ).await;
        } else {
            unreachable!();
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "srs" {
            Some(include_str!("../help/srs.md").to_owned())
        } else if command_name == "unsrs" {
            Some(include_str!("../help/unsrs.md").to_owned())
        } else {
            None
        }
    }
}
