use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Channel, PrivateConversation, User};
use rocketbot_interface::sync::Mutex;
use serde_json;


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


pub struct SimultypePlugin {
    interface: Weak<dyn RocketBotInterface>,
    probability: f64,
    typing_status: Mutex<TypingStatus>,
}
#[async_trait]
impl RocketBotPlugin for SimultypePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let probability = if config["probability"].is_null() {
            1.0
        } else {
            config["probability"]
                .as_f64().expect("probability is not representable as an f64")
        };
        let typing_status = Mutex::new(
            "SimultypePlugin::typing_status",
            TypingStatus::new(),
        );

        Self {
            interface,
            probability,
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
                    let random_value: f64 = status_guard.rng.gen();
                    if random_value < self.probability {
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

        {
            let mut status_guard = self.typing_status
                .lock().await;

            let typing_in_here = status_guard.users_typing_in_convos.entry(conversation.id.clone())
                .or_insert_with(|| HashSet::new());
            if typing {
                typing_in_here.insert(user.username.clone());
                if !status_guard.my_typing_convos.contains(&conversation.id) {
                    let random_value: f64 = status_guard.rng.gen();
                    if random_value < self.probability {
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
}
