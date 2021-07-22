use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ImpersonationInfo, OutgoingMessage, PrivateMessage};
use serde_json;


pub struct SockpuppetPlugin {
    interface: Weak<dyn RocketBotInterface>,
    allowed_usernames: HashSet<String>,
}
#[async_trait]
impl RocketBotPlugin for SockpuppetPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut allowed_usernames = HashSet::new();
        for username_value in config["allowed_usernames"].members().expect("allowed_usernames not a list") {
            let username = username_value
                .as_str().expect("entry in allowed_usernames not a string");
            allowed_usernames.insert(username.to_owned());
        }

        let mut chansend_options = HashMap::new();
        chansend_options.insert("impersonate".to_owned(), CommandValueType::String);
        let chansend_command = CommandDefinition::new(
            "chansend".to_owned(),
            "sockpuppet".to_owned(),
            Some(HashSet::new()),
            chansend_options,
            1,
            "{cpfx}chansend [{lopfx}impersonate USERNAME] CHANNEL MESSAGE".to_owned(),
            "Sends a message, pretending to be the bot or someone else.".to_owned(),
        );
        my_interface.register_private_message_command(&chansend_command).await;

        SockpuppetPlugin {
            interface,
            allowed_usernames,
        }
    }

    async fn plugin_name(&self) -> String {
        "sockpuppet".to_owned()
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "chansend" {
            return;
        }

        if !self.allowed_usernames.contains(&private_message.message.sender.username) {
            return;
        }

        let channel_name = command.args[0].clone();
        let message_body = command.rest.clone();

        let impersonation = if let Some(imp_username_val) = command.options.get("impersonate") {
            let imp_username = imp_username_val.as_str().expect("--impersonate value is string");

            let channel_users_opt = interface.obtain_users_in_channel(&channel_name).await;
            if let Some(channel_users) = channel_users_opt {
                let target_user_opt = channel_users.iter()
                    .filter(|u| u.username == imp_username)
                    .nth(0);
                if let Some(target_user) = target_user_opt {
                    let avatar_frag = percent_encode(&target_user.username.as_bytes(), NON_ALPHANUMERIC);
                    Some(ImpersonationInfo::new(
                        format!("/avatar/{}", avatar_frag),
                        target_user.nickname_or_username().to_owned(),
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let outgoing_message = OutgoingMessage::new(
            message_body,
            impersonation,
        );
        interface.send_channel_message_advanced(&channel_name, outgoing_message).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "chansend" {
            Some(include_str!("../help/chansend.md").to_owned())
        } else {
            None
        }
    }
}
