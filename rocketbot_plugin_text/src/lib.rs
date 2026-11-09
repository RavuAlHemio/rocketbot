use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;
use rocketbot_spelling::{HunspellEngine, SpellingEngine};
use serde_json;
use strsim;
use tracing::error;


fn rotate(s: &str, n: u32) -> String {
    assert!(n < 26);

    let mut ret = String::new();
    for c in s.chars() {
        let bias = if c >= 'A' && c <= 'Z' {
            'A' as u32
        } else if c >= 'a' && c <= 'z' {
            'a' as u32
        } else {
            ret.push(c);
            continue;
        };

        let old_letter_index = (c as u32) - bias;
        let new_letter_index = (old_letter_index + n) % 26;
        let new_letter = char::from_u32(new_letter_index + bias).unwrap();
        ret.push(new_letter);
    }
    ret
}


pub struct TextPlugin {
    interface: Weak<dyn RocketBotInterface>,
    spelling_engine: Mutex<Box<dyn SpellingEngine + Send>>,
}
impl TextPlugin {
    fn same_char_set(a: &str, b: &str) -> bool {
        let a_set: HashSet<char> = a.chars().collect();
        let b_set: HashSet<char> = b.chars().collect();
        a_set == b_set
    }

    async fn channel_command_compare(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let string1 = &command.args[0];
        let string2 = &command.args[1];

        let comparison_str = match string1.cmp(string2) {
            Ordering::Greater => ">",
            Ordering::Equal => "==",
            Ordering::Less => "<",
        };

        let lev = strsim::levenshtein(string1, string2);
        let dam_lev = strsim::damerau_levenshtein(string1, string2);
        let optimal_string_align_dist = strsim::osa_distance(string1, string2);

        let hamming = strsim::hamming(string1, string2);
        let hamming_string = match hamming {
            Ok(dist) => format!("H={}", dist),
            Err(_) => "!H".to_owned(),
        };

        let jaro = strsim::jaro(string1, string2);
        let jaro_winkler = strsim::jaro_winkler(string1, string2);
        let sorensen_dice = strsim::sorensen_dice(string1, string2);

        let same_set_str = if Self::same_char_set(string1, string2) {
            "SCS"
        } else {
            "!SCS"
        };

        let response = format!(
            concat!(
                "{:?} {} {:?}:",
                "\nL={} D-L={} OSA={} {}",
                "\nJ={:.3}% J-W={:.3}% S-D={:.3}%",
                "\n{}",
            ),
            string1, comparison_str, string2,
            lev, dam_lev, optimal_string_align_dist, hamming_string,
            jaro * 100.0, jaro_winkler * 100.0, sorensen_dice * 100.0,
            same_set_str,
        );

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_rot(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let number_str = &command.args[0];
        let s = &command.rest;

        let mut raw_number: i128 = match number_str.parse() {
            Ok(n) => n,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "What kind of an integer is that?! O_o",
                ).await;
                return;
            },
        };
        raw_number %= 26;
        while raw_number < 0 {
            raw_number += 26;
        }
        assert!(raw_number >= 0 && raw_number < 26);
        let number: u32 = raw_number.try_into().unwrap();

        let rotated = rotate(s, number);
        if &rotated == s {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "That string needs some ASCII letters...",
            ).await;
            return;
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &rotated,
        ).await;
    }

    async fn channel_command_rotfind(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let s = &command.rest;

        // go through the rotations
        let mut rot_words = Vec::with_capacity(26);
        {
            let spelling_engine_guard = self.spelling_engine
                .lock().await;
            for n in 1..26 {
                let rotated = rotate(s, n);
                if &rotated == s {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "That string needs some ASCII letters...",
                    ).await;
                    return;
                }

                if spelling_engine_guard.is_correct(&rotated) {
                    rot_words.push(format!("{}: {}", n, rotated));
                }
            }
        }

        if rot_words.len() > 0 {
            let resp_string = rot_words.join("\n");
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &resp_string,
            ).await;
        } else {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "No rotation found that forms a word. :disappointed:",
            ).await;
        }
    }
}
#[async_trait]
impl RocketBotPlugin for TextPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self where Self: Sized {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        // launch the spelling engine
        let hunspell_engine = HunspellEngine::new(config["hunspell"].clone())
            .expect("failed to construct the initial spelling engine");

        // register commands
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "compare",
                "text",
                "{cpfx}compare STRING1 STRING2",
                "Outputs information about how much two strings differ.",
            )
                .arg_count(2)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "rot",
                "text",
                "{cpfx}rot N STRING",
                "Performs a letter rotation cipher on the given string.",
            )
                .arg_count(1)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "rotfind",
                "text",
                "{cpfx}rotfind STRING",
                "Tries to find letter rotation results on the given string that are themselves words.",
            )
                .build()
        ).await;

        Self {
            interface,
            spelling_engine: Mutex::new(
                "TextPlugin::spelling_engine",
                Box::new(hunspell_engine),
            ),
        }
    }

    async fn plugin_name(&self) -> String {
        "text".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "compare" {
            self.channel_command_compare(channel_message, command).await
        } else if command.name == "rot" {
            self.channel_command_rot(channel_message, command).await
        } else if command.name == "rotfind" {
            self.channel_command_rotfind(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "compare" {
            Some(include_str!("../help/compare.md").to_owned())
        } else if command_name == "rot" {
            Some(include_str!("../help/rot.md").to_owned())
        } else if command_name == "rotfind" {
            Some(include_str!("../help/rotfind.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        // try obtaining a fresh spelling engine
        let hunspell_engine = match HunspellEngine::new(new_config["hunspell"].clone()) {
            Some(he) => he,
            None => {
                error!("failed to construct a new spelling engine");
                return false;
            },
        };

        // replace
        let mut spelling_engine_guard = self.spelling_engine
            .lock().await;
        *spelling_engine_guard = Box::new(hunspell_engine);

        // done
        true
    }
}
