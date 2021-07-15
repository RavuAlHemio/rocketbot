use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use json::JsonValue;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;


// should be filled in by CI/CD during a build
const VERSION_STRING: &str = "{{VERSION}}";


pub struct VersionPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for VersionPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: JsonValue) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let version_command = CommandDefinition::new(
            "version".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}version".to_owned(),
            "Outputs the currently running version of the bot.".to_owned(),
        );
        my_interface.register_channel_command(&version_command).await;

        VersionPlugin {
            interface,
        }
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
            "unknown"
        } else {
            VERSION_STRING
        };

        interface.send_channel_message(
            &channel_message.channel.name,
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
}
