use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Weak;

use async_trait::async_trait;
use regex::Regex;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::sync::RwLock;
use rocketbot_string::regex::EnjoyableRegex;
use serde_json;
use tracing::error;


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Config {
    alias_regex_to_username: BTreeMap<EnjoyableRegex, String>,
}


pub struct ConfigUserAliasPlugin {
    config: RwLock<Config>,
}
impl ConfigUserAliasPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, Cow<'static, str>> {
        let mut alias_regex_to_username = BTreeMap::new();
        let entries = config["alias_regex_to_username"]
            .entries().ok_or("alias_regex_to_username not an object")?;
        for (key, val) in entries {
            let key_regex = Regex::new(key)
                .map_err(|e| Cow::Owned(format!("regex {:?} is invalid: {}", key, e)))?;
            let key_enjoyable_regex = EnjoyableRegex::from_regex(key_regex);
            let value_string = val
                .as_str().ok_or("alias_regex_to_username item is not a string")?
                .to_owned();
            alias_regex_to_username.insert(
                key_enjoyable_regex,
                value_string,
            );
        }
        Ok(Config {
            alias_regex_to_username,
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
        let config_guard = self.config.read().await;
        for (regex, actual_username) in &config_guard.alias_regex_to_username {
            if regex.is_match(&username) {
                return Some(actual_username.clone());
            }
        }
        None
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
