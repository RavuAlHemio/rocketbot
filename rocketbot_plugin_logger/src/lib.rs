use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;
use tokio_postgres::NoTls;


pub struct LoggerPlugin {
    db_conn_string: String,
}
impl LoggerPlugin {
    async fn connect_db(&self) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
        let (client, connection) = match tokio_postgres::connect(&self.db_conn_string, NoTls).await {
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
}
#[async_trait]
impl RocketBotPlugin for LoggerPlugin {
    async fn new(_interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let db_conn_string = config["db_conn_string"].as_str()
            .expect("db_conn_string missing or not a string")
            .to_owned();

        LoggerPlugin {
            db_conn_string,
        }
    }

    async fn plugin_name(&self) -> String {
        "logger".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let db_conn = match self.connect_db().await {
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

    async fn channel_message_edited(&self, channel_message: &ChannelMessage) {
        let db_conn = match self.connect_db().await {
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
}
