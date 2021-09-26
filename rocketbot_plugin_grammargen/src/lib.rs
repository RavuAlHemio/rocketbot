pub mod grammar;
pub mod parsing;


use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;

use crate::grammar::{GeneratorState, Metacommand, Rulebook};
use crate::parsing::parse_grammar;


pub struct GrammarGenPlugin {
    interface: Weak<dyn RocketBotInterface>,
    grammars: HashMap<String, Rulebook>,
    grammar_to_allowed_channel_names: HashMap<String, Option<HashSet<String>>>,
    word_joiner_in_nicknames: bool,
}
#[async_trait]
impl RocketBotPlugin for GrammarGenPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut grammars = HashMap::new();

        // load grammars
        for grammar_path_value in config["grammars"].members().expect("grammars not a list") {
            let grammar_path_str = grammar_path_value
                .as_str().expect("grammar path not a string");
            let grammar_path = PathBuf::from(grammar_path_str);

            let grammar_name = grammar_path.file_stem()
                .expect("grammar name cannot be derived from file name")
                .to_str()
                .expect("grammar name is not valid Unicode")
                .to_owned();

            let grammar_str = {
                let mut grammar_file = File::open(&grammar_path)
                    .expect("failed to open grammar file");

                let mut grammar_string = String::new();
                grammar_file.read_to_string(&mut grammar_string)
                    .expect("failed to read grammar file");

                grammar_string
            };

            // parse the string
            let rulebook = parse_grammar(&grammar_name, &grammar_str)
                .expect("failed to parse grammar");

            grammars.insert(grammar_name, rulebook);
        }

        let mut grammar_to_allowed_channel_names = HashMap::new();
        if !config["grammar_to_allowed_channel_names"].is_null() {
            for (grammar_name, channels) in config["grammar_to_allowed_channel_names"].entries().expect("grammar_to_allowed_channel_names not an object") {
                if channels.is_null() {
                    grammar_to_allowed_channel_names.insert(grammar_name.clone(), None);
                } else {
                    let mut channel_names = HashSet::new();
                    for entry in channels.members().expect("grammar_to_allowed_channel_names member value not a list") {
                        let channel_name = entry
                            .as_str().expect("grammar_to_allowed_channel_names member value entry not a string")
                            .to_owned();
                        channel_names.insert(channel_name);
                    }
                    grammar_to_allowed_channel_names.insert(grammar_name.clone(), Some(channel_names));
                }
            }
        }

        let word_joiner_in_nicknames = if config["word_joiner_in_nicknames"].is_null() {
            false
        } else {
            config["word_joiner_in_nicknames"].as_bool()
                .expect("word_joiner_in_nicknames not a bool")
        };

        for grammar_name in grammars.keys() {
            let this_grammar_command = CommandDefinition::new(
                grammar_name.clone(),
                "grammargen".to_owned(),
                None,
                HashMap::new(),
                0,
                CommandBehaviors::empty(),
                format!("{{cpfx}}{} [NICKNAME]", grammar_name),
                "Produces a phrase from the given grammar.".to_owned(),
            );
            my_interface.register_channel_command(&this_grammar_command).await;
        }

        GrammarGenPlugin {
            interface,
            grammars,
            grammar_to_allowed_channel_names,
            word_joiner_in_nicknames,
        }
    }

    async fn plugin_name(&self) -> String {
        "grammargen".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let grammar = match self.grammars.get(&command.name) {
            None => return,
            Some(g) => g,
        };

        // allowed in this channel?
        if let Some(allowed_channels) = self.grammar_to_allowed_channel_names.get(&command.name) {
            // the grammar has been mentioned in the config file
            if let Some(ac) = allowed_channels {
                // the grammar has been restricted to specific channels
                if !ac.contains(&channel_message.channel.name) {
                    // the grammar is not allowed in this channel
                    return;
                }
            }
        }
        // assume that grammars not mentioned in the config file are allowed everywhere

        let chosen_nick_opt = if command.rest.len() > 0 {
            Some(command.rest.as_str())
        } else {
            None
        };
        let channel_users_opt = interface
            .obtain_users_in_channel(&channel_message.channel.name).await;
        let channel_users = match channel_users_opt {
            Some(cu) => cu,
            None => {
                error!("no user list for channel {}", channel_message.channel.name);
                return;
            },
        };
        let channel_nicks: HashSet<String> = channel_users.iter()
            .map(|u|
                if self.word_joiner_in_nicknames && u.username.len() > 1 {
                    let mut nick_chars: Vec<char> = u.username.chars().collect();
                    nick_chars.insert(1, '\u{2060}');
                    nick_chars.iter().collect()
                } else {
                    u.username.clone()
                }
            )
            .collect();

        let mut my_grammar = grammar.clone();
        my_grammar.add_builtins(&channel_nicks, chosen_nick_opt);

        let mut rng = StdRng::from_entropy();
        let mut conditions = HashSet::new();

        // process metacommands
        {
            for metacommand in &my_grammar.metacommands {
                match metacommand {
                    Metacommand::RandomizeCondition(cond) => {
                        let activate_condition: bool = rng.gen();
                        if activate_condition {
                            conditions.insert(cond.clone());
                        }
                    },
                }
            }
        }

        for flag in &command.flags {
            conditions.insert(format!("opt_{}", flag));
        }

        let start_production = my_grammar.rule_definitions
            .get(&command.name).unwrap()
            .top_production;
        let mut state = GeneratorState::new_topmost(
            my_grammar,
            start_production,
            conditions,
            rng,
        );

        let phrase = match crate::grammar::generate(&mut state) {
            Ok(s) => s,
            Err(e) => {
                error!("failed to generate {:?}: {}", command.name, e);
                return;
            },
        };
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &phrase,
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if self.grammars.contains_key(command_name) {
            Some(include_str!("../help/grammargen.md").to_owned())
        } else {
            None
        }
    }
}
