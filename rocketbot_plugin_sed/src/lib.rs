mod commands;
mod parsing;


use std::collections::HashMap;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use json::JsonValue;
use log::info;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;

use crate::commands::Transformer;
use crate::parsing::parse_replacement_commands;


pub struct SedPlugin {
    interface: Weak<dyn RocketBotInterface>,

    remember_last_messages: usize,
    max_result_length: usize,
    result_too_long_message: String,

    channel_name_to_last_messages: Mutex<HashMap<String, Vec<ChannelMessage>>>,
}
impl SedPlugin {
    async fn handle_replacement_command(&self, interface: Arc<dyn RocketBotInterface>, channel_message: ChannelMessage) -> bool {
        let transformations = match parse_replacement_commands(&channel_message.message.raw) {
            Some(sc) => sc,
            None => {
                // something that didn't even look like sed commands
                return false;
            },
        };

        if transformations.len() == 0 {
            // something that looked like sed commands but didn't work
            return true;
        }

        if channel_message.message.is_by_bot {
            // avoid botloops
            return true;
        }

        let last_messages = {
            // find the message to perform a replacement in
            let messages_guard = self.channel_name_to_last_messages
                .lock().await;
            match messages_guard.get(&channel_message.channel.name) {
                Some(lm) => lm.clone(),
                None => {
                    // no last bodies for this channel; never mind
                    return true;
                }
            }
        };

        let mut found_any = false;
        for last_message in last_messages {
            let mut replaced = last_message.message.raw.clone();

            for transformation in &transformations {
                replaced = transformation.transform(&replaced);
            }

            if replaced != last_message.message.raw {
                // success!
                if self.max_result_length > 0 && replaced.len() > self.max_result_length {
                    replaced = self.result_too_long_message.clone();
                }

                interface.send_channel_message(
                    &channel_message.channel.name,
                    &replaced,
                ).await;

                found_any = true;
                break;
            }
        }

        if !found_any {
            info!(
                "no recent messages found to match transformations {}",
                channel_message.message.raw,
            );
        }

        true
    }

    async fn remember_message(&self, channel_message: &ChannelMessage) {
        let mut messages_guard = self.channel_name_to_last_messages
            .lock().await;
        let last_messages = messages_guard
            .entry(channel_message.channel.name.clone())
            .or_insert_with(|| Vec::new());

        last_messages.insert(0, channel_message.clone());
        while last_messages.len() > self.remember_last_messages && last_messages.len() > 0 {
            last_messages.remove(last_messages.len() - 1);
        }
    }
}
#[async_trait]
impl RocketBotPlugin for SedPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        let remember_last_messages = config["remember_last_messages"].as_usize().unwrap_or(50);
        let max_result_length = config["max_result_length"].as_usize().unwrap_or(1024);
        let result_too_long_message = config["result_too_long_message"].as_str()
            .unwrap_or("(sorry, that's too long)").to_owned();

        let channel_name_to_last_messages = Mutex::new(
            "SedPlugin::channel_name_to_last_message",
            HashMap::new(),
        );

        SedPlugin {
            interface,

            remember_last_messages,
            max_result_length,
            result_too_long_message,

            channel_name_to_last_messages,
        }
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if self.handle_replacement_command(Arc::clone(&interface), channel_message.clone()).await {
            return;
        }

        if !interface.is_my_user_id(&channel_message.message.sender.id).await {
            self.remember_message(&channel_message).await;
        }
    }

    async fn get_additional_commands_usages(&self) -> HashMap<String, (String, String)> {
        let mut ret = HashMap::new();
        ret.insert(
            "s".to_owned(),
            (
                "s/old/new/".to_owned(),
                "Replaces `old` with `new` in the most recent matching message. Type `{cpfx}help s` for details.".to_owned(),
            ),
        );
        ret.insert(
            "tr".to_owned(),
            (
                "tr/abc/def/".to_owned(),
                "Transposes letters `abc` to letters `def` in the most recent matching message. Type `{cpfx}help tr` for details.".to_owned(),
            ),
        );
        ret
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "s" {
            Some(include_str!("../help/s.md").to_owned())
        } else if command_name == "tr" {
            Some(include_str!("../help/tr.md").to_owned())
        } else {
            None
        }
    }
}
