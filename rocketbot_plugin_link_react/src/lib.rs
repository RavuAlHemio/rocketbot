use std::sync::Weak;

use async_trait::async_trait;
use regex::Regex;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::message::collect_urls;
use rocketbot_interface::model::ChannelMessage;
use serde_json;


#[derive(Clone, Debug)]
struct Reaction {
    link_pattern: Regex,
    reaction_names: Vec<String>,
}
impl Reaction {
    pub fn new(
        link_pattern: Regex,
        reaction_names: Vec<String>,
    ) -> Self {
        Self {
            link_pattern,
            reaction_names,
        }
    }
}


pub struct LinkReactPlugin {
    interface: Weak<dyn RocketBotInterface>,
    reactions: Vec<Reaction>,
}
#[async_trait]
impl RocketBotPlugin for LinkReactPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let reaction_configs = config["reactions"].as_array()
            .expect("reactions is not an array");

        let mut reactions = Vec::with_capacity(reaction_configs.len());
        for reaction_config_value in reaction_configs {
            let reaction_config = reaction_config_value.as_object()
                .expect("element of reactions is not an object");

            let link_pattern_str = reaction_config
                .get("link_pattern").expect("link_pattern is missing")
                .as_str().expect("link_pattern is not a string");
            let link_pattern = Regex::new(link_pattern_str)
                .expect("failed to parse link_pattern");

            let reaction_names_values = reaction_config
                .get("reaction_names").expect("reaction_names is missing")
                .as_array().expect("reaction_names is not a list");
            let mut reaction_names = Vec::with_capacity(reaction_names_values.len());
            for reaction_name_value in reaction_names_values {
                let reaction_name = reaction_name_value
                    .as_str().expect("element of reaction_names is not a string");
                reaction_names.push(reaction_name.to_owned());
            }

            reactions.push(Reaction::new(
                link_pattern,
                reaction_names,
            ));
        }

        Self {
            interface,
            reactions,
        }
    }

    async fn plugin_name(&self) -> String {
        "link_react".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        // collect URLs from channel message
        let urls = collect_urls(channel_message.message.parsed.iter());

        // look for a match
        for url in &urls {
            for reaction in &self.reactions {
                if reaction.link_pattern.is_match(url) {
                    // react
                    for reaction_name in &reaction.reaction_names {
                        interface.add_reaction(
                            &channel_message.message.id,
                            &reaction_name,
                        ).await;
                    }
                }
            }
        }
    }
}
