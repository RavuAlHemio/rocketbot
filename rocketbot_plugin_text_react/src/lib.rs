use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use log::error;
use regex::Regex;
use rocketbot_interface::ResultExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;


#[derive(Clone, Debug)]
struct Reaction {
    text_pattern: Regex,
    reaction_names: Vec<String>,
}
impl Reaction {
    pub fn new(
        text_pattern: Regex,
        reaction_names: Vec<String>,
    ) -> Self {
        Self {
            text_pattern,
            reaction_names,
        }
    }
}


#[derive(Clone, Debug)]
struct Config {
    reactions: Vec<Reaction>,
}


pub struct TextReactPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl TextReactPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let reaction_configs = config["reactions"].as_array()
            .ok_or("reactions is not an array")?;

        let mut reactions = Vec::with_capacity(reaction_configs.len());
        for reaction_config_value in reaction_configs {
            let reaction_config = reaction_config_value.as_object()
                .ok_or("element of reactions is not an object")?;

            let text_pattern_str = reaction_config
                .get("text_pattern").ok_or("text_pattern is missing")?
                .as_str().ok_or("text_pattern is not a string")?;
            let text_pattern = Regex::new(text_pattern_str)
                .or_msg("failed to parse text_pattern")?;

            let reaction_names_values = reaction_config
                .get("reaction_names").ok_or("reaction_names is missing")?
                .as_array().ok_or("reaction_names is not a list")?;
            let mut reaction_names = Vec::with_capacity(reaction_names_values.len());
            for reaction_name_value in reaction_names_values {
                let reaction_name = reaction_name_value
                    .as_str().ok_or("element of reaction_names is not a string")?;
                reaction_names.push(reaction_name.to_owned());
            }

            reactions.push(Reaction::new(
                text_pattern,
                reaction_names,
            ));
        }

        Ok(Config {
            reactions,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for TextReactPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "LinkReactPlugin::config",
            config_object,
        );

        Self {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "text_react".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        // do not trigger if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        let raw_message = match &channel_message.message.raw {
            Some(rm) => rm,
            None => return, // just some attachments
        };

        let config_guard = self.config.read().await;

        // look for a match
        for reaction in &config_guard.reactions {
            if reaction.text_pattern.is_match(&raw_message) {
                // react
                for reaction_name in &reaction.reaction_names {
                    interface.add_reaction(
                        &channel_message.message.id,
                        &reaction_name,
                    ).await;
                }
            }
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
