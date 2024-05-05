mod pseudotrie;


use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::fs::File;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::{ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;
use tracing::error;

use crate::pseudotrie::StringPseudotrie;


#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
struct Singleton;


#[derive(Clone, Debug)]
struct Config {
    words: StringPseudotrie<Singleton>,
}


pub struct NumberwordPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    digit_to_letters: HashMap<char, Vec<char>>,
}
impl NumberwordPlugin {
    async fn channel_command_unkeypad(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let trimmed_chars: Vec<char> = command.rest.trim().chars().collect();

        for &c in &trimmed_chars {
            if !self.digit_to_letters.contains_key(&c) {
                error!("invalid digit {:?}", c);
                return;
            }
        }

        let wfd = self.get_words_for_digits(&config_guard, &trimmed_chars, "");
        let response_text = if wfd.len() > 5 {
            let mut r = wfd[0..5].join(", ");
            r.push_str(", ...");
            r
        } else {
            wfd.join(", ")
        };

        if response_text.len() == 0 {
            return;
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_text,
        ).await;
    }

    fn get_words_for_digits(&self, config: &Config, digits: &[char], prefix: &str) -> Vec<String> {
        if digits.len() == 0 {
            // a full word!

            return if config.words.get(prefix).is_some() {
                vec![prefix.to_owned()]
            } else {
                Vec::with_capacity(0)
            };
        }

        if !config.words.contains_entries_with_prefix(prefix) {
            // no such words...
            return Vec::with_capacity(0);
        }

        let digit = digits[0];
        let mut sub_words = Vec::new();
        for &letter in self.digit_to_letters.get(&digit).expect("digit missing in map") {
            let mut sub_prefix = prefix.to_owned();
            sub_prefix.push(letter);

            let mut this_sub_words = self.get_words_for_digits(config, &digits[1..], &sub_prefix);
            sub_words.append(&mut this_sub_words);
        }

        sub_words
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut words = StringPseudotrie::new();
        for entry in config["wordlists"].as_array().ok_or("wordlists is not an array")? {
            let file_name = entry.as_str().ok_or("wordlists entry is not a string")?;
            let file = File::open(file_name)
                .or_msg("failed to open wordlist file")?;
            let mut reader = BufReader::new(file);

            let mut line = String::new();
            loop {
                line.clear();
                let read = reader.read_line(&mut line)
                    .or_msg("failed to read line from wordlist file")?;
                if read == 0 {
                    break;
                }

                let word = line
                    .trim()
                    .to_uppercase()
                    .replace("\u{C4}", "AE")
                    .replace("\u{D6}", "OE")
                    .replace("\u{DC}", "UE")
                ;
                if word.chars().any(|c| c < 'A' || c > 'Z') {
                    // skip this word
                    continue;
                }
                words.insert(&word, Singleton);
            }
        }

        Ok(Config {
            words,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for NumberwordPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "NumberwordPlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "unkeypad",
                "numberword",
                "{cpfx}unkeypad NUMBER",
                "Attempts to guess the word described by the phone-keypad number.",
            )
                .build()
        ).await;

        let digit_to_letters = {
            let mut dtl = HashMap::new();

            dtl.insert('2', vec!['A', 'B', 'C']);
            dtl.insert('3', vec!['D', 'E', 'F']);
            dtl.insert('4', vec!['G', 'H', 'I']);
            dtl.insert('5', vec!['J', 'K', 'L']);
            dtl.insert('6', vec!['M', 'N', 'O']);
            dtl.insert('7', vec!['P', 'Q', 'R', 'S']);
            dtl.insert('8', vec!['T', 'U', 'V']);
            dtl.insert('9', vec!['W', 'X', 'Y', 'Z']);

            dtl
        };

        Self {
            interface,
            config: config_lock,
            digit_to_letters,
        }
    }

    async fn plugin_name(&self) -> String {
        "numberword".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "unkeypad" {
            self.channel_command_unkeypad(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "unkeypad" {
            Some(include_str!("../help/unkeypad.md").to_owned())
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
