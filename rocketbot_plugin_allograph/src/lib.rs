use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use log::{debug, error};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use regex::Regex;
use rocketbot_interface::{
    JsonValueExtensions, ResultExtensions, send_channel_message, send_private_message,
};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, PrivateMessage};
use rocketbot_interface::sync::{Mutex, RwLock};
use rocketbot_regex_replace::ReplacerRegex;
use serde_json;


#[derive(Clone, Debug)]
struct LocalReplacerRegex {
    pub replacer_regex: ReplacerRegex,
    pub additional_probability_percent: u8,
    pub custom_cooldown_increase_per_hit: Option<usize>,
}
impl LocalReplacerRegex {
    pub fn new(
        replacer_regex: ReplacerRegex,
        additional_probability_percent: u8,
        custom_cooldown_increase_per_hit: Option<usize>,
    ) -> LocalReplacerRegex {
        LocalReplacerRegex {
            replacer_regex,
            additional_probability_percent,
            custom_cooldown_increase_per_hit,
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


#[derive(Clone, Debug)]
struct Config {
    probability_percent: u8,
    replacer_regexes: Vec<LocalReplacerRegex>,
    cooldown_increase_per_hit: usize,
    ignore_bot_messages: bool,
}


pub struct AllographPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
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

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let probability_percent = config["probability_percent"].as_u8()
            .ok_or("probability_percent missing or not representable as u8")?;

        let replacements = config["replacements"].members()
            .ok_or("replacements is not a list")?;
        let mut replacer_regexes: Vec<LocalReplacerRegex> = Vec::new();
        for repl in replacements {
            let regex_string = repl["regex_string"].as_str()
                .ok_or("regex_string missing or not a string")?;
            let regex = Regex::new(regex_string)
                .or_msg("failed to compile regex")?;
            let replacement_string = repl["replacement_string"].as_str()
                .ok_or("replacement_string missing or not a string")?
                .to_owned();

            let replacer_regex = ReplacerRegex::compile_new(regex, replacement_string)
                .or_msg("failed to compile replacer regex")?;

            let additional_probability_percent = if repl.has_key("additional_probability_percent") {
                repl["additional_probability_percent"].as_u8()
                    .ok_or("additional_probability_percent not representable as u8")?
            } else {
                100
            };

            let custom_cooldown_increase_per_hit = if repl.has_key("custom_cooldown_increase_per_hit") {
                Some(
                    repl["custom_cooldown_increase_per_hit"].as_usize()
                        .ok_or("custom_cooldown_increase_per_hit not representable as usize")?
                )
            } else {
                None
            };

            replacer_regexes.push(LocalReplacerRegex::new(
                replacer_regex,
                additional_probability_percent,
                custom_cooldown_increase_per_hit,
            ));
        }

        let cooldown_increase_per_hit = if config.has_key("cooldown_increase_per_hit") {
            config["cooldown_increase_per_hit"].as_usize()
                .ok_or("cooldown_increase_per_hit not representable as usize")?
        } else {
            0
        };
        let ignore_bot_messages = if config["ignore_bot_messages"].is_null() {
            true
        } else {
            config["ignore_bot_messages"].as_bool()
                .ok_or("ignore_bot_messages not representable as bool")?
        };

        Ok(Config {
            probability_percent,
            replacer_regexes,
            cooldown_increase_per_hit,
            ignore_bot_messages,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for AllographPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_obj = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "AllographPlugin::config",
            config_obj,
        );

        my_interface.register_private_message_command(
            &CommandDefinitionBuilder::new(
                "allocool",
                "allograph",
                "{cpfx}allocool CHANNEL",
                "Outputs current Allograph cooldowns in the given channel.",
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
            config: config_lock,
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

        let config_guard = self.config.read().await;

        if config_guard.ignore_bot_messages && channel_message.message.is_by_bot {
            return;
        }

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
            .or_insert_with(|| vec![0usize; config_guard.replacer_regexes.len()]);

        let mut lookups: HashMap<String, String> = HashMap::new();
        lookups.insert("username".to_owned(), sender_nickname.to_owned());

        let mut changing_body = original_body.clone();
        for (i, replacement) in config_guard.replacer_regexes.iter().enumerate() {
            // ensure we have all cooldowns stored
            // (we might be in the middle of a config reload)
            while channel_cooldowns.len() <= i {
                channel_cooldowns.push(0);
            }

            // perform the replacement
            let replaced = replacement.replacer_regex.replace(&changing_body, &lookups);

            if config_guard.cooldown_increase_per_hit > 0 || replacement.custom_cooldown_increase_per_hit.is_some() {
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
                        config_guard.cooldown_increase_per_hit
                    };
                } else if channel_cooldowns[i] > 0 {
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
                debug!("serious mode on; not posting {:?}", changing_body);
                return;
            }
        }

        let main_prob = inner_state.rng.gen_range(0..100);
        if main_prob < config_guard.probability_percent {
            debug!("{} < {}; posting {:?}", main_prob, config_guard.probability_percent, changing_body);
            send_channel_message!(
                interface,
                channel_name,
                &changing_body,
            ).await;
        } else {
            debug!("{} >= {}; not posting {:?}", main_prob, config_guard.probability_percent, changing_body);
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

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                // get number of replacements
                let replacement_count = c.replacer_regexes.len();

                // store new config
                {
                    let mut config_guard = self.config.write().await;
                    *config_guard = c;
                }

                // update cooldown caches
                {
                    let mut state_guard = self.inner_state.lock().await;
                    for cooldown_vec in state_guard.cooldowns_per_channel.values_mut() {
                        *cooldown_vec = vec![0; replacement_count];
                    }
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
