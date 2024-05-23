use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct WebConfig {
    pub listen: SocketAddr,
    pub db_conn_string: String,
    pub bot_config_path: PathBuf,
    pub static_path: PathBuf,
    #[serde(default)] pub bim_odds_ends: Vec<BimOddEndConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BimOddEndConfig {
    pub title: String,
    #[serde(default)] pub description: Option<String>,
    pub query: String,
    pub column_titles: Vec<String>,
    #[serde(default)] pub column_link_formats: Vec<Option<String>>,
}
