use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use log::debug;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, send_channel_message, send_private_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, PrivateMessage};
use rocketbot_interface::sync::Mutex;
use rocketbot_regex_replace::ReplacerRegex;
use serde_json;


#[derive(Debug)]
struct LocalReplacerRegex {
    pub replacer_regex: ReplacerRegex,
    pub additional_probability_percent: u8,
    pub custom_cooldown_increase_per_hit: Option<usize>,
    pub replace_full_message: bool,
}
impl LocalReplacerRegex {
    pub fn new(
        replacer_regex: ReplacerRegex,
        additional_probability_percent: u8,
        custom_cooldown_increase_per_hit: Option<usize>,
        replace_full_message: bool,
    ) -> LocalReplacerRegex {
        LocalReplacerRegex {
            replacer_regex,
            additional_probability_percent,
            custom_cooldown_increase_per_hit,
            replace_full_message,
        }
    }
}


#[derive(Debug)]
struct InnerState {
    pub cooldowns_per_channel: HashMap<String, Vec<usize>>,
    pub rng: StdRng,
}
impl InnerState {
    pub fn new(
        cooldowns_per_channel: HashMap<String, Vec<usize>>,
        rng: StdRng,
    ) -> Self {
        Self {
            cooldowns_per_channel,
            rng,
        }
    }
}


pub struct AllographPlugin {
    interface: Weak<dyn RocketBotInterface>,
    probability_percent: u8,
    replacer_regexes: Vec<LocalReplacerRegex>,
    cooldown_increase_per_hit: usize,
    inner_state: Mutex<InnerState>,
}
impl AllographPlugin {
    async fn private_command_allocool(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let state_guard = self.inner_state
            .lock().await;

        let cooldowns = match state_guard.cooldowns_per_channel.get(&command.rest) {
            Some(cds) => cds,
            None => {
                send_private_message!(
                    interface,
                    &private_message.conversation.id,
                    "No cooldowns for this channel.",
                ).await;
                return;
            },
        };

        send_private_message!(
            interface,
            &private_message.conversation.id,
            &format!("Cooldowns for this channel: {:?}", cooldowns),
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for AllographPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let probability_percent = config["probability_percent"].as_u8()
            .expect("probability_percent missing or not representable as u8");
        let replacer_regexes: Vec<LocalReplacerRegex> = config["replacements"].members()
            .expect("replacements is not a list")
            .map(|repl| LocalReplacerRegex::new(
                ReplacerRegex::compile_new(
                    Regex::new(
                        repl["regex_string"].as_str().expect("regex_string missing or not a string"),
                    ).unwrap(),
                    repl["replacement_string"].as_str()
                        .expect("replacement_string missing or not a string")
                        .to_owned(),
                ).expect("failed to compile replacer regex"),
                if repl.has_key("additional_probability_percent") {
                    repl["additional_probability_percent"].as_u8()
                        .expect("additional_probability_percent not representable as u8")
                } else {
                    100
                },
                if repl.has_key("custom_cooldown_increase_per_hit") {
                    Some(
                        repl["custom_cooldown_increase_per_hit"].as_usize()
                            .expect("custom_cooldown_increase_per_hit not representable as usize")
                    )
                } else {
                    None
                },
                if repl.has_key("replace_full_message") {
                    repl["replace_full_message"].as_bool()
                        .expect("replace_full_message not representable as bool")
                } else {
                    false
                },
            ))
            .collect();
        let cooldown_increase_per_hit = if config.has_key("cooldown_increase_per_hit") {
            config["cooldown_increase_per_hit"].as_usize()
                .expect("cooldown_increase_per_hit not representable as usize")
        } else {
            0
        };

        my_interface.register_private_message_command(
            &CommandDefinitionBuilder::new(
                "allocool".into(),
                "allograph".to_owned(),
                "{cpfx}allocool CHANNEL".to_owned(),
                "Outputs current Allograph cooldowns in the given channel.".to_owned(),
            )
                .build()
        ).await;

        let inner_state = Mutex::new(
            "AllographPlugin::inner_state",
            InnerState::new(
                HashMap::new(),
                StdRng::from_entropy(),
            ),
        );

        AllographPlugin {
            interface,
            probability_percent,
            replacer_regexes,
            cooldown_increase_per_hit,
            inner_state,
        }
    }

    async fn plugin_name(&self) -> String {
        "allograph".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let original_body = match &channel_message.message.raw {
            Some(s) => s,
            None => return, // no message, probably just attachments
        };
        let channel_name = &channel_message.channel.name;
        let sender_nickname = channel_message.message.sender.nickname_or_username();

        let mut state_guard = self.inner_state
            .lock().await;
        let inner_state = &mut *state_guard;

        let channel_cooldowns = inner_state.cooldowns_per_channel
            .entry(channel_name.clone())
            .or_insert_with(|| vec![0usize; self.replacer_regexes.len()]);

        let mut lookups: HashMap<String, String> = HashMap::new();
        lookups.insert("username".to_owned(), sender_nickname.to_owned());

        let mut changing_body = original_body.clone();
        for (i, replacement) in self.replacer_regexes.iter().enumerate() {
            // perform the replacement
            let replaced = replacement.replacer_regex.replace(&changing_body, &lookups);

            if self.cooldown_increase_per_hit > 0 || replacement.custom_cooldown_increase_per_hit.is_some() {
                if changing_body != replaced {
                    // this rule changed something!

                    if channel_cooldowns[i] == 0 {
                        // cold, apply it!
                        if replacement.additional_probability_percent < 100 {
                            let add_prob = inner_state.rng.gen_range(0..100);
                            if add_prob < replacement.additional_probability_percent {
                                changing_body = replaced;
                            }
                        } else {
                            changing_body = replaced;
                        }
                    }

                    // heat it up
                    channel_cooldowns[i] += if let Some(cciph) = replacement.custom_cooldown_increase_per_hit {
                        cciph
                    } else {
                        self.cooldown_increase_per_hit
                    };
                } else {
                    // cool it down
                    channel_cooldowns[i] -= 1;
                }
            } else {
                // no cooldowns
                if replacement.additional_probability_percent < 100 {
                    let add_prob = inner_state.rng.gen_range(0..100);
                    if add_prob < replacement.additional_probability_percent {
                        changing_body = replaced;
                    }
                } else {
                    changing_body = replaced;
                }
            }
        }

        if &changing_body == original_body {
            return;
        }

        // do not trigger output logic if Serious Mode is active
        // (but do count against cooldown values)
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        let main_prob = inner_state.rng.gen_range(0..100);
        if main_prob < self.probability_percent {
            debug!("{} < {}; posting {:?}", main_prob, self.probability_percent, changing_body);
            send_channel_message!(
                interface,
                channel_name,
                &changing_body,
            ).await;
        } else {
            debug!("{} >= {}; not posting {:?}", main_prob, self.probability_percent, changing_body);
        }
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        if command.name == "allocool" {
            self.private_command_allocool(private_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "allocool" {
            Some(include_str!("../help/allocool.md").to_owned())
        } else {
            None
        }
    }
}
