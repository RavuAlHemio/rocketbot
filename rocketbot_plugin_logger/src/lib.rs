use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;
use tokio_postgres::NoTls;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Config {
    db_conn_string: String,
}


pub struct LoggerPlugin {
    config: RwLock<Config>,
}
impl LoggerPlugin {
    async fn connect_db(&self, config: &Config) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
        let (client, connection) = match tokio_postgres::connect(&config.db_conn_string, NoTls).await {
            Ok(cc) => cc,
            Err(e) => {
                error!("error connecting to database: {}", e);
                return Err(e);
            },
        };
        tokio::spawn(async move {
            connection.await
        });
        Ok(client)
    }

    async fn channel_message_received_or_delivered(&self, channel_message: &ChannelMessage) {
        let config_guard = self.config.read().await;
        let db_conn = match self.connect_db(&config_guard).await {
            Ok(dbc) => dbc,
            Err(e) => {
                error!("failed to connect to database: {}", e);
                return;
            },
        };

        // ensure the channel exists
        let channel_res = db_conn.execute(
            "
                INSERT INTO logger.channel (channel_id, channel_name)
                VALUES ($1, $2)
                ON CONFLICT (channel_id) DO NOTHING
            ",
            &[
                &channel_message.channel.id,
                &channel_message.channel.name,
            ],
        ).await;
        if let Err(e) = channel_res {
            error!("failed to insert channel: {}", e);
            return;
        }

        // add the message
        let msg_res = db_conn.execute(
            "
                INSERT INTO logger.message (message_id, channel_id, \"timestamp\", sender_username, sender_nickname)
                VALUES ($1, $2, $3, $4, $5)
            ",
            &[
                &channel_message.message.id,
                &channel_message.channel.id,
                &channel_message.message.timestamp,
                &channel_message.message.sender.username,
                &channel_message.message.sender.nickname,
            ],
        ).await;
        if let Err(e) = msg_res {
            error!("failed to insert message: {}", e);
            return;
        }

        // add the revision
        let msg_res = db_conn.execute(
            "
                INSERT INTO logger.message_revision (revision_id, message_id, \"timestamp\", author_username, body)
                VALUES (DEFAULT, $1, $2, $3, $4)
            ",
            &[
                &channel_message.message.id,
                &channel_message.message.timestamp,
                &channel_message.message.sender.username,
                &channel_message.message.raw,
            ],
        ).await;
        if let Err(e) = msg_res {
            error!("failed to insert message revision: {}", e);
            return;
        }
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let db_conn_string = config["db_conn_string"]
            .as_str().ok_or("db_conn_string missing or not a string")?
            .to_owned();

        Ok(Config {
            db_conn_string,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for LoggerPlugin {
    async fn new(_interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "LoggerPlugin::config",
            config_object,
        );

        LoggerPlugin {
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "logger".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        self.channel_message_received_or_delivered(channel_message).await
    }

    async fn channel_message_delivered(&self, channel_message: &ChannelMessage) {
        self.channel_message_received_or_delivered(channel_message).await
    }

    async fn channel_message_edited(&self, channel_message: &ChannelMessage) {
        let config_guard = self.config.read().await;
        let db_conn = match self.connect_db(&config_guard).await {
            Ok(dbc) => dbc,
            Err(e) => {
                error!("failed to connect to database: {}", e);
                return;
            },
        };

        let edit_info = match &channel_message.message.edit_info {
            Some(ei) => ei,
            None => {
                error!("edited message with ID {:?} contains no edit info!", channel_message.message.id);
                return;
            },
        };

        // assume the message exists

        // add the revision
        let msg_res = db_conn.execute(
            "
                INSERT INTO logger.message_revision (revision_id, message_id, \"timestamp\", author_username, body)
                VALUES (DEFAULT, $1, $2, $3, $4)
            ",
            &[
                &channel_message.message.id,
                &edit_info.timestamp,
                &edit_info.editor.username,
                &channel_message.message.raw,
            ],
        ).await;
        if let Err(e) = msg_res {
            error!("failed to insert message revision: {}", e);
            return;
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
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
