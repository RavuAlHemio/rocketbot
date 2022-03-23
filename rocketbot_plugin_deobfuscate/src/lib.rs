mod decoders;


use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::error;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rocketbot_interface::send_channel_message;
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::sync::Mutex;
use rocketbot_spelling::{HunspellEngine, SpellingEngine};

use crate::decoders::rot13;


static WORD_CHARS_RE: Lazy<Regex> = Lazy::new(|| Regex::new("\\w+").unwrap());


pub struct DeobfuscatePlugin {
    interface: Weak<dyn RocketBotInterface>,
    spelling_engine: Arc<Mutex<Box<dyn SpellingEngine + Send>>>,
}
#[async_trait]
impl RocketBotPlugin for DeobfuscatePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let spelling_engine_inner = HunspellEngine::new(config["spelling"].clone())
            .expect("failed to create spelling engine");
        let spelling_engine = Arc::new(Mutex::new(
            "DeobfuscatePlugin::spelling_engine",
            Box::new(spelling_engine_inner) as Box<dyn SpellingEngine + Send>,
        ));

        Self {
            interface,
            spelling_engine,
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

        let spelling_engine_mutex = Arc::clone(&self.spelling_engine);

        let replaced = tokio::task::spawn_blocking(move || {
            WORD_CHARS_RE.replace_all(&raw_message_clone, |caps: &Captures| {
                let match_str = caps.get(0).unwrap().as_str();
                let spelling_engine_guard = spelling_engine_mutex.blocking_lock();
                if !spelling_engine_guard.is_correct(match_str) {
                    // try rot13
                    let rot13d = rot13(match_str);
                    if spelling_engine_guard.is_correct(&rot13d) {
                        return rot13d;
                    }

                    // add other decoders here
                }
                match_str.to_owned()
            }).into_owned()
        }).await.expect("regex replacement panicked");

        if raw_message != &replaced {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &replaced,
            ).await;
        }
    }

    async fn plugin_name(&self) -> String {
        "deobfuscate".to_owned()
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        // try obtaining a new spelling engine
        let new_spelling_engine_inner = match HunspellEngine::new(new_config["spelling"].clone()) {
            Some(e) => e,
            None => {
                error!("failed to load new config: failed to create new Hunspell engine");
                return false;
            },
        };

        {
            let mut spelling_engine_guard = self.spelling_engine.lock().await;
            *spelling_engine_guard = Box::new(new_spelling_engine_inner) as Box<dyn SpellingEngine + Send>;
        }

        true
    }
}
