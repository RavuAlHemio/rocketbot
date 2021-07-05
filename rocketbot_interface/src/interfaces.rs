use std::sync::Weak;

use async_trait::async_trait;
use json::JsonValue;

use crate::commands::{CommandDefinition, CommandInstance};
use crate::model::{ChannelMessage, Message};


/// Trait to be implemented by a RocketBot connection.
#[async_trait]
pub trait RocketBotInterface : Send + Sync {
    /// Sends a textual message to a channel.
    async fn send_channel_message(&self, channel_name: &str, message: &str);

    /// Sends a textual message to a person.
    async fn send_private_message(&self, username: &str, message: &str);

    /// Attempts to resolve the username-like value to an actual username on the server. Potentially
    /// enlists the assistance of relevant plugins.
    async fn resolve_username(&self, username: &str) -> Option<String>;

    /// Registers a command that is delivered when it is detected in a channel message. Returns
    /// `true` if the command was registered successfully and `false` if a command of that name
    /// already exists.
    async fn register_channel_command(&self, command: &CommandDefinition) -> bool;

    /// Registers a command that is delivered when it is detected in a private message. Returns
    /// `true` if the command was registered successfully and `false` if a command of that name
    /// already exists.
    async fn register_private_message_command(&self, command: &CommandDefinition) -> bool;
}


/// Trait to be implemented by RocketBot plugins.
#[async_trait]
pub trait RocketBotPlugin: Send + Sync {
    /// Instantiates this plugin and provides it with an interface to communicate with the bot (and
    /// the server to which it is connected).
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self where Self: Sized;

    /// Called if a textual message has been received in a channel.
    async fn channel_message(&self, _channel_message: &ChannelMessage) {}

    /// Called if a textual message has been received directly from another user.
    async fn private_message(&self, _message: &Message) {}

    /// Called if another plugin has requested to resolve a username-like value to an actual
    /// username on the server.
    async fn username_resolution(&self, _username: &str) -> Option<String> { None }

    /// Called if a command has been issued in a channel.
    async fn channel_command(&self, _channel_message: &ChannelMessage, _command: &CommandInstance) {}

    /// Called if a command has been issued in a private message.
    async fn private_message_command(&self, _message: &Message, _command: &CommandInstance) {}
}
