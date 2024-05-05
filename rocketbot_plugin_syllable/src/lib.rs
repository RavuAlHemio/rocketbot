use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use regex::{Captures, Regex};
use rocketbot_interface::{JsonValueExtensions, ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::serde::serde_regex;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json;
use toml;
use tracing::error;


#[derive(Clone, Debug, Deserialize, Serialize)]
struct SyllableRule {
    #[serde(with = "serde_regex")] find: Regex,
    #[serde(default = "String::new")] replace: String,
    #[serde(default = "isize_one")] adjust_count: isize,
    #[serde(default)] disqualify: bool,
}

fn isize_one() -> isize { 1 }

fn calculate_syllables(text: &str, rules: &[SyllableRule]) -> Option<isize> {
    let mut current_text = text.to_owned();
    let mut count: isize = 0;
    for rule in rules {
        let mut matched = false;
        let new_text = rule.find.replace_all(&current_text, |caps: &Captures| {
            let mut dest = String::new();
            caps.expand(&rule.replace, &mut dest);
            count += rule.adjust_count;
            matched = true;
            dest
        });
        if rule.disqualify && matched {
            return None;
        }

        current_text = new_text.into_owned();
    }
    Some(count)
}


#[derive(Clone, Debug)]
struct Config {
    rules: Vec<SyllableRule>,
    detect_haiku_channel_names: HashSet<String>,
}


pub struct SyllablePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    channel_to_last_syllables: Mutex<HashMap<String, VecDeque<isize>>>,
}
impl SyllablePlugin {
    async fn channel_command_syl(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        // calculate the number of syllables
        if let Some(sc) = calculate_syllables(&command.rest, &config_guard.rules) {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("I count {} {}.", sc, if sc == 1 { "syllable" } else { "syllables" }),
            ).await;
        } else {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Value disqualified from syllable counting.",
            ).await;
        }
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let rules_path = config["rules_path"]
            .as_str().ok_or("rules_path not a string")?;
        let rules: Vec<SyllableRule> = {
            let buf = std::fs::read_to_string(rules_path)
                .or_msg("failed to read rules file")?;
            let toml_value: toml::Value = toml::from_str(&buf)
                .or_msg("failed to parse rules file")?;
            toml_value["rules"].clone().try_into()
                .or_msg("failed to decode rules")?
        };

        let detect_haiku_channel_name_values = config["detect_haiku_channel_names"]
            .members_or_empty_strict().ok_or("detect_haiku_channel_names not a list")?;
        let mut detect_haiku_channel_names: HashSet<String> = HashSet::new();
        for value in detect_haiku_channel_name_values {
            let channel_name = value
                .as_str().ok_or("detect_haiku_channel_names member not a string")?
                .to_owned();
            detect_haiku_channel_names.insert(channel_name);
        }

        Ok(Config {
            rules,
            detect_haiku_channel_names,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for SyllablePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        let channel_to_last_syllables = Mutex::new(
            "SyllablePlugin::channel_to_last_syllables",
            config_object.detect_haiku_channel_names.iter()
                .map(|cn|
                    (cn.clone(), VecDeque::with_capacity(4))
                )
                .collect(),
        );

        let config_lock = RwLock::new(
            "SyllablePlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "syl",
                "syllable",
                "{cpfx}syl TEXT",
                "Calculates the number of syllables in a text.",
            )
                .build()
        ).await;

        Self {
            interface,
            config: config_lock,
            channel_to_last_syllables,
        }
    }

    async fn plugin_name(&self) -> String {
        "syllable".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        {
            let mut guard = self.channel_to_last_syllables
                .lock().await;
            let last_syllables = match guard.get_mut(&channel_message.channel.name) {
                Some(ls) => ls,
                None => return, // we're not patrolling this channel
            };

            // calculate syllables in this message
            let syllables_opt = if let Some(r) = &channel_message.message.raw {
                calculate_syllables(r, &config_guard.rules)
            } else {
                None
            };

            // append the number
            if let Some(syllables) = syllables_opt {
                last_syllables.push_back(syllables);
            } else {
                // message is disqualified; forget everything
                last_syllables.clear();
                return;
            }

            // reduce to 3
            while last_syllables.len() > 3 {
                last_syllables.pop_front();
            }

            // check 5-7-5
            if last_syllables.len() != 3 {
                return;
            }
            if last_syllables[0] != 5 || last_syllables[1] != 7 || last_syllables[2] != 5 {
                return;
            }
        };

        // do not trigger output logic if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        // we have released the mutex; post a response
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            "Was that a haiku?"
        ).await;
    }

    async fn channel_message_delivered(&self, channel_message: &ChannelMessage) {
        // our own messages interrupt haiku
        let mut guard = self.channel_to_last_syllables
            .lock().await;
        if let Some(ctls) = guard.get_mut(&channel_message.channel.name) {
            ctls.clear();
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "syl" {
            self.channel_command_syl(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "syl" {
            Some(include_str!("../help/syl.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                {
                    // update channel-to-last-syllables
                    let mut ctls_guard = self.channel_to_last_syllables.lock().await;

                    // remove goners
                    let old_channels: Vec<String> = ctls_guard.keys().map(|c| c.clone()).collect();
                    for oc in &old_channels {
                        if !c.detect_haiku_channel_names.contains(oc) {
                            ctls_guard.remove(oc);
                        }
                    }

                    // insert new channels
                    for new_chan in &c.detect_haiku_channel_names {
                        ctls_guard
                            .entry(new_chan.clone())
                            .or_insert_with(|| VecDeque::with_capacity(4));
                    }
                }

                {
                    // update config itself
                    let mut config_guard = self.config.write().await;
                    *config_guard = c;
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


#[cfg(test)]
mod tests {
    use super::{calculate_syllables, SyllableRule};
    use regex::Regex;

    fn make_rule(find: &str, adjust_count: isize) -> SyllableRule {
        let find_regex = Regex::new(find).expect("failed to compile rule regex");
        SyllableRule {
            find: find_regex,
            replace: String::new(),
            adjust_count,
            disqualify: false,
        }
    }
    fn make_replacer(find: &str, replace: &str, adjust_count: isize) -> SyllableRule {
        let find_regex = Regex::new(find).expect("failed to compile rule regex");
        SyllableRule {
            find: find_regex,
            replace: replace.to_owned(),
            adjust_count,
            disqualify: false,
        }
    }
    fn make_disqualifier(find: &str) -> SyllableRule {
        let find_regex = Regex::new(find).expect("failed to compile rule regex");
        SyllableRule {
            find: find_regex,
            replace: String::new(),
            adjust_count: 0,
            disqualify: true,
        }
    }

    fn make_rules_de() -> Vec<SyllableRule> {
        vec![
            make_disqualifier("://"),
            make_replacer("(?i)zue", "e", 1),
            make_rule("(?i)[bdfgkmnpqstvwxz]l\\b", 1),
            make_rule("(?i)[bdfgkmnpqstvwxz]l[bcdfghjklmnpqrstvwxz]", 1),
            make_rule("(?i)[aeiouyäöü]{2}", 1),
            make_rule("(?i)[aeiouyäöü]", 1),
        ]
    }

    #[test]
    fn test_calculate_syllables() {
        let r = make_rules_de();

        // chosen at random from word list
        assert_eq!(calculate_syllables("Grüße", &r), Some(2));
        assert_eq!(calculate_syllables("fortgestoßenen", &r), Some(5));
        assert_eq!(calculate_syllables("unzensiertes", &r), Some(4));
        assert_eq!(calculate_syllables("unausgeglichenere", &r), Some(7));
        assert_eq!(calculate_syllables("herunterbringen", &r), Some(5));
        assert_eq!(calculate_syllables("Buchstabenrätseln", &r), Some(5));
        assert_eq!(calculate_syllables("zusteuernder", &r), Some(4));
        assert_eq!(calculate_syllables("Ehebruchs", &r), Some(3));
        assert_eq!(calculate_syllables("gezirkeltem", &r), Some(4));
        assert_eq!(calculate_syllables("unterziehen", &r), Some(4));

        assert_eq!(calculate_syllables("Signanz", &r), Some(2));

        // pathological case: syllabizing -xl
        assert_eq!(calculate_syllables("Beidl", &r), Some(2));

        // but not here
        assert_eq!(calculate_syllables("Kerl", &r), Some(1));
        assert_eq!(calculate_syllables("Händler", &r), Some(2));

        // pathological case: many diphthongs
        assert_eq!(calculate_syllables("Haarspalterei", &r), Some(4));

        // pathological case: syllabizing -dl in the middle of a word, diphthongs
        assert_eq!(calculate_syllables("Beidlhaarspalterei", &r), Some(6));

        // pathological case: massive consonant cluster
        assert_eq!(calculate_syllables("Angstschweiß", &r), Some(2));

        // pathological case: massive vowel cluster
        assert_eq!(calculate_syllables("Teeeier", &r), Some(3));

        // pathological case: "ue" not a diphthong
        assert_eq!(calculate_syllables("zuerst", &r), Some(2));
        assert_eq!(calculate_syllables("anzuerkennen", &r), Some(5));

        // longer phrase
        assert_eq!(calculate_syllables("Österreich ist eine demokratische Republik. Ihr Recht geht vom Volk aus.", &r), Some(20));

        // disqualify URLs
        assert_eq!(calculate_syllables("check this out: https://example.com/uri", &r), None);
    }
}
