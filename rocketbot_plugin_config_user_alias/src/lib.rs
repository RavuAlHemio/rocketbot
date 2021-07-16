use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use serde_json;


pub struct ConfigUserAliasPlugin {
    lowercase_alias_to_username: HashMap<String, String>,
}
#[async_trait]
impl RocketBotPlugin for ConfigUserAliasPlugin {
    async fn new(_interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let mut lowercase_alias_to_username = HashMap::new();
        for (key, val) in config["lowercase_alias_to_username"].entries().expect("lowercase_alias_to_username not an object") {
            lowercase_alias_to_username.insert(
                key.to_owned(),
                val.as_str().expect("lowercase_alias_to_username item is not a string").to_owned(),
            );
        }

        ConfigUserAliasPlugin {
            lowercase_alias_to_username,
        }
    }

    async fn plugin_name(&self) -> String {
        "config_user_alias".to_owned()
    }

    async fn username_resolution(&self, username: &str) -> Option<String> {
        let alias_lower = username.to_lowercase();
        self.lowercase_alias_to_username.get(&alias_lower).map(|un| un.clone())
    }
}
