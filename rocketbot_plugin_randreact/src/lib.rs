use std::ops::DerefMut;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use chrono::Local;
use rand::{Rng, RngCore, SeedableRng};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use tracing::error;


#[derive(Clone, Copy, Debug, PartialEq)]
struct Config {
    probability: f64,
    specific_emoji: bool,
}


pub struct RandReactPlugin {
    interface: Weak<dyn RocketBotInterface>,
    rng: Arc<Mutex<Box<dyn RngCore + Send>>>,
    config: RwLock<Config>,
}
impl RandReactPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let probability = config["probability"]
            .as_f64().ok_or("probability not an f64")?;

        let specific_emoji = if config["specific_emoji"].is_null() {
            true
        } else {
            config["specific_emoji"]
                .as_bool().ok_or("specific_emoji not a bool")?
        };

        Ok(Config {
            probability,
            specific_emoji,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for RandReactPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "RandReactPlugin::config",
            config_object,
        );

        let rng_box: Box<dyn RngCore + Send> = Box::new(StdRng::from_entropy());
        let rng = Arc::new(Mutex::new(
            "RandReactPlugin::rng",
            rng_box,
        ));

        Self {
            interface,
            rng,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "randreact".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        // do not trigger if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        let config_guard = self.config.read().await;

        let emoji_short_name = {
            let mut rng_guard = self.rng.lock().await;

            // become active?
            let f: f64 = rng_guard.gen();
            if f >= config_guard.probability {
                return;
            }

            let mut all_emoji = interface.obtain_emoji().await;
            if !config_guard.specific_emoji {
                all_emoji.retain(|e| !e.is_specific);
            }
            match all_emoji.choose(&mut rng_guard.deref_mut()) {
                Some(e) => e.short_name.clone(),
                None => return,
            }
        };

        interface.add_reaction(
            &channel_message.message.id,
            &emoji_short_name,
        ).await;
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;
                *config_guard = c;
                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
        }
    }
}
