mod interface;
mod readers;


use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json::Value;

use crate::interface::VitalsReader;


struct Config {
    lower_key_to_reader: HashMap<String, Box<dyn VitalsReader>>,
    lower_alias_to_lower_key: HashMap<String, String>,
    default_target_lower: String,
}


pub struct VitalsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl VitalsPlugin {
    async fn vitals_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let mut target_lower = command.rest.trim().to_lowercase();
        if target_lower.len() == 0 {
            target_lower = config_guard.default_target_lower.clone();
        }

        // resolve aliases
        let target_lower_aliased = match config_guard.lower_alias_to_lower_key.get(&target_lower) {
            Some(t) => t.clone(),
            None => target_lower.clone(),
        };

        let reader = match config_guard.lower_key_to_reader.get(&target_lower_aliased) {
            Some(r) => r,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("@{} Unknown vital {:?}.", channel_message.message.sender.username, target_lower),
                ).await;
                return;
            },
        };

        let vital = reader.read().await;
        if let Some(v) = vital {
            send_channel_message!(
                interface,
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

        let config_guard = self.config.read().await;

        let key_list: Vec<String> = config_guard.lower_key_to_reader
            .keys()
            .map(|k| format!("`{}`", k))
            .collect();
        let key_string = key_list.join(", ");

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("Available vitals: {}", key_string),
        ).await;
    }

    async fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut lower_key_to_reader = HashMap::new();
        for (target, target_properties) in config["targets"].as_object().ok_or("targets is not an object")?.iter() {
            let reader = target_properties["reader"].as_str()
                .ok_or("reader is not a string")?;
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
                error!("unknown reader type {:?}", reader);
                return Err("unknown reader type");
            };

            lower_key_to_reader.insert(target.to_lowercase(), reader_obj);
        }

        let mut lower_alias_to_lower_key = HashMap::new();
        for (alias, key_object) in config["aliases"].entries_or_empty() {
            let key_lower = match key_object.as_str() {
                Some(s) => s.to_lowercase(),
                None => {
                    error!("aliases[{:?}] not a string", alias);
                    return Err("one of aliases not a string");
                },
            };
            if !lower_key_to_reader.contains_key(&key_lower) {
                error!("aliases[{:?}] points to unknown target {:?}", alias, key_lower);
                return Err("one of aliases points to an unknown target");
            }
            lower_alias_to_lower_key.insert(alias.to_lowercase(), key_lower);
        }

        let default_target_lower = config["default_target"]
            .as_str().ok_or("default_target is not a string")?
            .to_lowercase();
        let default_target_known =
            lower_key_to_reader.contains_key(&default_target_lower)
            || lower_alias_to_lower_key.contains_key(&default_target_lower)
        ;
        if !default_target_known {
            error!("default_target {:?} found neither in targets nor in aliases", default_target_lower);
            return Err("default_target found neither in targets nor in aliases");
        }

        Ok(Config {
            lower_key_to_reader,
            lower_alias_to_lower_key,
            default_target_lower,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for VitalsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config).await
            .expect("failed to obtain config");
        let config_lock = RwLock::new(
            "VitalsPlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "vitals",
                "vitals",
                "{cpfx}vitals [TARGET]",
                "Obtains health-related information about the given target.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "vitallist",
                "vitals",
                "{cpfx}vitallist",
                "Lists the available vitals targets.",
            )
                .build()
        ).await;

        VitalsPlugin {
            interface,
            config: config_lock,
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

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config).await {
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
