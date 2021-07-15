use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use json::JsonValue;

use crate::commands::{CommandDefinition, CommandInstance};
use crate::model::{ChannelMessage, Message, User};


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

    /// Obtains the list of users in the channel with the given name. Returns `None` if the channel
    /// is not known.
    async fn obtain_users_in_channel(&self, channel_name: &str) -> Option<HashSet<User>>;

    /// Registers a command that is delivered when it is detected in a channel message. Returns
    /// `true` if the command was registered successfully and `false` if a command of that name
    /// already exists.
    async fn register_channel_command(&self, command: &CommandDefinition) -> bool;

    /// Registers a command that is delivered when it is detected in a private message. Returns
    /// `true` if the command was registered successfully and `false` if a command of that name
    /// already exists.
    async fn register_private_message_command(&self, command: &CommandDefinition) -> bool;

    /// Obtains a vector of all currently defined commands using the `rocketbot_interface::command`
    /// infrastructure.
    async fn get_defined_commands(&self) -> Vec<CommandDefinition>;

    /// Obtains a map of custom commands not defined using the `rocketbot_interface::command` from
    /// all plugins.
    async fn get_additional_commands_usages(&self) -> HashMap<String, String>;

    /// Obtains detailed help information for the given command (by requesting it using
    /// `RocketBotPlugin::get_command_help` from all active plugins), or `None` if it is not found.
    async fn get_command_help(&self, name: &str) -> Option<String>;

    /// Returns whether the given user ID is the bot's user ID.
    async fn is_my_user_id(&self, user_id: &str) -> bool;
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

    /// Called if a textual message is being sent in a channel. The plugin can return `false` to
    /// prevent the message from being sent.
    async fn outgoing_channel_message(&self, _channel_name: &str, _message: &str) -> bool { true }

    /// Called if a textual message is being sent directly to another user. The plugin can return
    /// `false` to prevent the message from being sent.
    async fn outgoing_private_message(&self, _username: &str, _message: &str) -> bool { true }

    /// Called if another plugin has requested to resolve a username-like value to an actual
    /// username on the server.
    async fn username_resolution(&self, _username: &str) -> Option<String> { None }

    /// Called if a command has been issued in a channel.
    async fn channel_command(&self, _channel_message: &ChannelMessage, _command: &CommandInstance) {}

    /// Called if a command has been issued in a private message.
    async fn private_message_command(&self, _message: &Message, _command: &CommandInstance) {}

    /// Called if a list of commands is requested; used to supply usage information for commands
    /// not handled using the `rocketbot_interface::command` infrastructure.
    async fn get_additional_commands_usages(&self) -> HashMap<String, String> { HashMap::new() }

    /// Called if detailed help information is requested for a given command. Should return `None`
    /// if the plugin doesn't provide this command.
    async fn get_command_help(&self, _command_name: &str) -> Option<String> { None }
}
