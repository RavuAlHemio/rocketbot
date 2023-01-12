use std::collections::{BTreeSet, HashSet};
use std::sync::Weak;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, error};
use rocketbot_interface::{JsonValueExtensions, send_channel_message, send_private_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, Message, PrivateMessage, User};
use tokio::sync::RwLock;
use url::Url;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Config {
    pub jack_to_port_api_uri: Url,
    pub authorized_usernames: BTreeSet<String>,
    pub timeout_ms: u64,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AnyMessage {
    Channel(ChannelMessage),
    Private(PrivateMessage),
}
impl AnyMessage {
    pub fn message(&self) -> &Message {
        match self {
            Self::Channel(c) => &c.message,
            Self::Private(p) => &p.message,
        }
    }

    pub fn sender(&self) -> &User {
        &self.message().sender
    }

    pub async fn respond<I: RocketBotInterface + ?Sized>(&self, interface: &I, message_body: &str) {
        match self {
            Self::Channel(c) => {
                send_channel_message!(
                    interface,
                    &c.channel.name,
                    message_body,
                ).await
            },
            Self::Private(p) => {
                send_private_message!(
                    interface,
                    &p.conversation.id,
                    message_body,
                ).await
            },
        }
    }
}


pub struct NetdevPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    known_commands: HashSet<String>,
    http_client: reqwest::Client,
}
impl NetdevPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let jack_to_port_api_uri_str = config["jack_to_port_api_uri"]
            .as_str().ok_or("jack_to_port_api_uri missing or not a string")?;
        let jack_to_port_api_uri = Url::parse(jack_to_port_api_uri_str)
            .or(Err("jack_to_port_api_uri not a valid URL"))?;
        let timeout_ms = config["timeout_ms"]
            .as_u64_or_strict(5_000).ok_or("timeout_ms missing or not an unsigned 64-bit integer")?;

        let authorized_usernames_iter = config["authorized_usernames"]
            .members_or_empty_strict().ok_or("authorized_usernames missing or not a list")?;
        let mut authorized_usernames = BTreeSet::new();
        for entry in authorized_usernames_iter {
            authorized_usernames.insert(
                entry
                    .as_str().ok_or("authorized_usernames entry not a string")?
                    .to_owned()
            );
        }

        Ok(Config {
            jack_to_port_api_uri,
            authorized_usernames,
            timeout_ms,
        })
    }

    async fn handle_command(&self, message: AnyMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            Some(i) => i,
            None => return,
        };
        let config = {
            let config_guard = self.config.read().await;
            (*config_guard).clone()
        };

        if !self.known_commands.contains(&command.name) {
            // not one of our commands
            return;
        }

        if !config.authorized_usernames.contains(&message.sender().username) {
            message.respond(&*interface, "Sorry, you are not authorized to use this command.").await;
            return;
        }

        match command.name.as_str() {
            "dose" => self.handle_dose_command(message, command, &config).await,
            _ => {},
        };
    }

    async fn get_http_json(&self, uri: Url, timeout: Duration) -> Option<serde_json::Value> {
        let resp_res = self.http_client
            .get(uri.clone())
            .timeout(timeout)
            .send().await
            .and_then(|response| response.error_for_status());
        let resp = match resp_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to obtain {} response: {}", uri, e);
                return None;
            },
        };
        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                error!("failed to obtain {} bytes: {}", uri, e);
                return None;
            },
        };
        let json: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to decode {} as JSON: {}", uri, e);
                return None;
            },
        };
        Some(json)
    }

    async fn handle_dose_command(&self, message: AnyMessage, command: &CommandInstance, config: &Config) {
        let interface = match self.interface.upgrade() {
            Some(i) => i,
            None => return,
        };

        // resolve jack
        let jack_name = command.rest.trim();

        let mut jack_uri = config.jack_to_port_api_uri.clone();
        jack_uri.query_pairs_mut()
            .append_pair("jack", jack_name);
        let jack_data_opt = self
            .get_http_json(jack_uri, Duration::from_millis(config.timeout_ms))
            .await;
        let jack_data = match jack_data_opt {
            Some(jd) => jd,
            None => {
                message.respond(&*interface, "Failed to obtain jack data.").await;
                return;
            },
        };

        debug!("obtained jack data: {}", jack_data);

        let mut ports = jack_data["ports"].members_or_empty().peekable();
        if ports.peek().is_none() {
            // no entries
            message.respond(
                &*interface,
                &format!("Jack `{}` is not known to Coruscant tools -- is it patched?", jack_name),
            ).await;
            return;
        }

        let mut info_blocks = Vec::new();
        for port in ports {
            let switch_name = port["switch"].as_str().unwrap_or("???");
            let port_name = port["port"].as_str().unwrap_or("???");

            let info_block = format!("connected to {} port {}", switch_name, port_name);
            // TODO: add real-time info once it is available
            info_blocks.push(info_block);
        }

        let mut response_text = format!("jack `{}`:", jack_name);
        for info_block in info_blocks {
            response_text.push_str("\n\n");
            response_text.push_str(&info_block);
        }

        message.respond(&*interface, &response_text).await;
    }
}
#[async_trait]
impl RocketBotPlugin for NetdevPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(config_object);

        let http_client = reqwest::Client::new();

        let mut known_commands = HashSet::new();
        let commands = [
            CommandDefinitionBuilder::new(
                "dose",
                "netdev",
                "{cpfx}dose JACK",
                "Outputs information about a network jack and the switch port it is connected to.",
            )
                .build(),
        ];
        for command in commands {
            known_commands.insert(command.name.clone());
            my_interface.register_channel_command(&command).await;
            my_interface.register_private_message_command(&command).await;
        }

        Self {
            interface,
            config: config_lock,
            known_commands,
            http_client,
        }
    }

    async fn plugin_name(&self) -> String {
        "netdev".to_owned()
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

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        self.handle_command(AnyMessage::Channel(channel_message.clone()), command).await
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        self.handle_command(AnyMessage::Private(private_message.clone()), command).await
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "dose" {
            Some(include_str!("../help/dose.md").to_owned())
        } else {
            None
        }
    }
}
