use std::fs::File;
use std::path::PathBuf;

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::errors::ConfigError;
use crate::jsonage::RocketBotJsonValue;


pub(crate) static CONFIG_FILE_NAME: OnceCell<PathBuf> = OnceCell::new();
pub(crate) static CONFIG: OnceCell<RwLock<Config>> = OnceCell::new();


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Config {
    pub server: ServerConfig,
    pub plugins: Vec<PluginConfig>,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct ServerConfig {
    pub websocket_uri: String,
    pub web_uri: String,
    pub username: String,
    pub password: String,
}


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PluginConfig {
    pub name: String,
    pub enabled: bool,
    pub config: RocketBotJsonValue,
}


pub(crate) async fn load_config() -> Result<(), ConfigError> {
    let config_file_name = CONFIG_FILE_NAME
        .get().expect("config file name set");

    let file = File::open(config_file_name)
        .map_err(|e| ConfigError::OpeningFile(e))?;
    let config: Config = serde_json::from_reader(file)
        .map_err(|e| ConfigError::Loading(e))?;

    match CONFIG.get() {
        None => {
            // initial setting
            if let Err(_) = CONFIG.set(RwLock::new(config)) {
                return Err(ConfigError::Setting);
            }
        },
        Some(c) => {
            *c.write().await = config;
        },
    };
    Ok(())
}
