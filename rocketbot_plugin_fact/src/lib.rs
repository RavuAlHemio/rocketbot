pub mod interface;
pub mod providers;


use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::error;
use rand::{Rng, RngCore, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;

use crate::interface::FactProvider;


struct Config {
    providers: Vec<Box<dyn FactProvider>>,
}


pub struct FactPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Arc<Mutex<Box<dyn RngCore + Send>>>,
}
impl FactPlugin {
    async fn handle_fact_command(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        if config_guard.providers.len() == 0 {
            return;
        }

        let provider_index = {
            let mut rng_guard = self.rng
                .lock().await;
            rng_guard.gen_range(0..config_guard.providers.len())
        };

        let result = config_guard.providers[provider_index]
            .get_random_fact(Arc::clone(&self.rng)).await
            .unwrap_or_else(|e| e.to_string());

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &result,
        ).await;
    }

    async fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let mut providers: Vec<Box<dyn FactProvider>> = Vec::with_capacity(config["providers"].members_or_empty().len());
        for provider_entry in config["providers"].members_or_empty() {
            let name = match provider_entry["name"].as_str() {
                Some(n) => n,
                None => return Err("/providers/?/name not a string"),
            };
            let provider_config = provider_entry["config"].clone();

            let provider: Box<dyn FactProvider> = if name == "uncyclopedia" {
                Box::new(crate::providers::uncyclopedia::UncyclopediaProvider::new(provider_config).await)
            } else {
                error!("unknown fact provider {:?}", name);
                return Err("unknown fact provider");
            };
            providers.push(provider);
        }
        Ok(Config {
            providers,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for FactPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        // register commands
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config).await
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "FactPlugin::config",
            config_object,
        );

        let rng = Arc::new(Mutex::new(
            "FactPlugin::rng",
            Box::new(StdRng::from_entropy()) as Box<dyn RngCore + Send>,
        ));

        let fact_command = CommandDefinitionBuilder::new(
            "fact",
            "fact",
            "{cpfx}fact",
            "Obtains and displays a random fact.",
        )
            .build();
        my_interface.register_channel_command(&fact_command).await;

        FactPlugin {
            interface,
            config: config_lock,
            rng,
        }
    }

    async fn plugin_name(&self) -> String {
        "fact".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "fact" {
            self.handle_fact_command(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "fact" {
            Some(include_str!("../help/fact.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config).await {
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
