use async_trait::async_trait;
use serde_json;


#[async_trait]
pub trait VitalsReader : Send + Sync {
    async fn new(config: &serde_json::Value) -> Self where Self : Sized;
    async fn read(&self) -> Option<String>;
}
