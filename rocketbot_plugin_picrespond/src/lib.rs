use std::fs::File;
use std::io::Read;
use std::ops::DerefMut;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use chrono::Local;
use rand::{Rng, RngCore, SeedableRng};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use regex::Regex;
use rocketbot_interface::ResultExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachmentBuilder};
use rocketbot_interface::sync::{Mutex, RwLock};
use tracing::{debug, error};


#[derive(Debug)]
struct Response {
    pub regex: Regex,
    pub file_name: String,
    pub response_paths: Vec<String>,
    pub percentage: f64,
}


#[derive(Debug)]
struct Config {
    responses: Vec<Response>,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ShuffleState {
    pub shuffled_indexes: Vec<usize>,
    pub progress: usize,
}


pub struct PicRespondPlugin {
    interface: Weak<dyn RocketBotInterface>,
    rng: Arc<Mutex<Box<dyn RngCore + Send>>>,
    config: RwLock<Config>,
    shuffle_states: Mutex<Vec<ShuffleState>>,
}
impl PicRespondPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut responses = Vec::new();
        for (key, values) in config["responses"].as_object().ok_or("responses not an object")? {
            let regex = Regex::new(key).or_msg("failed to parse regex")?;
            let file_name = values["file_name"]
                .as_str().unwrap_or("picture")
                .to_owned();
            let percentage = if values["percentage"].is_null() {
                100.0
            } else {
                values["percentage"].as_f64().ok_or("percentage not a float")?
            };
            let mut response_paths: Vec<String> = Vec::new();
            let path_values = values["paths"]
                .as_array().ok_or("paths not an array")?;
            for path_val in path_values {
                response_paths.push(
                    path_val
                        .as_str().ok_or("paths entry not a string")?
                        .to_owned()
                );
            }
            if response_paths.len() == 0 {
                error!("responses value for key {:?} has no entries", key);
                return Err("responses value has no entries");
            }
            responses.push(Response {
                regex,
                file_name,
                response_paths,
                percentage,
            });
        }

        Ok(Config {
            responses,
        })
    }

    fn get_shuffle_states(responses: &[Response]) -> Vec<ShuffleState> {
        let mut ret = Vec::with_capacity(responses.len());
        for response in responses {
            let shuffled_indexes: Vec<usize> = (0..response.response_paths.len()).collect();
            ret.push(ShuffleState {
                shuffled_indexes,
                progress: 0,
            });
        }
        ret
    }
}
#[async_trait]
impl RocketBotPlugin for PicRespondPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let mut rng_box: Box<dyn RngCore + Send> = Box::new(StdRng::from_entropy());

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        let mut shuffle_states = Self::get_shuffle_states(&config_object.responses);
        for shuffle_state in &mut shuffle_states {
            shuffle_state.shuffled_indexes.shuffle(&mut rng_box);
        }

        let rng = Arc::new(Mutex::new(
            "PicRespondPlugin::rng",
            rng_box,
        ));
        let config_lock = RwLock::new(
            "PicRespondPlugin::config",
            config_object,
        );
        let shuffle_states_lock = Mutex::new(
            "PicRespondPlugin::shuffle_states",
            shuffle_states,
        );

        Self {
            interface,
            rng,
            config: config_lock,
            shuffle_states: shuffle_states_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "picrespond".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let raw_message = match &channel_message.message.raw {
            Some(rm) => rm.as_str(),
            None => return,
        };

        // do not trigger if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        let config_guard = self.config.read().await;

        for (i, response) in config_guard.responses.iter().enumerate() {
            if !response.regex.is_match(raw_message) {
                continue;
            }

            let resp_path = {
                let mut rng_guard = self.rng.lock().await;

                // do we want to respond at all?
                let my_ratio: f64 = rng_guard.gen();
                if my_ratio * 100.0 >= response.percentage {
                    // no
                    return;
                }

                // what's our index?
                let mut shuffle_guard = self.shuffle_states.lock().await;
                let my_shuffle = &mut shuffle_guard[i];
                if my_shuffle.progress >= response.response_paths.len() {
                    // re-shuffle
                    my_shuffle.shuffled_indexes.shuffle(rng_guard.deref_mut());
                    my_shuffle.progress = 0;
                }
                if my_shuffle.progress >= response.response_paths.len() {
                    // sigh, there are no entries
                    continue;
                }

                // pick the next response
                let path = &response.response_paths[my_shuffle.shuffled_indexes[my_shuffle.progress]];

                // increment for next time
                my_shuffle.progress += 1;

                path
            };

            // open and read it
            let file_bytes = {
                let mut file = File::open(resp_path).expect("failed to open file");
                let mut buf = Vec::new();
                file.read_to_end(&mut buf).expect("failed to read file");
                buf
            };

            // guess the MIME type
            let mime_type = if file_bytes.len() >= 8 && &file_bytes[0..8] == &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] {
                "image/png"
            } else if file_bytes.len() >= 3 && &file_bytes[0..3] == &[0xff, 0xd8, 0xff] {
                "image/jpeg"
            } else if file_bytes.len() >= 12 && &file_bytes[4..12] == b"ftypM4A " {
                "audio/mp4"
            } else {
                "application/octet-stream"
            };

            // send it, attached
            let mess = OutgoingMessageWithAttachmentBuilder::new(Attachment::new(
                file_bytes,
                response.file_name.clone(),
                mime_type.to_owned(),
                None,
            ))
                .build();
            debug!("sending pictorial response message for {:?}", response.regex);
            interface.send_channel_message_with_attachment(
                &channel_message.channel.name,
                mess,
            ).await;
            return;
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;

                // obtain fresh shuffle states
                let mut new_shuffle_states = Self::get_shuffle_states(&c.responses);

                // perform initial shuffle
                {
                    let mut rng_guard = self.rng.lock().await;
                    for shuffle_state in &mut new_shuffle_states {
                        shuffle_state.shuffled_indexes.shuffle(rng_guard.deref_mut());
                    }
                }

                // update shuffle states
                {
                    let mut shuffle_states_guard = self.shuffle_states.lock().await;
                    *shuffle_states_guard = new_shuffle_states;
                }

                // update config
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
