use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Channel, PrivateConversation, User};
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;
use tracing::error;


#[derive(Clone, Debug, Eq, PartialEq)]
struct TypingStatus {
    rng: StdRng,
    users_typing_in_channels: HashMap<String, HashSet<String>>,
    users_typing_in_convos: HashMap<String, HashSet<String>>,
    my_typing_channels: HashSet<String>,
    my_typing_convos: HashSet<String>,
}
impl TypingStatus {
    pub fn new() -> Self {
        let rng = StdRng::from_entropy();
        let users_typing_in_channels = HashMap::new();
        let users_typing_in_convos = HashMap::new();
        let my_typing_channels = HashSet::new();
        let my_typing_convos = HashSet::new();

        Self {
            rng,
            users_typing_in_channels,
            users_typing_in_convos,
            my_typing_channels,
            my_typing_convos,
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
struct Config {
    probability: f64,
}


pub struct SimultypePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    typing_status: Mutex<TypingStatus>,
}
impl SimultypePlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let probability = if config["probability"].is_null() {
            1.0
        } else {
            config["probability"]
                .as_f64().ok_or("probability is not representable as an f64")?
        };

        Ok(Config {
            probability,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for SimultypePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "SimultypePlugin::config",
            config_object,
        );
        let typing_status = Mutex::new(
            "SimultypePlugin::typing_status",
            TypingStatus::new(),
        );

        Self {
            interface,
            config: config_lock,
            typing_status,
        }
    }

    async fn plugin_name(&self) -> String {
        "simultype".to_owned()
    }

    async fn user_typing_status_in_channel(&self, channel: &Channel, user: &User, typing: bool) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        {
            let mut status_guard = self.typing_status
                .lock().await;

            let typing_in_here = status_guard.users_typing_in_channels.entry(channel.name.clone())
                .or_insert_with(|| HashSet::new());
            if typing {
                // another user joined the fray!
                typing_in_here.insert(user.username.clone());

                // are we typing?
                if !status_guard.my_typing_channels.contains(&channel.name) {
                    // no

                    // should we start?
                    let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
                    if let Some(serious_mode_until) = behavior_flags["srs"][&channel.id].as_i64() {
                        if serious_mode_until > Local::now().timestamp() {
                            // no, Serious Mode is active
                            return;
                        }
                    }

                    let random_value: f64 = status_guard.rng.gen();
                    if random_value < config_guard.probability {
                        // yes
                        interface.set_channel_typing_status(&channel.name, true).await;
                        status_guard.my_typing_channels.insert(channel.name.clone());
                    }
                }

                // no need to do anything if we're already typing
            } else {
                // someone stopped typing
                typing_in_here.remove(&user.username);
                if typing_in_here.len() == 0 {
                    // nobody is typing anymore
                    // am I?
                    if status_guard.my_typing_channels.remove(&channel.name) {
                        // yes; stop
                        interface.set_channel_typing_status(&channel.name, false).await;
                    }
                }
            }
        }
    }

    async fn user_typing_status_in_private_conversation(&self, conversation: &PrivateConversation, user: &User, typing: bool) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        {
            let mut status_guard = self.typing_status
                .lock().await;

            let typing_in_here = status_guard.users_typing_in_convos.entry(conversation.id.clone())
                .or_insert_with(|| HashSet::new());
            if typing {
                typing_in_here.insert(user.username.clone());
                if !status_guard.my_typing_convos.contains(&conversation.id) {
                    let random_value: f64 = status_guard.rng.gen();
                    if random_value < config_guard.probability {
                        interface.set_private_conversation_typing_status(&conversation.id, true).await;
                        status_guard.my_typing_convos.insert(conversation.id.clone());
                    }
                }
            } else {
                typing_in_here.remove(&user.username);
                if typing_in_here.len() == 0 {
                    if status_guard.my_typing_convos.remove(&conversation.id) {
                        interface.set_private_conversation_typing_status(&conversation.id, false).await;
                    }
                }
            }
        }
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
