mod interface;
mod readers;


use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json::Value;

use crate::interface::VitalsReader;


pub struct VitalsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    key_to_reader: HashMap<String, Box<dyn VitalsReader>>,
    default_target: String,
}
#[async_trait]
impl RocketBotPlugin for VitalsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut key_to_reader = HashMap::new();
        for (target, target_properties) in config["targets"].as_object().expect("targets is not an object").iter() {
            let reader = target_properties["reader"].as_str()
                .expect("reader is not a string");
            let reader_config = &target_properties["config"];

            let reader_obj: Box<dyn VitalsReader> = if reader == "beepee" {
                Box::new(crate::readers::beepee::BeepeeReader::new(&reader_config).await)
            } else if reader == "constant" {
                Box::new(crate::readers::constant::ConstantReader::new(&reader_config).await)
            } else if reader == "nightscout" {
                Box::new(crate::readers::nightscout::NightscoutReader::new(&reader_config).await)
            } else if reader == "random" {
                Box::new(crate::readers::random::RandomReader::new(&reader_config).await)
            } else {
                panic!("unknown reader type {:?}", reader);
            };

            key_to_reader.insert(target.to_owned(), reader_obj);
        }

        let default_target = config["default_target"].as_str()
            .expect("default_target is not a string")
            .to_owned();

        my_interface.register_channel_command(&CommandDefinition::new(
            "vitals".to_owned(),
            "vitals".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}vitals [TARGET]".to_owned(),
            "Obtains health-related information about the given target.".to_owned(),
        )).await;

        VitalsPlugin {
            interface,
            key_to_reader,
            default_target,
        }
    }

    async fn plugin_name(&self) -> String {
        "vitals".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name != "vitals" {
            return;
        }

        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut target = command.rest.trim();
        if target.len() == 0 {
            target = self.default_target.as_str();
        }

        let reader = match self.key_to_reader.get(target) {
            Some(r) => r,
            None => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{} Unknown vital {:?}.", channel_message.message.sender.username, target),
                ).await;
                return;
            },
        };

        let vital = reader.read().await;
        if let Some(v) = vital {
            interface.send_channel_message(
                &channel_message.channel.name,
                &format!("@{} {}", channel_message.message.sender.username, v),
            ).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "vitals" {
            Some(include_str!("../help/vitals.md").to_owned())
        } else {
            None
        }
    }
}
