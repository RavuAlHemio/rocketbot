use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::{JsonValueExtensions, ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;


#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    name_to_fortunes: HashMap<String, Vec<String>>,
}


pub struct FortunePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Mutex<StdRng>,
}
impl FortunePlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut name_to_fortunes = HashMap::new();
        for fortune_file_path_value in config["fortune_files"].members().ok_or("fortune_files not a list")? {
            let fortune_file_path = fortune_file_path_value
                .as_str().ok_or("entry in fortune_files is not a string")?;
            let fortune_file_name: String = Path::new(fortune_file_path)
                .file_name().ok_or("file name does not exist")?
                .to_str().ok_or("file name is not valid UTF-8")?
                .to_owned();
            let mut fortune_file = match File::open(fortune_file_path) {
                Ok(ff) => ff,
                Err(e) => {
                    error!("failed to open fortune file {}: {}", fortune_file_path, e);
                    return Err("failed to open fortune file");
                },
            };

            let mut content = String::new();
            fortune_file.read_to_string(&mut content)
                .or_msg("failed to read file")?;

            let mut fortunes: Vec<String> = Vec::new();
            for piece in content.split("\n%\n") {
                fortunes.push(piece.trim().into());
            }
            name_to_fortunes.insert(fortune_file_name, fortunes);
        }

        Ok(Config {
            name_to_fortunes,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for FortunePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "FortunePlugin::config",
            config_object,
        );

        let fortune_command = CommandDefinitionBuilder::new(
            "fortune",
            "fortune",
            "{cpfx}fortune [GROUP]",
            "Selects and displays a random fortune, optionally from a specific group.",
        )
            .build();
        my_interface.register_channel_command(&fortune_command).await;

        let rng = Mutex::new(
            "FortunePlugin::rng",
            StdRng::from_entropy(),
        );

        FortunePlugin {
            interface,
            config: config_lock,
            rng,
        }
    }

    async fn plugin_name(&self) -> String {
        "fortune".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "fortune" {
            return;
        }

        let config_guard = self.config.read().await;

        let category: Option<String> = match command.rest.trim() {
            "" => None,
            other => Some(other.into()),
        };

        if let Some(cat) = category {
            match config_guard.name_to_fortunes.get(&cat) {
                None => {
                    // well, what groups _do_ we have?
                    let mut fortune_groups: Vec<String> = config_guard.name_to_fortunes.keys()
                        .map(|k| format!("`{}`", k))
                        .collect();
                    fortune_groups.sort_unstable();
                    let fortune_group_string = fortune_groups.join(", ");

                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!(
                            "@{} `{}` is an unknown fortune group; we have: {}",
                            channel_message.message.sender.username,
                            cat,
                            fortune_group_string,
                        ),
                    ).await;
                },
                Some(fortunes) => {
                    if fortunes.len() > 0 {
                        let mut rng_guard = self.rng
                            .lock().await;
                        let index = rng_guard.gen_range(0..fortunes.len());
                        let fortune = &fortunes[index];
                        let fortune_as_quote = format!(">{}", fortune.replace("\n", "\n>"));
                        send_channel_message!(
                            interface,
                            &channel_message.channel.name,
                            &fortune_as_quote,
                        ).await;
                    }
                },
            }
        } else {
            // pick one from all categories
            let total_count: usize = config_guard.name_to_fortunes.values()
                .map(|v| v.len())
                .sum();
            if total_count > 0 {
                let mut rng_guard = self.rng
                    .lock().await;
                let mut index = rng_guard.gen_range(0..total_count);
                for fortunes in config_guard.name_to_fortunes.values() {
                    if index >= fortunes.len() {
                        index -= fortunes.len();
                        continue;
                    }

                    let fortune = &fortunes[index];
                    let fortune_as_quote = format!(">{}", fortune.replace("\n", "\n>"));
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &fortune_as_quote,
                    ).await;
                    break;
                }
            }
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "fortune" {
            let config_guard = self.config.read().await;
            let mut fortune_groups: Vec<String> = config_guard.name_to_fortunes.keys()
                .map(|k| format!("`{}`", k))
                .collect();
            fortune_groups.sort_unstable();
            let fortune_group_string = fortune_groups.join(", ");

            Some(
                include_str!("../help/fortune.md")
                    .replace("{fortune_groups}", &fortune_group_string)
            )
        } else {
            None
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
