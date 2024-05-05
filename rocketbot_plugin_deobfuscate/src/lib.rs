mod decoders;


use std::sync::{Arc, Weak};

use async_trait::async_trait;
use chrono::Local;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rocketbot_interface::{JsonValueExtensions, ResultExtensions, send_channel_message};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::sync::Mutex;
use rocketbot_spelling::{HunspellEngine, SpellingEngine};
use tracing::error;

use crate::decoders::rot13;


static WORD_CHARS_RE: Lazy<Regex> = Lazy::new(|| Regex::new("\\w+").unwrap());


struct Config {
    ignore_regexes: Vec<Regex>,
    spelling_engine: Box<dyn SpellingEngine + Send>,
    min_different: usize,
}


pub struct DeobfuscatePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: Arc<Mutex<Config>>,
}
impl DeobfuscatePlugin {
    fn try_get_config(config: &serde_json::Value) -> Result<Config, &'static str> {
        let spelling_engine = HunspellEngine::new(config["spelling"].clone())
            .ok_or("failed to create spelling engine")?;

        let ignore_regexes_config = &config["ignore_regexes"];
        let ignore_regexes = if ignore_regexes_config.is_null() {
            Vec::new()
        } else {
            let ignore_regex_values = ignore_regexes_config
                .as_array().ok_or("ignore_regexes not an array")?;
            let mut ignore_regexes = Vec::with_capacity(ignore_regex_values.len());
            for ignore_regex_value in ignore_regex_values {
                let ignore_regex_str = ignore_regex_value
                    .as_str().ok_or("ignore_regexes element not a string")?;
                let ignore_regex = Regex::new(ignore_regex_str)
                    .or_msg("failed to parse ignore_regexes element as a regex")?;
                ignore_regexes.push(ignore_regex);
            }
            ignore_regexes
        };

        let min_different_config = &config["min_different"];
        let min_different = if min_different_config.is_null() {
            0
        } else {
            min_different_config
                .as_usize().ok_or("min_different not a usize")?
        };

        Ok(Config {
            ignore_regexes,
            min_different,
            spelling_engine: Box::new(spelling_engine),
        })
    }
}
#[async_trait]
impl RocketBotPlugin for DeobfuscatePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config = Self::try_get_config(&config)
            .expect("failed to obtain initial config");
        let config_lock = Arc::new(Mutex::new(
            "DeobfuscatePlugin::config",
            config,
        ));

        Self {
            interface,
            config: config_lock,
        }
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let raw_message = match &channel_message.message.raw {
            Some(s) => s,
            None => return,
        };
        let raw_message_clone = raw_message.clone();

        // don't trigger if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        // ignore this message?
        {
            let config_guard = self.config.lock().await;
            if config_guard.ignore_regexes.iter().any(|ir| ir.is_match(raw_message)) {
                // yes, ignore it
                return;
            }
        }

        let config_mutex = Arc::clone(&self.config);

        let replaced = tokio::task::spawn_blocking(move || {
            WORD_CHARS_RE.replace_all(&raw_message_clone, |caps: &Captures| {
                let match_str = caps.get(0).unwrap().as_str();
                let config_guard = config_mutex.blocking_lock();
                if !config_guard.spelling_engine.is_correct(match_str) {
                    // try rot13
                    let rot13d = rot13(match_str);
                    if config_guard.spelling_engine.is_correct(&rot13d) {
                        return rot13d;
                    }

                    // add other decoders here
                }
                match_str.to_owned()
            }).into_owned()
        }).await.expect("regex replacement panicked");

        // minimum difference count met?
        // FIXME: better metric for if original and replaced do not match in length (e.g. base64)
        let diff_count = raw_message.chars()
            .zip(replaced.chars())
            .filter(|(o, r)| o != r)
            .count();
        {
            let config_guard = self.config.lock().await;
            if diff_count <= config_guard.min_different {
                // too similar; ignore
                return;
            }
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &replaced,
        ).await;
    }

    async fn plugin_name(&self) -> String {
        "deobfuscate".to_owned()
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        // try obtaining updated config
        let new_config = match Self::try_get_config(&new_config) {
            Ok(nc) => nc,
            Err(e) => {
                error!("failed to load new config: {}", e);
                return false;
            },
        };

        {
            let mut config_guard = self.config.lock().await;
            *config_guard = new_config;
        }

        true
    }
}
