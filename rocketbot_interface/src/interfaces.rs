use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json;

use crate::commands::{CommandConfiguration, CommandDefinition, CommandInstance};
use crate::model::{
    Channel, ChannelMessage, ChannelTextType, PrivateConversation, PrivateMessage, User,
};


/// Trait to be implemented by a RocketBot connection.
#[async_trait]
pub trait RocketBotInterface : Send + Sync {
    /// Sends a textual message to a channel.
    async fn send_channel_message(&self, channel_name: &str, message: &str);

    /// Sends a textual message to a private conversation.
    async fn send_private_message(&self, conversation_id: &str, message: &str);

    /// Sends a textual message to a person.
    async fn send_private_message_to_user(&self, username: &str, message: &str);

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

    /// Obtains a copy of the command configuration currently in operation.
    async fn get_command_configuration(&self) -> CommandConfiguration;

    /// Obtains a vector of all currently defined channel commands using the
    /// `rocketbot_interface::command` infrastructure. If `plugin` is not `None`, only returns
    /// commands for that plugin.
    async fn get_defined_channel_commands(&self, plugin: Option<&str>) -> Vec<CommandDefinition>;

    /// Obtains a vector of all currently defined private message commands using the
    /// `rocketbot_interface::command` infrastructure. If `plugin` is not `None`, only returns
    /// commands for that plugin.
    async fn get_defined_private_message_commands(&self, plugin: Option<&str>) -> Vec<CommandDefinition>;

    /// Obtains a map of custom channel commands not defined using the
    /// `rocketbot_interface::command` infrastructure from all plugins. The key is the command name
    /// and the value is a tuple of usage information and description. If `plugin` is not `None`,
    /// only returns commands for that plugin.
    async fn get_additional_channel_commands_usages(&self, plugin: Option<&str>) -> HashMap<String, (String, String)>;

    /// Obtains a map of custom private message commands not defined using the
    /// `rocketbot_interface::command` infrastructure from all plugins. The key is the command name
    /// and the value is a tuple of usage information and description. If `plugin` is not `None`,
    /// only returns commands for that plugin.
    async fn get_additional_private_message_commands_usages(&self, plugin: Option<&str>) -> HashMap<String, (String, String)>;

    /// Obtains detailed help information for the given command (by requesting it using
    /// `RocketBotPlugin::get_command_help` from all active plugins), or `None` if it is not found.
    async fn get_command_help(&self, name: &str) -> Option<String>;

    /// Returns whether the given user ID is the bot's user ID.
    async fn is_my_user_id(&self, user_id: &str) -> bool;

    /// Returns the names of the currently active plugins.
    async fn get_plugin_names(&self) -> Vec<String>;

    /// Returns the maximum message length on the current server, or `None` if it is not known.
    async fn get_maximum_message_length(&self) -> Option<usize>;

    /// Registers a timer with the bot. Once the given timestamp is reached, a call to
    /// `RocketBotPlugin::timer_elapsed` with the contents of `custom_data` is made.
    async fn register_timer(&self, timestamp: DateTime<Utc>, custom_data: serde_json::Value);

    /// Obtains the given textual property of the given channel.
    async fn get_channel_text(&self, channel_name: &str, text_type: ChannelTextType) -> Option<String>;

    /// Sets the given textual property of the given channel to the given value.
    async fn set_channel_text(&self, channel_name: &str, text_type: ChannelTextType, text: &str);
}


/// Trait to be implemented by RocketBot plugins.
#[async_trait]
pub trait RocketBotPlugin: Send + Sync {
    /// Instantiates this plugin and provides it with an interface to communicate with the bot (and
    /// the server to which it is connected).
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self where Self: Sized;

    /// Returns the plugin's name.
    async fn plugin_name(&self) -> String;

    /// Called if a textual message has been received in a channel whose author is not the bot.
    async fn channel_message(&self, _channel_message: &ChannelMessage) {}

    /// Called if a textual message has been received in a channel whose author is the bot.
    async fn channel_message_delivered(&self, _channel_message: &ChannelMessage) {}

    /// Called if a textual message in a channel has been edited (whether or not the original author
    /// is the bot).
    async fn channel_message_edited(&self, _channel_message: &ChannelMessage) {}

    /// Called if a textual message is being sent to a channel. The plugin can return `false` to
    /// prevent the message from being sent.
    async fn outgoing_channel_message(&self, _channel: &Channel, _message: &str) -> bool { true }

    /// Called if a command has been issued in a channel.
    async fn channel_command(&self, _channel_message: &ChannelMessage, _command: &CommandInstance) {}

    /// Called if a textual private message has been received whose author is not the bot.
    async fn private_message(&self, _private_message: &PrivateMessage) {}

    /// Called if a textual private message has been received whose author is the bot.
    async fn private_message_delivered(&self, _private_message: &PrivateMessage) {}

    /// Called if a textual private message has been edited (whether or not the original author is
    /// the bot).
    async fn private_message_edited(&self, _private_message: &PrivateMessage) {}

    /// Called if a textual private message is being sent. The plugin can return `false` to prevent
    /// the message from being sent.
    async fn outgoing_private_message(&self, _conversation: &PrivateConversation, _message: &str) -> bool { true }

    /// Called if a command has been issued in a private message.
    async fn private_command(&self, _private_message: &PrivateMessage, _command: &CommandInstance) {}

    /// Called if another plugin has requested to resolve a username-like value to an actual
    /// username on the server.
    async fn username_resolution(&self, _username: &str) -> Option<String> { None }

    /// Called if a list of channel commands is requested; used to supply usage information for
    /// commands not handled using the `rocketbot_interface::command` infrastructure. The key of
    /// each entry is the command name and the value is a tuple of usage information and
    /// description.
    async fn get_additional_channel_commands_usages(&self) -> HashMap<String, (String, String)> { HashMap::new() }

    /// Called if a list of private message commands is requested; used to supply usage information
    /// for commands not handled using the `rocketbot_interface::command` infrastructure. The key of
    /// each entry is the command name and the value is a tuple of usage information and
    /// description.
    async fn get_additional_private_message_commands_usages(&self) -> HashMap<String, (String, String)> { HashMap::new() }

    /// Called if detailed help information is requested for a given command. Should return `None`
    /// if the plugin doesn't provide this command.
    async fn get_command_help(&self, _command_name: &str) -> Option<String> { None }

    /// Called when a timer, registered previously using `RocketBotInterface::register_timer`,
    /// elapses.
    async fn timer_elapsed(&self, _custom_data: &serde_json::Value) {}
}
