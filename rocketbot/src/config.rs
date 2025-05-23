use std::fs::File;
use std::path::PathBuf;

use once_cell::sync::OnceCell;
use rocketbot_interface::sync::RwLock;
use rocketbot_interface::commands::CommandConfiguration;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::errors::ConfigError;


pub(crate) static CONFIG_FILE_NAME: OnceCell<PathBuf> = OnceCell::new();
pub(crate) static CONFIG: OnceCell<RwLock<Config>> = OnceCell::new();


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Config {
    pub server: ServerConfig,
    #[serde(default)] pub commands: CommandConfiguration,
    pub plugins: Vec<PluginConfig>,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct ServerConfig {
    pub websocket_uri: String,
    pub web_uri: String,
    pub emojione_emoji_json_uri: String,
    pub username: String,
    pub password: String,
    #[serde(default)] pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct RateLimitConfig {
    pub max_messages: usize,
    pub time_slot_ms: u64,
}


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PluginConfig {
    pub name: String,
    pub enabled: bool,
    pub config: serde_json::Value,
}


pub(crate) async fn load_config() -> Result<Config, ConfigError> {
    let config_file_name = CONFIG_FILE_NAME
        .get().expect("config file name not set");

    let file = File::open(config_file_name)
        .map_err(|e| ConfigError::OpeningFile(e))?;
    let config: Config = serde_json::from_reader(file)
        .map_err(|e| ConfigError::Loading(e))?;

    Ok(config)
}

pub(crate) async fn set_config(config: Config) -> Result<(), ConfigError> {
    match CONFIG.get() {
        None => {
            // initial setting
            if let Err(_) = CONFIG.set(RwLock::new("CONFIG", config)) {
                return Err(ConfigError::Setting);
            }
        },
        Some(c) => {
            *c.write().await = config;
        },
    };
    Ok(())
}
