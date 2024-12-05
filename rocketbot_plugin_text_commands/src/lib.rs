use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;
use tracing::{debug, error};


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MadLibsCommand {
    /// Argument count except rest.
    arg_count: usize,

    response_templates: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    commands_responses: HashMap<String, Vec<String>>,
    nicknamable_commands_responses: HashMap<String, Vec<String>>,
    mad_libs_commands: HashMap<String, MadLibsCommand>,
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
            random_flags.insert("b".to_owned());
            random_flags.insert("also-bots".to_owned());
            random_flags.insert("B".to_owned());
            random_flags.insert("only-bots".to_owned());
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

    async fn register_mad_libs_command<I: RocketBotInterface + ?Sized>(interface: &I, name: &str, cmd: &MadLibsCommand) {
        let mut usage = format!("{{cpfx}}{}", name);
        for i in 0..cmd.arg_count {
            write!(usage, " ARG{}", i).unwrap();
        }
        write!(usage, " TEXT").unwrap();
        let description = "Responds to the given text command, filling predefined placeholders.";

        let command = CommandDefinitionBuilder::new(
            name,
            "text_commands",
            usage,
            description,
        )
            .arg_count(cmd.arg_count)
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

        let mut mad_libs_commands = HashMap::new();
        for (cmd_name, cmd_def) in config["mad_libs_commands"].entries_or_empty() {
            let arg_count = match cmd_def["arg_count"].as_usize() {
                Some(ac) => ac,
                None => continue,
            };
            let mut response_templates = Vec::new();
            for rt in cmd_def["response_templates"].members_or_empty() {
                if let Some(s) = rt.as_str() {
                    response_templates.push(s.to_owned());
                }
            }
            if response_templates.len() == 0 {
                continue;
            }

            let command = MadLibsCommand {
                arg_count,
                response_templates,
            };
            mad_libs_commands.insert(cmd_name.clone(), command);
        }

        Ok(Config {
            commands_responses,
            nicknamable_commands_responses,
            mad_libs_commands,
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
        for (cmd_name, cmd_def) in config_object.mad_libs_commands.iter() {
            Self::register_mad_libs_command(&*my_interface, cmd_name, cmd_def).await;
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

            let mut channel_members = interface.obtain_users_in_channel(
                &channel_message.channel.name,
            ).await
                .unwrap_or(HashSet::new());

            if command.flags.contains("B") || command.flags.contains("only-bots") {
                // find bots and only allow them
                let bot_ids = interface.obtain_bot_user_ids().await;
                debug!("bot IDs: {:?}", bot_ids);
                channel_members.retain(|member| bot_ids.contains(&member.id));
            } else if !command.flags.contains("b") && !command.flags.contains("also-bots") {
                // find bots and remove them
                let bot_ids = interface.obtain_bot_user_ids().await;
                debug!("bot IDs: {:?}", bot_ids);
                channel_members.retain(|member| !bot_ids.contains(&member.id));
            }
            debug!("channel members being considered: {:?}", channel_members);

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
        } else if let Some(mad_libs_def) = config_guard.mad_libs_commands.get(&command.name) {
            if mad_libs_def.response_templates.len() == 0 {
                return;
            }

            let mut outgoing = {
                let mut rng_guard = self.rng.lock().await;
                let index = rng_guard.gen_range(0..mad_libs_def.response_templates.len());
                mad_libs_def.response_templates[index].clone()
            };

            for (i, replacement) in command.args.iter().enumerate() {
                let placeholder = format!("{}ARG{}{}", "{{", i, "}}");
                outgoing = outgoing.replace(&placeholder, replacement);
            }
            outgoing = outgoing.replace("{{TEXT}}", &command.rest);

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &outgoing,
            ).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        let config_guard = self.config.read().await;
        if config_guard.commands_responses.contains_key(command_name) {
            Some(include_str!("../help/textcommand.md").to_owned())
        } else if config_guard.nicknamable_commands_responses.contains_key(command_name) {
            Some(include_str!("../help/nicktextcommand.md").to_owned())
        } else if config_guard.mad_libs_commands.contains_key(command_name) {
            Some(include_str!("../help/madlibscommand.md").to_owned())
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
                for command_name in config_guard.mad_libs_commands.keys() {
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
                for (cmd_name, cmd_def) in config_guard.mad_libs_commands.iter() {
                    Self::register_mad_libs_command(&*interface, cmd_name, cmd_def).await;
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
