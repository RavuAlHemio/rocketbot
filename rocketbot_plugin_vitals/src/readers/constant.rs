use async_trait::async_trait;

use crate::interface::VitalsReader;


pub(crate) struct ConstantReader {
    constant_text: String,
}
#[async_trait]
impl VitalsReader for ConstantReader {
    async fn new(config: &serde_json::Value) -> Self {
        let constant_text = config["constant_text"].as_str()
            .expect("constant_text is not a string")
            .to_owned();

        Self {
            constant_text,
        }
    }

    async fn read(&self) -> Option<String> {
        Some(self.constant_text.clone())
    }
}
