mod grammar;
mod parsing;


use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use json::JsonValue;
use rand::SeedableRng;
use rand::rngs::StdRng;
use tokio::sync::Mutex;

use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;

use crate::grammar::{GeneratorState, Rulebook, TextGenerator};
use crate::parsing::parse_grammar;


pub struct GrammarGenPlugin {
    interface: Weak<dyn RocketBotInterface>,
    grammars: HashMap<String, Rulebook>,
    rng: Arc<Mutex<StdRng>>,
}
#[async_trait]
impl RocketBotPlugin for GrammarGenPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut grammars = HashMap::new();

        // load grammars
        for grammar_path_value in config["grammars"].members() {
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
            let rulebook = parse_grammar(&grammar_str)
                .expect("failed to parse grammar");

            grammars.insert(grammar_name, rulebook);
        }

        for grammar_name in grammars.keys() {
            let this_grammar_command = CommandDefinition::new(
                grammar_name.clone(),
                None,
                HashMap::new(),
                0,
            );
            my_interface.register_channel_command(&this_grammar_command).await;
        }

        let rng = Arc::new(Mutex::new(StdRng::from_entropy()));

        GrammarGenPlugin {
            interface,
            grammars,
            rng,
        }
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

        let top_rule = match grammar.rule_definitions.get(&command.name) {
            None => return,
            Some(tr) => tr,
        };

        let conditions = command.flags.clone();

        let state = GeneratorState::new(
            grammar.clone(),
            conditions,
            Arc::clone(&self.rng),
        );

        let phrase = match top_rule.top_production.generate(&state).await {
            None => return,
            Some(s) => s,
        };
        interface.send_channel_message(
            &channel_message.channel.name,
            &phrase,
        ).await;
    }
}
