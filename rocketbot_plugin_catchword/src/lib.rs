use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use regex::{Match, Regex};
use rocketbot_interface::{JsonValueExtensions, ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use rocketbot_regex_replace::ReplacerRegex;
use serde_json;
use tracing::{debug, error};


#[derive(Clone, Debug)]
struct Replacement {
    regex: ReplacerRegex,
    skip_chance_percent: u8,
}
impl Replacement {
    pub fn new(
        regex: ReplacerRegex,
        skip_chance_percent: u8,
    ) -> Replacement {
        Replacement {
            regex,
            skip_chance_percent,
        }
    }
}


#[derive(Clone, Debug)]
struct Config {
    catchments: HashMap<String, Vec<Replacement>>,
}


pub struct CatchwordPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Mutex<StdRng>,
}
impl CatchwordPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut catchments = HashMap::new();
        let catchment_entries = config["catchments"]
            .entries().ok_or("catchments is not an object")?;
        for (catch_name, catch_configs) in catchment_entries {
            let mut replacements = Vec::new();

            let repl_configs = catch_configs
                .members().ok_or("catchments entry is not an array")?;
            for repl_config in repl_configs {
                let regex_str = repl_config["regex_string"]
                    .as_str().ok_or("regex missing or not a string")?;
                let regex = Regex::new(regex_str)
                    .or_msg("failed to parse regex")?;
                let replacement_string = repl_config["replacement_string"]
                    .as_str().ok_or("replacement_string missing or not a string")?
                    .to_owned();
                let skip_chance_percent = if repl_config.has_key("skip_chance_percent") {
                    repl_config["skip_chance_percent"]
                        .as_u8().ok_or("skip_chance_percent missing or not representable as u8")?
                } else {
                    0
                };

                let replacer_regex = ReplacerRegex::compile_new(regex, replacement_string)
                    .or_msg("failed to compile replacer regex")?;

                replacements.push(Replacement::new(
                    replacer_regex,
                    skip_chance_percent,
                ));
            }

            catchments.insert(catch_name.to_owned(), replacements);
        }

        Ok(Config {
            catchments,
        })
    }

    async fn register_catchword_command<I: RocketBotInterface + ?Sized>(interface: &I, catch_name: &str) {
        interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                catch_name,
                "catchword",
                format!("{{cpfx}}{} PHRASE", catch_name),
                "Performs replacements in the PHRASE according to preconfigured rules.",
            )
                .build()
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for CatchwordPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> CatchwordPlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        for catch_name in config_object.catchments.keys() {
            Self::register_catchword_command(my_interface.as_ref(), catch_name).await;
        }
        let config_lock = RwLock::new(
            "CatchwordPlugin::config",
            config_object,
        );

        let rng = Mutex::new(
            "CatchwordPlugin::rng",
            StdRng::from_entropy(),
        );

        CatchwordPlugin {
            interface,
            config: config_lock,
            rng,
        }
    }

    async fn plugin_name(&self) -> String {
        "catchword".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let replacements = match config_guard.catchments.get(&command.name) {
            Some(rs) => rs,
            None => return,
        };

        let message = &command.rest;
        let mut message_index: usize = 0;
        let mut ret = String::with_capacity(message.len());
        loop {
            let mut first_match: Option<Match> = None;
            let mut first_match_regex: Option<&ReplacerRegex> = None;

            for repl in replacements {
                let m = match repl.regex.find_at(&message, message_index) {
                    None => continue,
                    Some(m) => m,
                };

                debug!("matched {:?} at {}", repl.regex, m.start());

                // RNG skippage
                if repl.skip_chance_percent > 0 {
                    let mut rng_guard = self.rng.lock().await;
                    let skip_value = rng_guard.gen_range(0..100u8);
                    if skip_value < repl.skip_chance_percent {
                        continue;
                    }
                }

                if first_match.is_none() || first_match.unwrap().start() > m.start() {
                    first_match = Some(m);
                    first_match_regex = Some(&repl.regex);
                }
            }

            if first_match.is_none() {
                // we're done; copy the rest
                ret.push_str(&message[message_index..]);
                break;
            }

            // copy verbatim between messageIndex and index of firstMatch
            ret.push_str(&message[message_index..first_match.unwrap().start()]);

            // replace only within the matched string
            let replaced_chunk = first_match_regex.unwrap()
                .replace(first_match.unwrap().as_str(), &HashMap::new());

            // add that too
            ret.push_str(&replaced_chunk);

            // walk forward
            message_index = first_match.unwrap().end();
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &ret,
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        let config_guard = self.config.read().await;
        if config_guard.catchments.contains_key(command_name) {
            Some(include_str!("../help/catchment.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let interface = match self.interface.upgrade() {
            None => return false,
            Some(i) => i,
        };

        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;

                // unregister existing commands
                for old_command in config_guard.catchments.keys() {
                    interface.unregister_channel_command(old_command).await;
                }

                // store new config
                *config_guard = c;

                // register new commands
                for catch_name in config_guard.catchments.keys() {
                    Self::register_catchword_command(interface.as_ref(), catch_name).await;
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
