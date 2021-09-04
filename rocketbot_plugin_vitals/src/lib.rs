mod interface;
mod readers;


use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json::Value;

use crate::interface::VitalsReader;


pub struct VitalsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    lower_key_to_reader: HashMap<String, Box<dyn VitalsReader>>,
    lower_alias_to_lower_key: HashMap<String, String>,
    default_target_lower: String,
}
impl VitalsPlugin {
    async fn vitals_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut target_lower = command.rest.trim().to_lowercase();
        if target_lower.len() == 0 {
            target_lower = self.default_target_lower.clone();
        }

        // resolve aliases
        let target_lower_aliased = match self.lower_alias_to_lower_key.get(&target_lower) {
            Some(t) => t.clone(),
            None => target_lower.clone(),
        };

        let reader = match self.lower_key_to_reader.get(&target_lower_aliased) {
            Some(r) => r,
            None => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{} Unknown vital {:?}.", channel_message.message.sender.username, target_lower),
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

    async fn vitallist_command(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let key_list: Vec<String> = self.lower_key_to_reader
            .keys()
            .map(|k| format!("`{}`", k))
            .collect();
        let key_string = key_list.join(", ");

        interface.send_channel_message(
            &channel_message.channel.name,
            &format!("Available vitals: {}", key_string),
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for VitalsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut lower_key_to_reader = HashMap::new();
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

            lower_key_to_reader.insert(target.to_lowercase(), reader_obj);
        }

        let mut lower_alias_to_lower_key = HashMap::new();
        for (alias, key_object) in config["aliases"].entries_or_empty() {
            let key_lower = match key_object.as_str() {
                Some(s) => s.to_lowercase(),
                None => panic!("aliases[{:?}] not a string", alias),
            };
            if !lower_key_to_reader.contains_key(&key_lower) {
                panic!("aliases[{:?}] points to unknown target {:?}", alias, key_lower);
            }
            lower_alias_to_lower_key.insert(alias.to_lowercase(), key_lower);
        }

        let default_target_lower = config["default_target"].as_str()
            .expect("default_target is not a string")
            .to_lowercase();
        let default_target_known =
            lower_key_to_reader.contains_key(&default_target_lower)
            || lower_alias_to_lower_key.contains_key(&default_target_lower)
        ;
        if !default_target_known {
            panic!("default_target {:?} found neither in targets nor in aliases", default_target_lower);
        }

        my_interface.register_channel_command(&CommandDefinition::new(
            "vitals".to_owned(),
            "vitals".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}vitals [TARGET]".to_owned(),
            "Obtains health-related information about the given target.".to_owned(),
        )).await;
        my_interface.register_channel_command(&CommandDefinition::new(
            "vitallist".to_owned(),
            "vitals".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}vitallist".to_owned(),
            "Lists the available vitals targets.".to_owned(),
        )).await;

        VitalsPlugin {
            interface,
            lower_key_to_reader,
            lower_alias_to_lower_key,
            default_target_lower,
        }
    }

    async fn plugin_name(&self) -> String {
        "vitals".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "vitals" {
            self.vitals_command(channel_message, command).await
        } else if command.name == "vitallist" {
            self.vitallist_command(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "vitals" {
            Some(include_str!("../help/vitals.md").to_owned())
        } else if command_name == "vitallist" {
            Some(include_str!("../help/vitallist.md").to_owned())
        } else {
            None
        }
    }
}
