use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WebConfig {
    pub listen: SocketAddr,
    pub db_conn_string: String,
    pub bot_config_path: PathBuf,
    pub template_path: String,
    pub static_path: PathBuf,
}
