mod ast;
#[cfg(feature = "currency")]
mod currency;
mod factor;
mod grimoire;
mod numbers;
mod parsing;
mod units;


use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use log::error;
use num_bigint::BigUint;
use rocketbot_interface::{
    add_thousands_separators, JsonValueExtensions, ResultExtensions, send_channel_message,
};
use rocketbot_interface::commands::{
    CommandBehaviors, CommandDefinition, CommandDefinitionBuilder, CommandInstance,
};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, run_stoppable_task_timeout, RwLock, StoppableTaskResult};
use serde_json;
use toml;

use crate::ast::{AstNode, SimplificationState};
use crate::factor::PrimeCache;
use crate::grimoire::{get_canonical_constants, get_canonical_functions};
use crate::parsing::parse_full_expression;
use crate::units::{StoredUnitDatabase, UnitDatabase};


#[derive(Clone, Debug)]
struct Config {
    timeout_seconds: f64,
    max_result_string_length: usize,
    currency_units: bool,
    unit_database: UnitDatabase,
}


pub struct CalcPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    last_currency_update: Mutex<DateTime<Utc>>,
    prime_cache: Arc<Mutex<PrimeCache>>,
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
                send_channel_message!(
                    interface,
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
                let mut write_guard = self.config
                    .write().await;
                if cfg!(feature = "currency") && write_guard.currency_units {
                    crate::currency::update_currencies(&mut write_guard.unit_database).await;
                }
                *lcu_guard = Utc::now();
            }
        }

        let config_guard = self.config.read().await;

        let simplified_res = {
            let mut state = SimplificationState {
                constants: get_canonical_constants(),
                functions: get_canonical_functions(),
                units: config_guard.unit_database.clone(),
                start_time: Instant::now(),
                timeout: Duration::from_secs_f64(config_guard.timeout_seconds),
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
                        send_channel_message!(
                            interface,
                            channel_name,
                            &format!("@{} Simplification produced an invalid value!", sender_username),
                        ).await;
                        return;
                    },
                };

                if result_string.len() > config_guard.max_result_string_length {
                    send_channel_message!(
                        interface,
                        channel_name,
                        &format!("@{} NUMBER TOO GROSS", sender_username),
                    ).await;
                }

                send_channel_message!(
                    interface,
                    channel_name,
                    &format!("@{} {}", sender_username, result_string),
                ).await;
            },
            Err(e) => {
                if let Some((start, end)) = e.start_end {
                    let wavey: String = (0..end)
                        .map(|i| if i < start { ' ' } else { '^' })
                        .collect();
                    send_channel_message!(
                        interface,
                        channel_name,
                        &format!(
                            "@{} Simplification failed: {}\n```\n{}\n{}\n```",
                            sender_username, e.error, command.rest, wavey,
                        ),
                    ).await;
                } else {
                    send_channel_message!(
                        interface,
                        channel_name,
                        &format!("@{} Simplification failed: {}", sender_username, e.error),
                    ).await;
                }
            },
        };
    }

    async fn handle_calcconst(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut const_names: Vec<String> = get_canonical_constants().drain()
            .map(|(cn, _cdef)| format!("`{}`", cn))
            .collect();
        const_names.sort_unstable();
        let consts_str = const_names.join(", ");
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("The following constants are available: {}", consts_str),
        ).await;
    }

    async fn handle_calcfunc(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut func_names: Vec<String> = get_canonical_functions().drain()
            .map(|(fname, _fdef)| format!("`{}`", fname))
            .collect();
        func_names.sort_unstable();
        let funcs_str = func_names.join(", ");
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("The following functions are available: {}", funcs_str),
        ).await;
    }

    async fn handle_factor(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let number: BigUint = match command.rest.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to parse that as a natural number."),
                ).await;
                return;
            },
        };

        // handle pathological cases with grace
        let two = BigUint::from(2u8);
        if number < two {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("\\[{0} = {0}\\]", number),
            ).await;
            return;
        }

        let prime_cache_mutex = Arc::clone(&self.prime_cache);
        let factors_tr = run_stoppable_task_timeout(
            Duration::from_secs_f64(config_guard.timeout_seconds),
            move |stopper| {
                let mut prime_cache_guard = prime_cache_mutex.blocking_lock();
                prime_cache_guard.factor_caching(&number, &stopper)
            },
        ).await;

        let factors = match factors_tr {
            StoppableTaskResult::ChannelBreakdown => {
                error!("factoring result channel broke down");
                return;
            },
            StoppableTaskResult::TaskPanicked(e) => {
                error!("factoring task panicked: {}", e);
                return;
            },
            StoppableTaskResult::Timeout => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "That took too long.",
                ).await;
                return;
            },
            StoppableTaskResult::Success(fs) => fs,
        };

        let mut factor_strings = Vec::with_capacity(factors.len());
        let one = BigUint::from(1u8);
        let nine = BigUint::from(9u8);
        for (factor, power) in &factors {
            let mut factor_string = factor.to_string();

            add_thousands_separators(&mut factor_string, "\\,");

            if power == &one {
                factor_strings.push(factor_string);
            } else if power <= &nine {
                factor_strings.push(format!("{}^{}", factor_string, power));
            } else {
                let mut power_string = power.to_string();
                add_thousands_separators(&mut power_string, "\\,");

                factor_strings.push(format!("{}^{}{}{}", factor_string, '{', power_string, '}'));
            }
        }

        let factor_string = factor_strings.join("\\cdot ");
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("\\[{}\\]", factor_string),
        ).await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let timeout_seconds = config["timeout_seconds"].as_f64()
            .ok_or("timeout_seconds missing or not representable as f64")?;
        let max_result_string_length = config["max_result_string_length"].as_usize()
            .ok_or("max_result_string_length missing or not representable as usize")?;
        let currency_units = config["currency_units"].as_bool()
            .ok_or("currency_units missing or not representable as boolean")?;

        let unit_db_file_value = &config["unit_database_file"];
        let unit_database = if unit_db_file_value.is_null() {
            UnitDatabase::new_empty()
        } else {
            let unit_db_file = unit_db_file_value.as_str()
                .ok_or("unit_database_file not a string")?;
            let mut f = File::open(unit_db_file)
                .or_msg("failed to open unit_database_file")?;
            let mut unit_db_toml = Vec::new();
            f.read_to_end(&mut unit_db_toml)
                .or_msg("failed to read unit_database_file")?;
            let unit_db: StoredUnitDatabase = toml::from_slice(&unit_db_toml)
                .or_msg("failed to load unit_database_file")?;
            unit_db.to_unit_database()
                .or_msg("failed to process unit database file")?
        };

        Ok(Config {
            timeout_seconds,
            max_result_string_length,
            currency_units,
            unit_database,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for CalcPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> CalcPlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "CalcPlugin::config",
            config_object,
        );

        let last_currency_update = Mutex::new(
            "CalcPlugin::last_currency_update",
            Utc.ymd(2000, 1, 1)
                .and_hms(0, 0, 0)
        );

        let prime_cache = Arc::new(Mutex::new(
            "CalcPlugin::prime_cache",
            PrimeCache::new(),
        ));

        my_interface.register_channel_command(&CommandDefinition::new(
            "calc".to_owned(),
            "calc".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::NO_ARGUMENT_PARSING,
            "{cpfx}calc EXPRESSION".to_owned(),
            "Calculates the given mathematical expression and outputs the result.".to_owned(),
        )).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "calcconst".to_owned(),
                "calc".to_owned(),
                "{cpfx}calcconst".to_owned(),
                "Lists available calculator constants.".to_owned(),
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "calcfunc".to_owned(),
                "calc".to_owned(),
                "{cpfx}calcfunc".to_owned(),
                "Lists available calculator functions.".to_owned(),
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "factor".to_owned(),
                "calc".to_owned(),
                "{cpfx}factor NUMBER".to_owned(),
                "Attempts to subdivide the given natural number into its prime factors.".to_owned(),
            )
                .build()
        ).await;

        CalcPlugin {
            interface,
            config: config_lock,
            last_currency_update,
            prime_cache,
        }
    }

    async fn plugin_name(&self) -> String {
        "calc".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "calc" {
            self.handle_calc(channel_message, command).await
        } else if command.name == "calcconst" {
            self.handle_calcconst(channel_message, command).await
        } else if command.name == "calcfunc" {
            self.handle_calcfunc(channel_message, command).await
        } else if command.name == "factor" {
            self.handle_factor(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "calc" {
            Some(include_str!("../help/calc.md").to_owned())
        } else if command_name == "calcconst" {
            Some(include_str!("../help/calcconst.md").to_owned())
        } else if command_name == "calcfunc" {
            Some(include_str!("../help/calcfunc.md").to_owned())
        } else if command_name == "factor" {
            Some(include_str!("../help/factor.md").to_owned())
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
                error!("failed to reload configuration: {}", e);
                false
            },
        }
    }
}
