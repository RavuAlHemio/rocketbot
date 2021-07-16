use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Weak;

use async_trait::async_trait;
use json::JsonValue;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;


pub struct FortunePlugin {
    interface: Weak<dyn RocketBotInterface>,
    name_to_fortunes: HashMap<String, Vec<String>>,
    rng: Mutex<StdRng>,
}
#[async_trait]
impl RocketBotPlugin for FortunePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let fortune_command = CommandDefinition::new(
            "fortune".to_owned(),
            "fortune".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}fortune [GROUP]".to_owned(),
            "Selects and displays a random fortune, optionally from a specific group.".to_owned(),
        );
        my_interface.register_channel_command(&fortune_command).await;

        let mut name_to_fortunes = HashMap::new();
        for fortune_file_path_value in config["fortune_files"].members() {
            let fortune_file_path = fortune_file_path_value
                .as_str().expect("entry in fortune_files is not a string");
            let fortune_file_name: String = Path::new(fortune_file_path)
                .file_name().expect("file name exists")
                .to_str().expect("file name is valid UTF-8")
                .into();
            let mut fortune_file = match File::open(fortune_file_path) {
                Ok(ff) => ff,
                Err(e) => panic!("failed to open fortune file {}: {}", fortune_file_path, e),
            };

            let mut content = String::new();
            fortune_file.read_to_string(&mut content)
                .expect("failed to read file");

            let mut fortunes: Vec<String> = Vec::new();
            for piece in content.split("\n%\n") {
                fortunes.push(piece.trim().into());
            }
            name_to_fortunes.insert(fortune_file_name, fortunes);
        }

        let rng = Mutex::new(
            "FortunePlugin::rng",
            StdRng::from_entropy(),
        );

        FortunePlugin {
            interface,
            name_to_fortunes,
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

        let category: Option<String> = match command.rest.trim() {
            "" => None,
            other => Some(other.into()),
        };

        if let Some(cat) = category {
            match self.name_to_fortunes.get(&cat) {
                None => {
                    // well, what groups _do_ we have?
                    let mut fortune_groups: Vec<String> = self.name_to_fortunes.keys()
                        .map(|k| format!("`{}`", k))
                        .collect();
                    fortune_groups.sort_unstable();
                    let fortune_group_string = fortune_groups.join(", ");

                    interface
                        .send_channel_message(
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
                        interface
                            .send_channel_message(
                                &channel_message.channel.name,
                                &fortune_as_quote,
                            ).await;
                    }
                },
            }
        } else {
            // pick one from all categories
            let total_count: usize = self.name_to_fortunes.values()
                .map(|v| v.len())
                .sum();
            if total_count > 0 {
                let mut rng_guard = self.rng
                    .lock().await;
                let mut index = rng_guard.gen_range(0..total_count);
                for fortunes in self.name_to_fortunes.values() {
                    if index >= fortunes.len() {
                        index -= fortunes.len();
                        continue;
                    }

                    let fortune = &fortunes[index];
                    let fortune_as_quote = format!(">{}", fortune.replace("\n", "\n>"));
                    interface
                        .send_channel_message(
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
            let mut fortune_groups: Vec<String> = self.name_to_fortunes.keys()
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
}
