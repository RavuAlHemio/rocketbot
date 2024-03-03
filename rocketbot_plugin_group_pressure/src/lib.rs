use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use log::error;
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;


const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.fZ";


#[derive(Clone, Debug, Default)]
struct Config {
    message_remember_count: usize,
    trigger_count: usize,
    time_is_of_the_essence: Vec<TimeConfig>,
}

#[derive(Clone, Debug)]
struct TimeConfig {
    message_regex: Regex,
    timestamp_regex: Regex,
    unmatched_message: String,
}


pub struct GroupPressurePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    channel_name_to_recent_messages: Mutex<HashMap<String, VecDeque<ChannelMessage>>>,
    sent_responses: Mutex<HashSet<String>>,
}
impl GroupPressurePlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let message_remember_count = config["message_remember_count"]
            .as_usize().ok_or("message_remember_count missing or not representable as a usize")?;
        let trigger_count = config["trigger_count"]
            .as_usize().ok_or("trigger_count missing or not representable as a usize")?;

        let tiote_members = config["time_is_of_the_essence"]
            .members_or_empty_strict().ok_or("time_is_of_the_essence is not a list")?;
        let mut time_is_of_the_essence = Vec::new();
        for tiote_member in tiote_members {
            let message_regex_str = tiote_member["message_regex"]
                .as_str().ok_or("time_is_of_the_essence.[].message_regex is not a string")?;
            let message_regex = Regex::new(message_regex_str)
                .map_err(|_| "time_is_of_the_essence.[].message_regex is invalid")?;
            let timestamp_regex_str = tiote_member["timestamp_regex"]
                .as_str().ok_or("time_is_of_the_essence.[].timestamp_regex is not a string")?;
            let timestamp_regex = Regex::new(timestamp_regex_str)
                .map_err(|_| "time_is_of_the_essence.[].timestamp_regex is invalid")?;
            let unmatched_message = tiote_member["timestamp_regex"]
                .as_str().ok_or("time_is_of_the_essence.[].unmatched_message is not a string")?
                .to_owned();
            time_is_of_the_essence.push(TimeConfig {
                message_regex,
                timestamp_regex,
                unmatched_message,
            });
        }

        Ok(Config {
            message_remember_count,
            trigger_count,
            time_is_of_the_essence,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for GroupPressurePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let channel_name_to_recent_messages = Mutex::new(
            "GroupPressurePlugin::channel_name_to_recent_messages",
            HashMap::new(),
        );
        let sent_responses = Mutex::new(
            "GroupPressurePlugin::sent_responses",
            HashSet::new(),
        );

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "GroupPressurePlugin::config",
            config_object,
        );

        GroupPressurePlugin {
            interface,
            channel_name_to_recent_messages,
            config: config_lock,
            sent_responses,
        }
    }

    async fn plugin_name(&self) -> String {
        "group_pressure".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let raw_message = match &channel_message.message.raw {
            Some(rm) => rm,
            None => return, // no group pressure for non-textual messages
        };
        if raw_message.len() == 0 {
            // no group pressure for empty (e.g. attachment-only) messages
            return;
        }

        let config_guard = self.config.read().await;
        let mut recent_messages_guard = self.channel_name_to_recent_messages
            .lock().await;

        let recent_messages_queue = recent_messages_guard
            .entry(channel_message.channel.name.clone())
            .or_insert_with(|| VecDeque::with_capacity(config_guard.message_remember_count + 1));

        // have enough people said the same message?
        let mut usernames_said = HashSet::new();
        let mut timestamps_said = HashSet::new();
        usernames_said.insert(channel_message.message.sender.username.clone());
        timestamps_said.insert(channel_message.message.timestamp);
        for rm in recent_messages_queue.iter() {
            if rm.message.raw == channel_message.message.raw {
                usernames_said.insert(rm.message.sender.username.clone());
                timestamps_said.insert(rm.message.timestamp);
            }
        }

        if usernames_said.len() >= config_guard.trigger_count {
            // yes

            // do not output anything if Serious Mode is active
            // (but do remember it)
            let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
            if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
                if serious_mode_until > Local::now().timestamp() {
                    return;
                }
            }

            // remove matching messages from the queue
            recent_messages_queue
                .retain(|m| m.message.raw != channel_message.message.raw);

            // add to the fray!
            let new_message_id_opt = send_channel_message!(
                interface,
                &channel_message.channel.name,
                raw_message,
            ).await;
            if let Some(new_message_id) = new_message_id_opt {
                // are we interested in this message?
                // (the contents and all the timestamps match)
                let remember_this = config_guard.time_is_of_the_essence
                    .iter()
                    .any(|tiote| {
                        let contents_match = tiote.message_regex.is_match(raw_message);
                        let timestamps_match = timestamps_said.iter().all(|ts|
                            tiote.timestamp_regex.is_match(&ts.format(TIMESTAMP_FORMAT).to_string())
                        );
                        contents_match && timestamps_match
                    });
                if remember_this {
                    let mut sent_responses_guard = self.sent_responses
                        .lock().await;
                    sent_responses_guard.insert(new_message_id);
                }
            }
        } else {
            // no (not yet?)

            // add this message to the queue
            recent_messages_queue.push_back(channel_message.clone());

            // dare to forget
            while recent_messages_queue.len() > config_guard.message_remember_count {
                recent_messages_queue.pop_front();
            }
        }
    }

    async fn channel_message_delivered(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut sent_responses_guard = self.sent_responses.lock().await;
        let message_is_interesting = sent_responses_guard.remove(&channel_message.message.id);
        if !message_is_interesting {
            return;
        }

        let Some(raw_message) = channel_message.message.raw.as_ref() else { return };

        let config_guard = self.config.read().await;
        for tiote in &config_guard.time_is_of_the_essence {
            if !tiote.message_regex.is_match(raw_message) {
                continue;
            }

            let timestamp_string = channel_message.message.timestamp.format(TIMESTAMP_FORMAT).to_string();
            if tiote.timestamp_regex.is_match(&timestamp_string) {
                continue;
            }

            // the message is the expected one; the timestamp is wrong
            // complain
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &tiote.unmatched_message,
            ).await;
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
