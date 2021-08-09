mod ast;
#[cfg(feature = "currency")]
mod currency;
mod grimoire;
mod numbers;
mod parsing;
mod units;


use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::sync::Weak;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use log::error;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;
use toml;

use crate::ast::{AstNode, SimplificationState};
use crate::grimoire::{get_canonical_constants, get_canonical_functions};
use crate::parsing::parse_full_expression;
use crate::units::{StoredUnitDatabase, UnitDatabase};


pub struct CalcPlugin {
    interface: Weak<dyn RocketBotInterface>,
    timeout_seconds: f64,
    max_result_string_length: usize,
    unit_database: RwLock<UnitDatabase>,
    currency_units: bool,
    last_currency_update: Mutex<DateTime<Utc>>,
}
impl CalcPlugin {
    async fn handle_calc(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let channel_name = &channel_message.channel.name;
        let sender_username = &channel_message.message.sender.username;

        let top_node = match parse_full_expression(&command.rest) {
            Ok(node) => node,
            Err(e) => {
                interface.send_channel_message(
                    channel_name,
                    &format!("@{} Failed to parse message:\n```\n{}\n```", sender_username, e),
                ).await;
                return;
            },
        };

        {
            let mut lcu_guard = self.last_currency_update
                .lock().await;
            let delta = Utc::now() - (*lcu_guard);
            if delta > chrono::Duration::hours(24) {
                // update!
                let mut write_guard = self.unit_database
                    .write().await;
                if cfg!(feature = "currency") && self.currency_units {
                    crate::currency::update_currencies(&mut write_guard).await;
                }
                *lcu_guard = Utc::now();
            }
        }

        let unit_database = {
            let read_guard = self.unit_database
                .read().await;
            (*read_guard).clone()
        };

        let simplified_res = {
            let mut state = SimplificationState {
                constants: get_canonical_constants(),
                functions: get_canonical_functions(),
                units: unit_database,
                start_time: Instant::now(),
                timeout: Duration::from_secs_f64(self.timeout_seconds),
            };
            top_node.simplify(&mut state)
        };
        match simplified_res {
            Ok(ntn) => {
                let result_string = match &ntn.node {
                    AstNode::Number(i) => {
                        i.to_string()
                    },
                    other => {
                        error!("simplification produced invalid value: {:?}", other);
                        interface.send_channel_message(
                            channel_name,
                            &format!("@{} Simplification produced an invalid value!", sender_username),
                        ).await;
                        return;
                    },
                };

                if result_string.len() > self.max_result_string_length {
                    interface.send_channel_message(
                        channel_name,
                        &format!("@{} NUMBER TOO GROSS", sender_username),
                    ).await;
                }

                interface.send_channel_message(
                    channel_name,
                    &format!("@{} {}", sender_username, result_string),
                ).await;
            },
            Err(e) => {
                if let Some((start, end)) = e.start_end {
                    let wavey: String = (0..end)
                        .map(|i| if i < start { ' ' } else { '^' })
                        .collect();
                    interface.send_channel_message(
                        channel_name,
                        &format!(
                            "@{} Simplification failed: {}\n```\n{}\n{}\n```",
                            sender_username, e.error, command.rest, wavey,
                        ),
                    ).await;
                } else {
                    interface.send_channel_message(
                        channel_name,
                        &format!("@{} Simplification failed: {}", sender_username, e.error),
                    ).await;
                }
            },
        };
    }
}
#[async_trait]
impl RocketBotPlugin for CalcPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> CalcPlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let timeout_seconds = config["timeout_seconds"].as_f64()
            .expect("timeout_seconds missing or not representable as f64");
        let max_result_string_length = config["max_result_string_length"].as_usize()
            .expect("max_result_string_length missing or not representable as usize");
        let currency_units = config["currency_units"].as_bool()
            .expect("currency_units missing or not representable as boolean");

        let unit_db_file_value = &config["unit_database_file"];
        let unit_database_inner = if unit_db_file_value.is_null() {
            UnitDatabase::new_empty()
        } else {
            let unit_db_file = unit_db_file_value.as_str()
                .expect("unit_database_file not a string");
            let mut f = File::open(unit_db_file)
                .expect("failed to open unit_database_file");
            let mut unit_db_toml = Vec::new();
            f.read_to_end(&mut unit_db_toml)
                .expect("failed to read unit_database_file");
            let unit_db: StoredUnitDatabase = toml::from_slice(&unit_db_toml)
                .expect("failed to load unit_database_file");
            unit_db.to_unit_database()
                .expect("failed to process unit database file")
        };
        let unit_database = RwLock::new(
            "CalcPlugin::unit_database",
            unit_database_inner,
        );

        let last_currency_update = Mutex::new(
            "CalcPlugin::last_currency_update",
            Utc.ymd(2000, 1, 1)
                .and_hms(0, 0, 0)
        );

        my_interface.register_channel_command(&CommandDefinition::new(
            "calc".to_owned(),
            "calc".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}calc EXPRESSION".to_owned(),
            "Calculates the given mathematical expression and outputs the result.".to_owned(),
        )).await;

        CalcPlugin {
            interface,
            timeout_seconds,
            max_result_string_length,
            unit_database,
            currency_units,
            last_currency_update,
        }
    }

    async fn plugin_name(&self) -> String {
        "calc".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "calc" {
            self.handle_calc(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "calc" {
            Some(include_str!("../help/calc.md").to_owned())
        } else {
            None
        }
    }
}
