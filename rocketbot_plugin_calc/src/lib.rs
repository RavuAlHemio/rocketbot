mod ast;
#[cfg(feature = "currency")]
mod currency;
mod factor;
mod grimoire;
mod known_primes;
mod numbers;
mod parsing;
mod units;


use std::collections::BTreeSet;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use num_bigint::BigUint;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, run_stoppable_task_timeout, RwLock, StoppableTaskResult};
use serde_json;
use toml;
use tracing::error;

use crate::ast::{AstNode, SimplificationState};
use crate::factor::{PrimeCache, PrimeFactors};
use crate::grimoire::{get_canonical_constants, get_canonical_functions};
use crate::numbers::NumberValue;
use crate::parsing::parse_full_expression;
use crate::units::{StoredUnitDatabase, UnitDatabase};


static FRACTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "\\s*",
    "(?P<numer>[0-9_]+)",
    "\\s*",
    "/",
    "\\s*",
    "(?P<denom>[0-9_]+)",
    "\\s*",
    "$",
)).expect("failed to compile fraction regex"));


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

        let ast_root = match parse_full_expression(&command.rest) {
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

        let config_copy = {
            let config_guard = self.config.read().await;
            (*config_guard).clone()
        };

        let simplified_res = {
            let mut state = SimplificationState {
                constants: get_canonical_constants(),
                functions: get_canonical_functions(),
                units: config_copy.unit_database.clone(),
                start_time: Instant::now(),
                timeout: Duration::from_secs_f64(config_copy.timeout_seconds),
            };
            ast_root.root_node.simplify(&mut state)
        };
        match simplified_res {
            Ok(ntn) => {
                let result_string = match &ntn.node {
                    AstNode::Number(i) => {
                        if ast_root.instructions.contains("tex") {
                            // TeX output requested
                            if ast_root.instructions.contains("hex") || ast_root.instructions.contains("oct") || ast_root.instructions.contains("bin") || ast_root.instructions.contains("trunc") {
                                // output in a different base (if convertible to an integer)
                                let i_integer = i.value.to_int_trunc();
                                if ast_root.instructions.contains("hex") {
                                    format!("\\({:#X}_{}16{}{}\\)", i_integer, '{', '}', i.units_to_tex())
                                } else if ast_root.instructions.contains("oct") {
                                    format!("\\({:#o}_{}8{}{}\\)", i_integer, '{', '}', i.units_to_tex())
                                } else if ast_root.instructions.contains("bin") {
                                    format!("\\({:#b}_{}2{}{}\\)", i_integer, '{', '}', i.units_to_tex())
                                } else {
                                    assert!(ast_root.instructions.contains("trunc"));
                                    if ast_root.instructions.contains("thou") {
                                        format!("\\({}{}\\)", i_integer.to_tex_string_thou_sep(), i.units_to_tex())
                                    } else {
                                        format!("\\({}{}\\)", i_integer, i.units_to_tex())
                                    }
                                }
                            } else if ast_root.instructions.contains("dms") {
                                // output as degrees-minutes-seconds
                                let f64_val = i.value.to_f64();
                                let deg = f64_val.trunc();
                                let min_s = (f64_val - deg) * 60.0;
                                let min = min_s.trunc();
                                let s = (min_s - min) * 60.0;
                                let (deg_string, s_string) = if ast_root.instructions.contains("thou") {
                                    (
                                        NumberValue::Float(deg).to_tex_string_thou_sep(),
                                        NumberValue::Float(s).to_tex_string_thou_sep(),
                                    )
                                } else {
                                    (
                                        deg.to_string(),
                                        s.to_string(),
                                    )
                                };
                                format!("\\({}°{}'{}''{}\\)", deg_string, min, s_string, i.units_to_tex())
                            } else if ast_root.instructions.contains("dm") {
                                // output as degrees-minutes
                                let f64_val = i.value.to_f64();
                                let deg = f64_val.trunc();
                                let min = (f64_val - deg) * 60.0;
                                let (deg_string, min_string) = if ast_root.instructions.contains("thou") {
                                    (
                                        NumberValue::Float(deg).to_tex_string_thou_sep(),
                                        NumberValue::Float(min).to_tex_string_thou_sep(),
                                    )
                                } else {
                                    (
                                        deg.to_string(),
                                        min.to_string(),
                                    )
                                };
                                format!("\\({}°{}'{}\\)", deg_string, min_string, i.units_to_tex())
                            } else {
                                // regular output
                                let num_string = if ast_root.instructions.contains("thou") {
                                    i.value.to_tex_string_thou_sep()
                                } else {
                                    i.value.to_string()
                                };
                                format!("\\({}{}\\)", num_string, i.units_to_tex())
                            }
                        } else {
                            if ast_root.instructions.contains("hex") || ast_root.instructions.contains("oct") || ast_root.instructions.contains("bin") || ast_root.instructions.contains("trunc") {
                                // output in a different base (if convertible to an integer)
                                let i_integer = i.value.to_int_trunc();
                                if ast_root.instructions.contains("hex") {
                                    format!("{:#X}", i_integer)
                                } else if ast_root.instructions.contains("oct") {
                                    format!("{:#o}", i_integer)
                                } else if ast_root.instructions.contains("bin") {
                                    format!("{:#b}", i_integer)
                                } else {
                                    assert!(ast_root.instructions.contains("trunc"));
                                    format!("{}", i_integer)
                                }
                            } else if ast_root.instructions.contains("dms") {
                                // output as degrees-minutes-seconds
                                let f64_val = i.value.to_f64();
                                let deg = f64_val.trunc();
                                let min_s = (f64_val - deg) * 60.0;
                                let min = min_s.trunc();
                                let s = (min_s - min) * 60.0;
                                format!("{}°{}'{}\"{}", deg, min, s, i.units_to_string())
                            } else if ast_root.instructions.contains("dm") {
                                // output as degrees-minutes
                                let f64_val = i.value.to_f64();
                                let deg = f64_val.trunc();
                                let min = (f64_val - deg) * 60.0;
                                format!("{}°{}'{}", deg, min, i.units_to_string())
                            } else {
                                // regular output
                                i.to_string()
                            }
                        }
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

                if result_string.len() > config_copy.max_result_string_length {
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

    async fn handle_calcconst(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let const_name = command.rest.as_str().trim();

        if const_name.len() > 0 {
            let const_help_opt = get_canonical_constants()
                .get(const_name)
                .map(|c| c.help_text.clone());
            if let Some(const_help) = const_help_opt {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("`{}`: {}", const_name, const_help),
                ).await;
            } else {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Constant not found.",
                ).await;
            }
        } else {
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
    }

    async fn handle_calcfunc(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let func_name = command.rest.as_str().trim();

        if func_name.len() > 0 {
            let func_help_opt = get_canonical_functions()
                .get(func_name)
                .map(|fi| fi.help_text.clone());
            if let Some(func_help) = func_help_opt {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &func_help,
                ).await;
            } else {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Function not found.",
                ).await;
            }
        } else {
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
    }

    async fn handle_calcunit(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let all_unit_names = {
            let config = self.config.read().await;
            let mut all_unit_names: BTreeSet<String> = BTreeSet::new();
            for unit in config.unit_database.get_base_unit_names() {
                all_unit_names.insert(unit);
            }
            for unit in config.unit_database.get_derived_unit_names() {
                all_unit_names.insert(unit);
            }
            all_unit_names
        };
        let unit_names: Vec<String> = all_unit_names.into_iter()
            .map(|un| format!("`{}`", un))
            .collect();
        let units_str = unit_names.join(", ");
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("The following units are known: {}", units_str),
        ).await;
    }

    async fn handle_factor(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let timeout = {
            let config_guard = self.config.read().await;
            Duration::from_secs_f64(config_guard.timeout_seconds)
        };

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

        let factors = if let Some(pat) = PrimeFactors::pathological(&number) {
            // handle pathological cases with grace
            pat
        } else {
            let prime_cache_mutex = Arc::clone(&self.prime_cache);
            let factors_tr = run_stoppable_task_timeout(
                timeout,
                move |stopper| {
                    let mut prime_cache_guard = prime_cache_mutex.blocking_lock();
                    prime_cache_guard.factor_caching(&number, &stopper)
                },
            ).await;

            match factors_tr {
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
            }
        };

        let output_as_code = command.flags.contains("c") || command.flags.contains("code");
        let factor_string = if output_as_code {
            factors.to_code_string()
        } else {
            factors.to_tex_string()
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &factor_string,
        ).await;
    }

    async fn handle_redfrac(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let caps = match FRACTION_RE.captures(&command.rest) {
            Some(c) => c,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to parse fraction.",
                ).await;
                return;
            },
        };

        let numerator: BigUint = caps
            .name("numer").expect("missing numerator capture")
            .as_str()
            .replace("_", "")
            .parse().expect("failed to parse numerator");
        let denominator: BigUint = caps
            .name("denom").expect("missing denominator capture")
            .as_str()
            .replace("_", "")
            .parse().expect("failed to parse denominator");

        let zero = BigUint::from(0u8);

        if denominator == zero {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "lol.",
            ).await;
            return;
        }

        // Euclidean
        let mut a = numerator.clone();
        let mut b = denominator.clone();
        while b > zero {
            let a_mod_b = a % &b;
            a = b;
            b = a_mod_b;
        }

        let new_numerator = numerator / &a;
        let new_denominator = denominator / &a;

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("{}/{}", new_numerator, new_denominator),
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
            let unit_db_toml = std::fs::read_to_string(unit_db_file)
                .or_msg("failed to read unit_database_file")?;
            let unit_db: StoredUnitDatabase = toml::from_str(&unit_db_toml)
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
            Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap()
        );

        let prime_cache = Arc::new(Mutex::new(
            "CalcPlugin::prime_cache",
            PrimeCache::new_until_100k_th(),
        ));

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "calc",
                "calc",
                "{cpfx}calc EXPRESSION",
                "Calculates the given mathematical expression and outputs the result.",
            )
                .behaviors(CommandBehaviors::NO_ARGUMENT_PARSING)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "calcconst",
                "calc",
                "{cpfx}calcconst",
                "Lists available calculator constants.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "calcfunc",
                "calc",
                "{cpfx}calcfunc",
                "Lists available calculator functions.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "calcunit",
                "calc",
                "{cpfx}calcunit",
                "Lists available calculator units.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "factor",
                "calc",
                "{cpfx}factor NUMBER",
                "Attempts to subdivide the given natural number into its prime factors.",
            )
                .add_flag("c").add_flag("code")
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "redfrac",
                "calc",
                "{cpfx}redfrac NUMERATOR/DENOMINATOR",
                "Reduces the given fraction.",
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
        } else if command.name == "calcunit" {
            self.handle_calcunit(channel_message, command).await
        } else if command.name == "factor" {
            self.handle_factor(channel_message, command).await
        } else if command.name == "redfrac" {
            self.handle_redfrac(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "calc" {
            Some(include_str!("../help/calc.md").to_owned())
        } else if command_name == "calcconst" {
            Some(include_str!("../help/calcconst.md").to_owned())
        } else if command_name == "calcfunc" {
            Some(include_str!("../help/calcfunc.md").to_owned())
        } else if command_name == "calcunit" {
            Some(include_str!("../help/calcunit.md").to_owned())
        } else if command_name == "factor" {
            Some(include_str!("../help/factor.md").to_owned())
        } else if command_name == "redfrac" {
            Some(include_str!("../help/redfrac.md").to_owned())
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
