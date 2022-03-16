use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::error;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;


#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    commands_responses: HashMap<String, Vec<String>>,
    nicknamable_commands_responses: HashMap<String, Vec<String>>,
}

pub struct TextCommandsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Mutex<StdRng>,
}
impl TextCommandsPlugin {
    fn collect_commands(config_dict: &serde_json::Value) -> Result<HashMap<String, Vec<String>>, &'static str> {
        let mut commands_responses = HashMap::new();
        for (command, variant) in config_dict.entries().ok_or("command structure is not a dict")? {
            let response_values = variant.members()
                .ok_or("responses structure is not a list")?;
            let mut responses: Vec<String> = Vec::new();
            for response_value in response_values {
                let response = response_value
                    .as_str().ok_or("variant is not a string")?
                    .to_owned();
                responses.push(response);
            }
            if responses.len() == 0 {
                continue;
            }
            let command_name = command.to_owned();
            commands_responses.insert(command_name.clone(), responses);
        }
        Ok(commands_responses)
    }

    async fn register_text_command(interface: Arc<dyn RocketBotInterface>, name: &str, nicknamable: bool) {
        let mut random_flags = HashSet::new();
        let (usage, description) = if nicknamable {
            random_flags.insert("r".to_owned());
            random_flags.insert("random".to_owned());
            (
                format!("{{cpfx}}{} [{{lopfx}}random|NICKNAME]", name),
                "Responds to the given text command, inserting a nickname at a predefined location.",
            )
        } else {
            (
                format!("{{cpfx}}{}", name),
                "Responds to the given text command.",
            )
        };

        let command = CommandDefinitionBuilder::new(
            name,
            "text_commands",
            usage,
            description,
        )
            .flags(Some(random_flags))
            .build();
        interface.register_channel_command(&command).await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let commands_responses = Self::collect_commands(
            &config["commands_responses"],
        )?;
        let nicknamable_commands_responses = Self::collect_commands(
            &config["nicknamable_commands_responses"],
        )?;

        Ok(Config {
            commands_responses,
            nicknamable_commands_responses,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for TextCommandsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        for command in config_object.commands_responses.keys() {
            Self::register_text_command(Arc::clone(&my_interface), command, false).await;
        }
        for command in config_object.nicknamable_commands_responses.keys() {
            Self::register_text_command(Arc::clone(&my_interface), command, true).await;
        }

        let config_lock = RwLock::new(
            "TextCommandsPlugin::config",
            config_object,
        );

        let rng = Mutex::new(
            "TextCommandsPlugin::rng",
            StdRng::from_entropy(),
        );

        TextCommandsPlugin {
            interface,
            config: config_lock,
            rng,
        }
    }

    async fn plugin_name(&self) -> String {
        "text_commands".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        if let Some(responses) = config_guard.commands_responses.get(&command.name) {
            if responses.len() == 0 {
                return;
            }

            let variant = {
                let mut rng_guard = self.rng.lock().await;
                let index = rng_guard.gen_range(0..responses.len());
                responses[index].clone()
            };

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &variant,
            ).await;
        } else if let Some(nicknamable_responses) = config_guard.nicknamable_commands_responses.get(&command.name) {
            if nicknamable_responses.len() == 0 {
                return;
            }

            let variant = {
                let mut rng_guard = self.rng.lock().await;
                let index = rng_guard.gen_range(0..nicknamable_responses.len());
                nicknamable_responses[index].clone()
            };

            let channel_members = interface.obtain_users_in_channel(
                &channel_message.channel.name,
            ).await
                .unwrap_or(HashSet::new());

            let target = if channel_members.len() == 0 {
                // fallback to sender
                channel_message.message.sender.username.clone()
            } else if command.flags.contains("r") || command.flags.contains("random") {
                // pick a user randomly
                let mut rng_guard = self.rng.lock().await;
                let index = rng_guard.gen_range(0..channel_members.len());
                channel_members.iter()
                    .nth(index).expect("user entry exists")
                    .username.clone()
            } else if channel_members.iter().any(|u| u.username == command.rest) {
                // the specified user exists
                command.rest.clone()
            } else {
                // the specified user does not exist; fallback to sender
                channel_message.message.sender.username.clone()
            };

            let message_with_target = variant.replace("{{NICKNAME}}", &target);

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &message_with_target,
            ).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        let config_guard = self.config.read().await;
        if config_guard.commands_responses.contains_key(command_name) {
            Some(include_str!("../help/textcommand.md").to_owned())
        } else if config_guard.nicknamable_commands_responses.contains_key(command_name) {
            Some(include_str!("../help/nicktextcommand.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let interface = match self.interface.upgrade() {
            None => {
                error!("interface is gone");
                return false;
            },
            Some(i) => i,
        };

        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;

                // remove old commands
                for command_name in config_guard.commands_responses.keys() {
                    interface.unregister_channel_command(command_name).await;
                }
                for command_name in config_guard.nicknamable_commands_responses.keys() {
                    interface.unregister_channel_command(command_name).await;
                }

                // replace config
                *config_guard = c;

                // register new commands
                for command_name in config_guard.commands_responses.keys() {
                    Self::register_text_command(Arc::clone(&interface), command_name, false).await;
                }
                for command_name in config_guard.nicknamable_commands_responses.keys() {
                    Self::register_text_command(Arc::clone(&interface), command_name, true).await;
                }

                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
        }
    }
}
