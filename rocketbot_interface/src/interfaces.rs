use std::sync::Weak;

use async_trait::async_trait;
use json::JsonValue;

use crate::model::{ChannelMessage, Message};


/// Trait to be implemented by a RocketBot connection.
#[async_trait]
pub trait RocketBotInterface : Send + Sync {
    /// Sends a textual message to a channel.
    async fn send_channel_message(&self, channel_name: &str, message: &str);

    /// Sends a textual message to a person.
    async fn send_private_message(&self, username: &str, message: &str);
}


/// Trait to be implemented by RocketBot plugins.
#[async_trait]
pub trait RocketBotPlugin: Send + Sync {
    /// Instantiates this plugin and provides it with an interface to communicate with the bot (and
    /// the server to which it is connected).
    fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self where Self: Sized;

    /// Called if a textual message has been received in a channel.
    async fn channel_message(&self, _channel_message: &ChannelMessage) {}

    /// Called if a textual message has been received directly from another user.
    async fn private_message(&self, _message: &Message) {}
}
