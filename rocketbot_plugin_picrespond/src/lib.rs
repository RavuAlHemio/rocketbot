use std::fs::File;
use std::io::Read;
use std::ops::DerefMut;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use chrono::Local;
use log::debug;
use rand::{RngCore, SeedableRng};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use regex::Regex;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachmentBuilder};
use rocketbot_interface::sync::Mutex;


#[derive(Debug)]
struct Response {
    pub regex: Regex,
    pub response_paths: Vec<String>,
}


pub struct PicRespondPlugin {
    interface: Weak<dyn RocketBotInterface>,
    rng: Arc<Mutex<Box<dyn RngCore + Send>>>,
    responses: Vec<Response>,
}
#[async_trait]
impl RocketBotPlugin for PicRespondPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let rng_box: Box<dyn RngCore + Send> = Box::new(StdRng::from_entropy());
        let rng = Arc::new(Mutex::new(
            "PicRespondPlugin::rng",
            rng_box,
        ));

        let mut responses = Vec::new();
        for (key, pics) in config["responses"].as_object().expect("responses not an object") {
            let regex = Regex::new(key).expect("failed to parse regex");
            let response_paths: Vec<String> = pics.as_array().expect("responses value not an array")
                .iter()
                .map(|path_val| path_val.as_str().expect("responses value entry not a string").to_owned())
                .collect();
            if response_paths.len() == 0 {
                panic!("responses value for key {:?} has no entries", key);
            }
            responses.push(Response {
                regex,
                response_paths,
            });
        }

        Self {
            interface,
            rng,
            responses,
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

        for response in &self.responses {
            if !response.regex.is_match(raw_message) {
                continue;
            }

            // pick a response at random
            let resp_path = {
                let mut rng_guard = self.rng.lock().await;
                response.response_paths
                    .choose(rng_guard.deref_mut()).expect("at least one response path is available")
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
            } else {
                "application/octet-stream"
            };

            // send it, attached
            let mess = OutgoingMessageWithAttachmentBuilder::new(Attachment::new(
                file_bytes,
                "picture".to_owned(),
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
}
