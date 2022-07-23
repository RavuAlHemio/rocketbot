use std::sync::Weak;

use async_trait::async_trait;
use chrono::Utc;
use log::{error, info};
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;
use tokio_postgres::{self, NoTls};


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Config {
    db_conn_string: String,
    most_grateful_count: usize,
}


pub struct ThanksPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl ThanksPlugin {
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

    async fn handle_thank(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;
        let db_client = match self.connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let mut raw_target = command.args[0].as_str();
        if raw_target.starts_with("@") {
            raw_target = &raw_target[1..];
        }
        let target = match interface.resolve_username(&raw_target).await {
            Some(u) => u,
            None => raw_target.to_owned(),
        };

        let now = Utc::now();
        let thanker_lower = channel_message.message.sender.username.to_lowercase();
        let thankee_lower = target.to_lowercase();
        let channel = channel_message.channel.name.clone();
        let reason = command.rest.clone();

        if thanker_lower == thankee_lower {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!(
                    "You are so full of yourself, @{}",
                    channel_message.message.sender.username,
                ),
            ).await;
            return;
        }

        let exec_res = db_client.execute(
            r#"
                INSERT INTO thanks.thanks (thanks_id, "timestamp", thanker_lowercase, thankee_lowercase, channel, reason, deleted)
                VALUES (DEFAULT, $1, $2, $3, $4, $5, FALSE)
            "#,
            &[&now, &thanker_lower, &thankee_lower, &channel, &reason],
        ).await;
        if let Err(e) = exec_res {
            error!("error inserting thanks: {}", e);
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!(
                    "@{}: something broke, sorry!",
                    channel_message.message.sender.username,
                ),
            ).await;
            return;
        }

        info!("{} thanks {} in {}: {}", thanker_lower, thankee_lower, channel, reason);

        let count_row_res = db_client.query_one(
            r#"
                SELECT COUNT(*) count FROM thanks.thanks WHERE thankee_lowercase = $1 AND deleted = FALSE
            "#,
            &[&thankee_lower],
        ).await;
        let count_row = match count_row_res {
            Ok(cr) => cr,
            Err(e) => {
                error!("error querying thanks count: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!(
                        "@{} Alright! {} has been thanked.",
                        channel_message.message.sender.username,
                        target,
                    ),
                ).await;
                return;
            },
        };
        let count: i64 = count_row.get(0);

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!(
                "@{} Alright! By the way, {} has been thanked {} until now.",
                channel_message.message.sender.username,
                target,
                if count == 1 { "once".into() } else { format!("{} times", count) },
            ),
        ).await;
    }

    async fn handle_thanked(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;
        let db_client = match self.connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let mut raw_target = command.args[0].as_str();
        if raw_target.starts_with("@") {
            raw_target = &raw_target[1..];
        }
        let target = match interface.resolve_username(&raw_target).await {
            Some(u) => u,
            None => raw_target.to_owned(),
        };

        let target_lower = target.to_lowercase();
        let count_row_res = db_client.query_one(
            r#"
                SELECT COUNT(*) count FROM thanks.thanks WHERE thankee_lowercase = $1 AND deleted = FALSE
            "#,
            &[&target_lower],
        ).await;
        let count_row = match count_row_res {
            Ok(cr) => cr,
            Err(e) => {
                error!("error querying thanks count: {}", e);
                return;
            },
        };
        let count: i64 = count_row.get(0);

        let count_phrase: String = match count {
            0 => "not been thanked".into(),
            1 => "been thanked once".into(),
            other => format!("been thanked {} times", other),
        };

        let most_grateful_suffix = if count == 0 {
            String::new()
        } else {
            // also show stats
            let count_plus_one: i64 = (config_guard.most_grateful_count + 1).try_into()
                .expect("most grateful count not representable as i64");
            let most_grateful_res = db_client.query(
                r#"
                    SELECT thanker_lowercase, COUNT(*) count
                    FROM thanks.thanks
                    WHERE thankee_lowercase = $1 AND deleted = FALSE
                    GROUP BY thanker_lowercase
                    ORDER BY count DESC
                    LIMIT $2
                "#,
                &[&target_lower, &count_plus_one],
            ).await;
            match most_grateful_res {
                Err(e) => {
                    error!("error querying most grateful: {}", e);
                    String::new()
                },
                Ok(mg) => {
                    let mut entries = Vec::new();
                    for row in mg.iter().take(config_guard.most_grateful_count) {
                        let thanker: String = row.get(0);
                        let count: i64 = row.get(1);
                        entries.push(format!("{}: {}\u{D7}", thanker, count));
                    }
                    let entries_string = entries.join(", ");
                    if mg.len() > config_guard.most_grateful_count {
                        // additional people have been grateful
                        format!(" (Most grateful {}: {})", config_guard.most_grateful_count, entries_string)
                    } else {
                        // nobody else has been
                        format!(" ({})", entries_string)
                    }
                },
            }
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!(
                "@{} {} has {}{}.",
                channel_message.message.sender.username,
                target,
                count_phrase,
                most_grateful_suffix,
            ),
        ).await;
    }

    async fn handle_grateful(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;
        let db_client = match self.connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let mut raw_target = command.args[0].as_str();
        if raw_target.starts_with("@") {
            raw_target = &raw_target[1..];
        }
        let target = match interface.resolve_username(&raw_target).await {
            Some(u) => u,
            None => raw_target.to_owned(),
        };

        let target_lower = target.to_lowercase();
        let count_row_res = db_client.query_one(
            r#"
                SELECT COUNT(*) count FROM thanks.thanks WHERE thanker_lowercase = $1 AND deleted = FALSE
            "#,
            &[&target_lower],
        ).await;
        let count_row = match count_row_res {
            Ok(cr) => cr,
            Err(e) => {
                error!("error querying thanks count: {}", e);
                return;
            },
        };
        let count: i64 = count_row.get(0);

        let count_phrase: String = match count {
            0 => "thanked nobody".into(),
            1 => "given thanks once".into(),
            other => format!("given thanks {} times", other),
        };

        let most_thanked_suffix = if count == 0 {
            String::new()
        } else {
            // also show stats
            let count_plus_one: i64 = (config_guard.most_grateful_count + 1).try_into()
                .expect("most grateful count not representable as i64");
            let most_thanked_res = db_client.query(
                r#"
                    SELECT thankee_lowercase, COUNT(*) count
                    FROM thanks.thanks
                    WHERE thanker_lowercase = $1 AND deleted = FALSE
                    GROUP BY thankee_lowercase
                    ORDER BY count DESC
                    LIMIT $2
                "#,
                &[&target_lower, &count_plus_one],
            ).await;
            match most_thanked_res {
                Err(e) => {
                    error!("error querying most thanked: {}", e);
                    String::new()
                },
                Ok(mg) => {
                    let mut entries = Vec::new();
                    for row in mg.iter().take(config_guard.most_grateful_count) {
                        let thankee: String = row.get(0);
                        let count: i64 = row.get(1);
                        entries.push(format!("{}: {}\u{D7}", thankee, count));
                    }
                    let entries_string = entries.join(", ");
                    if mg.len() > config_guard.most_grateful_count {
                        // user thanked additional people
                        format!(" (Most thanked {}: {})", config_guard.most_grateful_count, entries_string)
                    } else {
                        // user thanked nobody else
                        format!(" ({})", entries_string)
                    }
                },
            }
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!(
                "@{} {} has {}{}.",
                channel_message.message.sender.username,
                target,
                count_phrase,
                most_thanked_suffix,
            ),
        ).await;
    }

    async fn handle_topthanked(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;
        let db_client = match self.connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let count: i64 = config_guard.most_grateful_count.try_into()
            .expect("most grateful count not representable as i64");
        let most_thanked_res = db_client.query(
            r#"
                SELECT thankee_lowercase, COUNT(*) count
                FROM thanks.thanks
                WHERE deleted = FALSE
                GROUP BY thankee_lowercase
                ORDER BY count DESC
                LIMIT $1
            "#,
            &[&count],
        ).await;
        match most_thanked_res {
            Err(e) => {
                error!("error querying most thanked: {}", e);
            },
            Ok(mg) => {
                let mut entries = Vec::new();
                for row in mg.iter().take(config_guard.most_grateful_count) {
                    let thankee: String = row.get(0);
                    let count: i64 = row.get(1);
                    entries.push(format!("{}: {}\u{D7}", thankee, count));
                }
                let entries_string = entries.join(", ");
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!(
                        "@{} {}",
                        channel_message.message.sender.username,
                        entries_string,
                    ),
                ).await;
            },
        }
    }

    async fn handle_topgrateful(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;
        let db_client = match self.connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let count: i64 = config_guard.most_grateful_count.try_into()
            .expect("most grateful count not representable as i64");
        let most_thanked_res = db_client.query(
            r#"
                SELECT thanker_lowercase, COUNT(*) count
                FROM thanks.thanks
                WHERE deleted = FALSE
                GROUP BY thanker_lowercase
                ORDER BY count DESC
                LIMIT $1
            "#,
            &[&count],
        ).await;
        match most_thanked_res {
            Err(e) => {
                error!("error querying most grateful: {}", e);
            },
            Ok(mg) => {
                let mut entries = Vec::new();
                for row in mg.iter().take(config_guard.most_grateful_count) {
                    let thanker: String = row.get(0);
                    let count: i64 = row.get(1);
                    entries.push(format!("{}: {}\u{D7}", thanker, count));
                }
                let entries_string = entries.join(", ");
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!(
                        "@{} {}",
                        channel_message.message.sender.username,
                        entries_string,
                    ),
                ).await;
            },
        }
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let db_conn_string = config["db_conn_string"]
            .as_str().ok_or("db_conn_string is not a string")?
            .to_owned();
        let most_grateful_count = config["most_grateful_count"]
            .as_usize().ok_or("most_grateful_count is not a usize")?;

        Ok(Config {
            db_conn_string,
            most_grateful_count,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for ThanksPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        // register commands
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "ThanksPlugin::config",
            config_object,
        );

        let thanks_command = CommandDefinitionBuilder::new(
            "thanks",
            "thanks",
            "{cpfx}thanks|{cpfx}thank|{cpfx}thx USERNAME [REASON]",
            "Thanks a user.",
        )
            .arg_count(1)
            .build();
        let thank_command = thanks_command.copy_named("thank");
        let thx_command = thanks_command.copy_named("thx");
        my_interface.register_channel_command(&thanks_command).await;
        my_interface.register_channel_command(&thank_command).await;
        my_interface.register_channel_command(&thx_command).await;

        let thanked_command = CommandDefinitionBuilder::new(
            "thanked",
            "thanks",
            "{cpfx}thanked USERNAME",
            "Displays how often the given user has been thanked.",
        )
            .arg_count(1)
            .build();
        let grateful_command = CommandDefinitionBuilder::new(
            "grateful",
            "thanks",
            "{cpfx}grateful USERNAME",
            "Displays how often the given user has thanked others.",
        )
            .arg_count(1)
            .build();
        my_interface.register_channel_command(&thanked_command).await;
        my_interface.register_channel_command(&grateful_command).await;

        let topthanked_command = CommandDefinitionBuilder::new(
            "topthanked",
            "thanks",
            "{cpfx}topthanked",
            "Displays the top thanked users to the knowledge of this bot.",
        )
            .build();
        let topgrateful_command = CommandDefinitionBuilder::new(
            "topgrateful",
            "thanks",
            "{cpfx}topgrateful",
            "Displays the top grateful users to the knowledge of this bot.",
        )
            .build();
        my_interface.register_channel_command(&topthanked_command).await;
        my_interface.register_channel_command(&topgrateful_command).await;

        ThanksPlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "thanks".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "thank" || command.name == "thanks" || command.name == "thx" {
            self.handle_thank(channel_message, command).await
        } else if command.name == "thanked" {
            self.handle_thanked(channel_message, command).await
        } else if command.name == "grateful" {
            self.handle_grateful(channel_message, command).await
        } else if command.name == "topthanked" {
            self.handle_topthanked(channel_message, command).await
        } else if command.name == "topgrateful" {
            self.handle_topgrateful(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "thank" || command_name == "thanks" || command_name == "thx" {
            Some(include_str!("../help/thank.md").to_owned())
        } else if command_name == "thanked" {
            Some(include_str!("../help/thanked.md").to_owned())
        } else if command_name == "grateful" {
            Some(include_str!("../help/grateful.md").to_owned())
        } else if command_name == "topthanked" {
            Some(include_str!("../help/topthanked.md").to_owned())
        } else if command_name == "topgrateful" {
            Some(include_str!("../help/topgrateful.md").to_owned())
        } else {
            None
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
