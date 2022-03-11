use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::sync::RwLock;
use serde_json;


#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    lowercase_alias_to_username: HashMap<String, String>,
}


pub struct ConfigUserAliasPlugin {
    config: RwLock<Config>,
}
impl ConfigUserAliasPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut lowercase_alias_to_username = HashMap::new();
        let entries = config["lowercase_alias_to_username"]
            .entries().ok_or("lowercase_alias_to_username not an object")?;
        for (key, val) in entries {
            lowercase_alias_to_username.insert(
                key.to_owned(),
                val
                    .as_str().ok_or("lowercase_alias_to_username item is not a string")?
                    .to_owned(),
            );
        }
        Ok(Config {
            lowercase_alias_to_username,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for ConfigUserAliasPlugin {
    async fn new(_interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "ConfigUserAliasPlugin::config",
            config_object,
        );

        ConfigUserAliasPlugin {
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "config_user_alias".to_owned()
    }

    async fn username_resolution(&self, username: &str) -> Option<String> {
        let alias_lower = username.to_lowercase();
        let config_guard = self.config.read().await;
        config_guard.lowercase_alias_to_username.get(&alias_lower).map(|un| un.clone())
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
