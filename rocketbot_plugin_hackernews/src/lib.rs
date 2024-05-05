use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use md5::{Digest, Md5};
use regex::Regex;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use tracing::error;


#[derive(Clone, Debug)]
struct Config {
    hacker_news_regex: Regex,
    n_gate_url: String,
    n_gate_md5: [u8; 16],
    md5_matches_message: String,
    md5_differs_message: String,
}


pub struct HackernewsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl HackernewsPlugin {
    fn load_config(config: &serde_json::Value) -> Option<Config> {
        let hacker_news_regex_str = match config["hacker_news_regex"].as_str() {
            Some(hnr) => hnr,
            None => {
                error!("hacker_news_regex missing in config or not a string");
                return None;
            },
        };
        let hacker_news_regex = match Regex::new(hacker_news_regex_str) {
            Ok(hnr) => hnr,
            Err(e) => {
                error!("failed to parse hacker_news_regex config value {:?}: {}", hacker_news_regex_str, e);
                return None;
            },
        };

        let n_gate_url: String = match &config["n_gate_url"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "http://n-gate.com/".to_owned(),
            _ => {
                error!("invalid value type for n_gate_url; must be a string");
                return None;
            },
        };

        let n_gate_md5_str = match config["n_gate_md5"].as_str() {
            Some(s) => s,
            None => {
                error!("n_gate_md5 missing in config or not a string");
                return None;
            },
        };
        if n_gate_md5_str.len() != 32 {
            error!("n_gate_md5 is {} bytes long; expected 32 (MD5 hex digest)", n_gate_md5_str.len());
            return None;
        }
        let invalid_char = n_gate_md5_str.char_indices()
            .filter(|&(_i, c)| !((c >= '0' && c <= '9') || (c >= 'A' && c <= 'F') || (c >= 'a' && c <= 'f')))
            .nth(0);
        if let Some((i, c)) = invalid_char {
            error!("n_gate_md5 contains an invalid character for a hex string: {:?} at byte position {}", c, i);
            return None;
        }
        let mut n_gate_md5 = [0u8; 16];
        for (hex_byte, md5_byte) in n_gate_md5_str.as_bytes().chunks(2).zip(n_gate_md5.iter_mut()) {
            let hex_byte_str = std::str::from_utf8(hex_byte).unwrap();
            *md5_byte = u8::from_str_radix(hex_byte_str, 16).unwrap();
        }

        let md5_matches_message = match config["md5_matches_message"].as_str() {
            Some(val) => val.to_owned(),
            None => {
                error!("md5_matches_message missing in config or not a string");
                return None;
            },
        };
        let md5_differs_message = match config["md5_differs_message"].as_str() {
            Some(val) => val.to_owned(),
            None => {
                error!("md5_differs_message missing in config or not a string");
                return None;
            },
        };

        Some(Config {
            hacker_news_regex,
            n_gate_url,
            n_gate_md5,
            md5_matches_message,
            md5_differs_message,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for HackernewsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let actual_config = Self::load_config(&config)
            .expect("failed to load config");
        let wrapped_config = RwLock::new(
            "HackernewsPlugin::config",
            actual_config,
        );

        HackernewsPlugin {
            interface,
            config: wrapped_config,
        }
    }

    async fn plugin_name(&self) -> String {
        "hackernews".to_owned()
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let new_config_object = match Self::load_config(&new_config) {
            Some(nco) => nco,
            None => {
                error!("failed to load new config");
                return false;
            },
        };

        {
            let mut config_guard = self.config.write().await;
            *config_guard = new_config_object;
        }

        true
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let body = match &channel_message.message.raw {
            Some(b) => b,
            None => return,
        };

        // don't trigger if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        let config = {
            let config_guard = self.config.read().await;
            config_guard.clone()
        };

        if !config.hacker_news_regex.is_match(body) {
            return;
        }

        // obtain page
        let response = match reqwest::get(&config.n_gate_url).await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to GET {:?}: {}", config.n_gate_url, e);
                return;
            },
        };
        let response_body = match response.bytes().await {
            Ok(rb) => rb,
            Err(e) => {
                error!("failed to get response bytes for GET {:?}: {}", config.n_gate_url, e);
                return;
            },
        };

        // calculate MD5
        let md5_digest = {
            let mut hasher = Md5::new();
            hasher.update(&response_body);
            hasher.finalize()
        };

        let outgoing_message = if md5_digest.as_slice() == config.n_gate_md5.as_slice() {
            config.md5_matches_message.as_str()
        } else {
            config.md5_differs_message.as_str()
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &outgoing_message,
        ).await;
    }
}
