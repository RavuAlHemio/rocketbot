mod commands;
mod parsing;


use std::collections::HashMap;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::info;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;
use serde_json;

use crate::commands::Transformer;
use crate::parsing::parse_replacement_commands;


pub struct SedPlugin {
    interface: Weak<dyn RocketBotInterface>,

    remember_last_messages: usize,
    max_result_length: usize,
    result_too_long_message: String,
    require_all_transformations_successful: bool,

    channel_name_to_last_messages: Mutex<HashMap<String, Vec<ChannelMessage>>>,
    channel_name_to_my_outgoing_messages: Mutex<HashMap<String, Vec<String>>>,
}
impl SedPlugin {
    async fn handle_replacement_command(&self, interface: Arc<dyn RocketBotInterface>, channel_message: ChannelMessage) -> bool {
        let raw_message = match &channel_message.message.raw {
            Some(rm) => rm,
            None => return false, // non-textual messages do not contain commands
        };
        if raw_message.len() == 0 {
            // this message is non-textual too
            return false;
        }

        let transformations = match parse_replacement_commands(&raw_message) {
            Ok(sc) => sc,
            Err(e) => {
                return if e.is_disqualifying() {
                    // something that didn't even look like sed commands
                    false
                } else {
                    // similar enough to a sed command but not valid
                    info!("failed to parse command in {:?}: {}", raw_message, e);
                    true
                };
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
        assert!(last_messages.iter().all(|m| m.message.raw.is_some()));

        let mut found_any = false;
        for last_message in last_messages {
            let last_raw_message = last_message.message.raw.unwrap();
            let mut replaced = last_raw_message.clone();

            let mut all_transformations_successful = true;
            for transformation in &transformations {
                let this_replaced = transformation.transform(&replaced);
                if this_replaced == replaced {
                    all_transformations_successful = false;
                }
                replaced = this_replaced;
            }

            if replaced != last_raw_message && (!self.require_all_transformations_successful || all_transformations_successful) {
                // success!
                if self.max_result_length > 0 && replaced.len() > self.max_result_length {
                    replaced = self.result_too_long_message.clone();
                }

                {
                    let mut outgoing_guard = self.channel_name_to_my_outgoing_messages
                        .lock().await;
                    outgoing_guard
                        .entry(channel_message.channel.name.clone())
                        .or_insert_with(|| Vec::new())
                        .push(replaced.clone());
                }

                send_channel_message!(
                    interface,
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
                raw_message,
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

    async fn channel_command_sedparse(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let output = match parse_replacement_commands(&command.rest) {
            Ok(cmds) => format!("Successfully parsed {} {}.", cmds.len(), if cmds.len() == 1 { "command" } else { "commands" }),
            Err(e) => format!("Error while parsing: {}", e),
        };
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &output,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for SedPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let remember_last_messages = config["remember_last_messages"].as_usize().unwrap_or(50);
        let max_result_length = config["max_result_length"].as_usize().unwrap_or(1024);
        let result_too_long_message = config["result_too_long_message"].as_str()
            .unwrap_or("(sorry, that's too long)").to_owned();
        let require_all_transformations_successful = config["require_all_transformations_successful"]
            .as_bool().unwrap_or(true);

        let channel_name_to_last_messages = Mutex::new(
            "SedPlugin::channel_name_to_last_message",
            HashMap::new(),
        );
        let channel_name_to_my_outgoing_messages = Mutex::new(
            "SedPlugin::channel_name_to_my_outgoing_messages",
            HashMap::new(),
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "sedparse".to_owned(),
                "sed".to_owned(),
                "{cpfx}sedparse SEDCOMMANDS".to_owned(),
                "Attempts to parse a series of sed commands and pinpoint issues.".to_owned(),
            )
                .behaviors(CommandBehaviors::NO_ARGUMENT_PARSING)
                .build()
        ).await;

        SedPlugin {
            interface,

            remember_last_messages,
            max_result_length,
            result_too_long_message,
            require_all_transformations_successful,

            channel_name_to_last_messages,
            channel_name_to_my_outgoing_messages,
        }
    }

    async fn plugin_name(&self) -> String {
        "sed".to_owned()
    }

    async fn channel_message_delivered(&self, channel_message: &ChannelMessage) {
        let raw_msg = match &channel_message.message.raw {
            Some(msg) => msg,
            None => return,
        };

        // a message sent by the bot
        // is it one sent by this plugin (as a response to a replacement command)?
        {
            let mut outgoing_guard = self.channel_name_to_my_outgoing_messages
                .lock().await;
            let outgoing_messages = outgoing_guard
                .entry(channel_message.channel.name.clone())
                .or_insert_with(|| Vec::new());
            let msg_pos_opt = outgoing_messages
                .iter()
                .position(|msg| msg == raw_msg);
            if let Some(msg_pos) = msg_pos_opt {
                // yes, it is

                // it is now accounted for
                outgoing_messages.remove(msg_pos);

                // do not remember it as a sed-able message
                return;
            }
        }

        self.remember_message(&channel_message).await;
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if self.handle_replacement_command(Arc::clone(&interface), channel_message.clone()).await {
            // it looked very much like a replacement command
            // do not remember it for further sed-ing
            return;
        }

        if channel_message.message.raw.is_some() {
            self.remember_message(&channel_message).await;
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "sedparse" {
            self.channel_command_sedparse(channel_message, command).await
        }
    }

    async fn get_additional_channel_commands_usages(&self) -> HashMap<String, (String, String)> {
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
        } else if command_name == "sedparse" {
            Some(include_str!("../help/sedparse.md").to_owned())
        } else {
            None
        }
    }
}
