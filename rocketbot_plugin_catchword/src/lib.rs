use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use log::debug;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use regex::{Match, Regex};
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;
use rocketbot_regex_replace::ReplacerRegex;
use serde_json;


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


pub struct CatchwordPlugin {
    interface: Weak<dyn RocketBotInterface>,
    catchments: HashMap<String, Vec<Replacement>>,
    rng: Mutex<StdRng>,
}
#[async_trait]
impl RocketBotPlugin for CatchwordPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> CatchwordPlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut catchments = HashMap::new();

        for (catch_name, catch_configs) in config["catchments"].entries().expect("catchments is not an object") {
            let mut replacements = Vec::new();

            for repl_config in catch_configs.members().expect("catchments entry is not an array") {
                let regex_str = repl_config["regex_string"]
                    .as_str().expect("regex missing or not a string");
                let regex = Regex::new(regex_str)
                    .expect("failed to parse regex");
                let replacement_string = repl_config["replacement_string"]
                    .as_str().expect("replacement_string missing or not a string")
                    .to_owned();
                let skip_chance_percent = if repl_config.has_key("skip_chance_percent") {
                    repl_config["skip_chance_percent"]
                        .as_u8().expect("skip_chance_percent missing or not representable as u8")
                } else {
                    0
                };

                let replacer_regex = ReplacerRegex::compile_new(regex, replacement_string)
                    .expect("failed to compile replacer regex");

                replacements.push(Replacement::new(
                    replacer_regex,
                    skip_chance_percent,
                ));
            }

            catchments.insert(catch_name.to_owned(), replacements);
        }

        for catch_name in catchments.keys() {
            my_interface.register_channel_command(&CommandDefinition::new(
                catch_name.clone(),
                "catchword".to_owned(),
                Some(HashSet::new()),
                HashMap::new(),
                0,
                format!("{{cpfx}}{} PHRASE", catch_name),
                "Performs replacements in the PHRASE according to preconfigured rules.".to_owned(),
            )).await;
        }

        let rng = Mutex::new(
            "CatchwordPlugin::rng",
            StdRng::from_entropy(),
        );

        CatchwordPlugin {
            interface,
            catchments,
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

        let replacements = match self.catchments.get(&command.name) {
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

        interface.send_channel_message(
            &channel_message.channel.name,
            &ret,
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if self.catchments.contains_key(command_name) {
            Some(include_str!("../help/catchment.md").to_owned())
        } else {
            None
        }
    }
}
