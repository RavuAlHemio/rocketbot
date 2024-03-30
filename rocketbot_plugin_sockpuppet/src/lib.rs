use std::collections::HashSet;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::{JsonValueExtensions, send_channel_message_advanced};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ImpersonationInfo, OutgoingMessage, PrivateMessage};
use rocketbot_interface::sync::RwLock;
use serde_json;


#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    allowed_usernames: HashSet<String>,
}


pub struct SockpuppetPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl SockpuppetPlugin {
    async fn private_command_chansend(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        if !config_guard.allowed_usernames.contains(&private_message.message.sender.username) {
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
                    Some(ImpersonationInfo::new(
                        format!("/avatar/{}", target_user.username),
                        target_user.username.clone(),
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
            None,
        );
        send_channel_message_advanced!(interface, &channel_name, outgoing_message).await;
    }

    async fn private_command_react(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        if !config_guard.allowed_usernames.contains(&private_message.message.sender.username) {
            return;
        }

        let message_id = &command.args[0];
        let emoji_name = command.rest.trim();
        let undo = command.flags.contains("undo") || command.flags.contains("u");

        if undo {
            interface.remove_reaction(&message_id, emoji_name).await;
        } else {
            interface.add_reaction(&message_id, emoji_name).await;
        }
    }

    async fn private_command_reload(&self, private_message: &PrivateMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        // make sure to release lock; the config update will need it
        {
            let config_guard = self.config.read().await;

            if !config_guard.allowed_usernames.contains(&private_message.message.sender.username) {
                return;
            }
        }

        interface.reload_configuration().await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut allowed_usernames = HashSet::new();
        for username_value in config["allowed_usernames"].members().ok_or("allowed_usernames not a list")? {
            let username = username_value
                .as_str().ok_or("entry in allowed_usernames not a string")?;
            allowed_usernames.insert(username.to_owned());
        }

        Ok(Config {
            allowed_usernames,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for SockpuppetPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "SockpuppetPlugin::config",
            config_object,
        );

        let chansend_command = CommandDefinitionBuilder::new(
            "chansend",
            "sockpuppet",
            "{cpfx}chansend [{lopfx}impersonate USERNAME] CHANNEL MESSAGE",
            "Sends a message, pretending to be the bot or someone else.",
        )
            .add_option("impersonate", CommandValueType::String)
            .arg_count(1)
            .build();
        my_interface.register_private_message_command(&chansend_command).await;

        my_interface.register_private_message_command(
            &CommandDefinitionBuilder::new(
                "react",
                "sockpuppet",
                "{cpfx}react MSGID EMOJI",
                "Reacts to the given message with the given emoji.",
            )
                .add_flag("undo")
                .add_flag("u")
                .arg_count(1)
                .build()
        ).await;
        my_interface.register_private_message_command(
            &CommandDefinitionBuilder::new(
                "reload",
                "sockpuppet",
                "{cpfx}reload",
                "Reloads the bot's configuration.",
            )
                .build()
        ).await;

        SockpuppetPlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "sockpuppet".to_owned()
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        if command.name == "chansend" {
            self.private_command_chansend(private_message, command).await
        } else if command.name == "react" {
            self.private_command_react(private_message, command).await
        } else if command.name == "reload" {
            self.private_command_reload(private_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "chansend" {
            Some(include_str!("../help/chansend.md").to_owned())
        } else if command_name == "react" {
            Some(include_str!("../help/react.md").to_owned())
        } else if command_name == "reload" {
            Some(include_str!("../help/reload.md").to_owned())
        } else {
            None
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
