use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use json::JsonValue;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;


pub struct TextCommandsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    commands_responses: HashMap<String, Vec<String>>,
    nicknamable_commands_responses: HashMap<String, Vec<String>>,
    rng: Mutex<StdRng>,
}
impl TextCommandsPlugin {
    async fn collect_commands(my_interface: Arc<dyn RocketBotInterface>, config_dict: &JsonValue, nicknamable: bool) -> HashMap<String, Vec<String>> {
        let mut commands_responses = HashMap::new();
        for (command, variant) in config_dict.entries() {
            let responses: Vec<String> = variant.members()
                .map(|s|
                    s.as_str()
                        .expect("variant is not a string")
                        .to_owned()
                )
                .collect();
            if responses.len() == 0 {
                continue;
            }
            let command_name = command.to_owned();
            commands_responses.insert(command_name.clone(), responses);

            let mut random_flags = HashSet::new();
            if nicknamable {
                random_flags.insert("r".to_owned());
                random_flags.insert("random".to_owned());
            }

            let command = CommandDefinition::new(
                command_name.clone(),
                Some(random_flags),
                HashMap::new(),
                0,
                if nicknamable { format!("{{cpfx}}{} [{{lopfx}}random|NICKNAME]", command_name) } else { format!("{{cpfx}}{}", command_name) },
                if nicknamable {
                    "Responds to the given text command, inserting a nickname at a predefined location.".to_owned()
                } else {
                    "Responds to the given text command.".to_owned()
                },
            );
            my_interface.register_channel_command(&command).await;
        }
        commands_responses
    }
}
#[async_trait]
impl RocketBotPlugin for TextCommandsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let commands_responses = TextCommandsPlugin::collect_commands(
            Arc::clone(&my_interface),
            &config["commands_responses"],
            false,
        ).await;

        let nicknamable_commands_responses = TextCommandsPlugin::collect_commands(
            Arc::clone(&my_interface),
            &config["nicknamable_commands_responses"],
            true,
        ).await;

        let rng = Mutex::new(
            "TextCommandsPlugin::rng",
            StdRng::from_entropy(),
        );

        TextCommandsPlugin {
            interface,
            commands_responses,
            nicknamable_commands_responses,
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

        if let Some(responses) = self.commands_responses.get(&command.name) {
            if responses.len() == 0 {
                return;
            }

            let variant = {
                let mut rng_guard = self.rng.lock().await;
                let index = rng_guard.gen_range(0..responses.len());
                responses[index].clone()
            };

            interface.send_channel_message(
                &channel_message.channel.name,
                &variant,
            ).await;
        } else if let Some(nicknamable_responses) = self.nicknamable_commands_responses.get(&command.name) {
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

            interface.send_channel_message(
                &channel_message.channel.name,
                &message_with_target,
            ).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if self.commands_responses.contains_key(command_name) {
            Some(include_str!("../help/textcommand.md").to_owned())
        } else if self.nicknamable_commands_responses.contains_key(command_name) {
            Some(include_str!("../help/nicktextcommand.md").to_owned())
        } else {
            None
        }
    }
}
