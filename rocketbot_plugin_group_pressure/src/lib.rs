use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;
use serde_json;


pub struct GroupPressurePlugin {
    interface: Weak<dyn RocketBotInterface>,
    channel_name_to_recent_messages: Mutex<HashMap<String, VecDeque<ChannelMessage>>>,
    message_remember_count: usize,
    trigger_count: usize,
}
#[async_trait]
impl RocketBotPlugin for GroupPressurePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let channel_name_to_recent_messages = Mutex::new(
            "GroupPressurePlugin::channel_name_to_recent_messages",
            HashMap::new(),
        );
        let message_remember_count = config["message_remember_count"]
            .as_usize().expect("message_remember_count missing or not representable as a usize");
        let trigger_count = config["trigger_count"]
            .as_usize().expect("trigger_count missing or not representable as a usize");

        GroupPressurePlugin {
            interface,
            channel_name_to_recent_messages,
            message_remember_count,
            trigger_count,
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

        let mut recent_messages_guard = self.channel_name_to_recent_messages
            .lock().await;

        let recent_messages_queue = recent_messages_guard
            .entry(channel_message.channel.name.clone())
            .or_insert_with(|| VecDeque::with_capacity(self.message_remember_count + 1));

        // have enough people said the same message?
        let mut usernames_said = HashSet::new();
        usernames_said.insert(channel_message.message.sender.username.clone());
        for rm in recent_messages_queue.iter() {
            if rm.message.raw == channel_message.message.raw {
                usernames_said.insert(rm.message.sender.username.clone());
            }
        }

        if usernames_said.len() >= self.trigger_count {
            // yes

            // do not output anything if Serious Mode is active
            // (but do remember it)
            let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
            if let Some(ts) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
                if ts < Local::now().timestamp() {
                    return;
                }
            }

            // remove matching messages from the queue
            recent_messages_queue
                .retain(|m| m.message.raw != channel_message.message.raw);

            // add to the fray!
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                raw_message,
            ).await;
        } else {
            // no (not yet?)

            // add this message to the queue
            recent_messages_queue.push_back(channel_message.clone());

            // dare to forget
            while recent_messages_queue.len() > self.message_remember_count {
                recent_messages_queue.pop_front();
            }
        }
    }
}
