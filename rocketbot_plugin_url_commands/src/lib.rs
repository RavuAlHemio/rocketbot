use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use reqwest;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde::{Deserialize, Serialize};
use serde_json;
use tracing::{error, warn};


#[derive(Clone, Debug, Default, Deserialize, Hash, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub commands_urls: BTreeMap<String, String>,
}

pub struct UrlCommandsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    http_client: reqwest::Client,
}
impl UrlCommandsPlugin {
    async fn register_url_command(interface: Arc<dyn RocketBotInterface>, name: &str) {
        let usage = format!("{{cpfx}}{}", name);
        let command = CommandDefinitionBuilder::new(
            name,
            "url_commands",
            usage,
            "Responds to the given command by querying a URL.",
        )
            .build();
        interface.register_channel_command(&command).await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, String> {
        serde_json::from_value(config)
            .map_err(|e| e.to_string())
    }
}
#[async_trait]
impl RocketBotPlugin for UrlCommandsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        for command in config_object.commands_urls.keys() {
            Self::register_url_command(Arc::clone(&my_interface), command).await;
        }

        let config_lock = RwLock::new(
            "UrlCommandsPlugin::config",
            config_object,
        );

        let http_client = reqwest::Client::builder()
            .build()
            .expect("failed to build reqwest HTTP client");

        UrlCommandsPlugin {
            interface,
            config: config_lock,
            http_client,
        }
    }

    async fn plugin_name(&self) -> String {
        "url_commands".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        if let Some(url) = config_guard.commands_urls.get(&command.name) {
            // request URL
            let response = match self.http_client.get(url).send().await {
                Ok(r) => r,
                Err(e) => {
                    error!("failed to GET {:?}: {}", url, e);
                    return;
                },
            };
            let status_code = response.status();
            let response_text = match response.text().await {
                Ok(rt) => rt,
                Err(e) => {
                    error!("failed to obtain text response for GET {:?}: {}", url, e);
                    return;
                },
            };
            if status_code != 200 {
                warn!("GET {:?} returned status code {}: {}", url, status_code, response_text);
                return;
            }

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &response_text,
            ).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        let config_guard = self.config.read().await;
        if config_guard.commands_urls.contains_key(command_name) {
            Some(include_str!("../help/urlcommand.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let interface = match self.interface.upgrade() {
            None => {
                error!("interface is gone");
                return false;
            },
            Some(i) => i,
        };

        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;

                // remove old commands
                for command_name in config_guard.commands_urls.keys() {
                    interface.unregister_channel_command(command_name).await;
                }

                // replace config
                *config_guard = c;

                // register new commands
                for command_name in config_guard.commands_urls.keys() {
                    Self::register_url_command(Arc::clone(&interface), command_name).await;
                }

                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
        }
    }
}
