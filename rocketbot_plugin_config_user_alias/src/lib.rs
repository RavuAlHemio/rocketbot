use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use json::JsonValue;

use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};


pub struct ConfigUserAliasPlugin {
    lowercase_alias_to_username: HashMap<String, String>,
}
#[async_trait]
impl RocketBotPlugin for ConfigUserAliasPlugin {
    fn new(_interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        let mut lowercase_alias_to_username = HashMap::new();
        for (key, val) in config["lowercase_alias_to_username"].entries() {
            lowercase_alias_to_username.insert(
                key.to_owned(),
                val.as_str().expect("lowercase_alias_to_username item is not a string").to_owned(),
            );
        }

        ConfigUserAliasPlugin {
            lowercase_alias_to_username,
        }
    }

    async fn username_resolution(&self, username: &str) -> Option<String> {
        let alias_lower = username.to_lowercase();
        self.lowercase_alias_to_username.get(&alias_lower).map(|un| un.clone())
    }
}
