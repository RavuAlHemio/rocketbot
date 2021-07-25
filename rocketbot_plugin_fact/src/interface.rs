use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use rand::RngCore;
use rocketbot_interface::sync::Mutex;
use serde_json;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FactError {
    message: String,
}
impl FactError {
    pub fn new(message: String) -> Self {
        Self { message }
    }

    pub fn new_str(message: &str) -> Self {
        Self::new(message.to_owned())
    }
}
impl fmt::Display for FactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for FactError {
}


#[async_trait]
pub trait FactProvider : Send + Sync {
    async fn new(config: serde_json::Value) -> Self where Self: Sized;
    async fn get_random_fact(&self, rng: Arc<Mutex<Box<dyn RngCore + Send>>>) -> Result<String, FactError>;
}
