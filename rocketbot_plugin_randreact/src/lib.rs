use std::ops::DerefMut;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use rand::{Rng, RngCore, SeedableRng};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;


pub struct RandReactPlugin {
    interface: Weak<dyn RocketBotInterface>,
    rng: Arc<Mutex<Box<dyn RngCore + Send>>>,
    probability: f64,
    specific_emoji: bool,
}
#[async_trait]
impl RocketBotPlugin for RandReactPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let probability = config["probability"]
            .as_f64().expect("probability not an f64");

        let specific_emoji = if config["specific_emoji"].is_null() {
            true
        } else {
            config["specific_emoji"]
                .as_bool().expect("specific_emoji not a bool")
        };

        let rng_box: Box<dyn RngCore + Send> = Box::new(StdRng::from_entropy());
        let rng = Arc::new(Mutex::new(
            "RandReactPlugin::rng",
            rng_box,
        ));

        Self {
            interface,
            rng,
            probability,
            specific_emoji,
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

        let emoji_short_name = {
            let mut rng_guard = self.rng.lock().await;

            // become active?
            let f: f64 = rng_guard.gen();
            if f >= self.probability {
                return;
            }

            let mut all_emoji = interface.obtain_emoji().await;
            if !self.specific_emoji {
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
}
