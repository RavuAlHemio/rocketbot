use std::sync::Weak;

use async_trait::async_trait;
use log::warn;
use rocketbot_interface::{send_channel_message, send_private_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, PrivateMessage};
use serde_json;


// should be filled in by CI/CD during a build
const VERSION_STRING: &str = "{{VERSION}}";
const COMMIT_MESSAGE_SHORT: &str = "{{COMMIT_MESSAGE_SHORT}}";


pub struct VersionPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for VersionPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let version_command = CommandDefinitionBuilder::new(
            "version",
            "version",
            "{cpfx}version",
            "Outputs the currently running version of the bot.",
        )
            .build();
        my_interface.register_channel_command(&version_command).await;
        my_interface.register_private_message_command(&version_command).await;

        VersionPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "version".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "version" {
            return;
        }

        // use concat! to hide this string from CI/CD, lest it be replaced too
        let unset_version_string = concat!("{{", "VERSION", "}}");

        let this_version = if VERSION_STRING == unset_version_string {
            warn!("version requested but unknown!");
            "unknown".to_owned()
        } else {
            format!("`{}` _{}_", VERSION_STRING, COMMIT_MESSAGE_SHORT)
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("rocketbot revision {}", this_version),
        ).await;
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "version" {
            return;
        }

        // use concat! to hide this string from CI/CD, lest it be replaced too
        let unset_version_string = concat!("{{", "VERSION", "}}");

        let this_version = if VERSION_STRING == unset_version_string {
            warn!("version requested but unknown!");
            "unknown".to_owned()
        } else {
            format!("`{}` _{}_", VERSION_STRING, COMMIT_MESSAGE_SHORT)
        };

        send_private_message!(
            interface,
            &private_message.conversation.id,
            &format!("rocketbot revision {}", this_version),
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "version" {
            Some(include_str!("../help/version.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // not much to update
        true
    }
}
