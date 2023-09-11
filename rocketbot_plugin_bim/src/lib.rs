mod achievements;
mod range_set;
pub mod table_draw;


use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::collections::hash_map;
use std::fmt::{self, Write};
use std::fs::File;
use std::io::Cursor;
use std::num::ParseIntError;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use chrono::{
    Datelike, DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, Timelike, TimeZone,
    Weekday,
};
use log::{debug, error, info};
use once_cell::sync::OnceCell;
use rand::{Rng, thread_rng};
use regex::Regex;
use rocketbot_bim_common::{CouplingMode, VehicleInfo, VehicleNumber};
use rocketbot_bim_common::achievements::ACHIEVEMENT_DEFINITIONS;
use rocketbot_interface::{JsonValueExtensions, phrase_join, ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachmentBuilder};
use rocketbot_interface::serde::serde_opt_regex;
use rocketbot_interface::sync::RwLock;
use rocketbot_render_text::map_to_png;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio::sync::mpsc;
use tokio_postgres::NoTls;
use tokio_postgres::types::ToSql;

use crate::achievements::{get_all_achievements, recalculate_achievements};
use crate::range_set::RangeSet;
use crate::table_draw::{draw_ride_table, Ride, RideTableData, RideTableVehicle, UserRide};


const INVISIBLE_JOINER: &str = "\u{2060}"; // WORD JOINER
const TIMESTAMP_INPUT_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.f";


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum LookbackRange {
    SinceBeginning,
    LastYear,
    LastMonth,
    LastWeek,
    LastDay,
}
impl LookbackRange {
    pub fn days(&self) -> Option<i64> {
        match self {
            Self::SinceBeginning => None,
            Self::LastYear => Some(366),
            Self::LastMonth => Some(31), // yeah, I know
            Self::LastWeek => Some(7),
            Self::LastDay => Some(1),
        }
    }

    pub fn start_timestamp(&self) -> Option<DateTime<Local>> {
        self.days()
            .map(|d| Local::now() - Duration::days(d))
    }
}
impl Default for LookbackRange {
    fn default() -> Self { Self::SinceBeginning }
}


trait AddLookbackFlags {
    fn add_lookback_flags(self) -> Self;
}
impl AddLookbackFlags for CommandDefinitionBuilder {
    fn add_lookback_flags(self) -> Self {
        self
            .add_flag("m")
            .add_flag("last-month")
            .add_flag("y")
            .add_flag("last-year")
            .add_flag("w")
            .add_flag("last-week")
            .add_flag("d")
            .add_flag("last-day")
    }
}


pub type RegionToLineToOperator = HashMap<String, HashMap<String, LineOperatorInfo>>;


macro_rules! write_expect {
    ($dst:expr, $($arg:tt)*) => {
        write!($dst, $($arg)*).expect("write failed")
    };
}


#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct LastRideInfo {
    pub ride_count: usize,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct OtherRiderInfo {
    pub all_other_rider_count: usize,
    pub rider_username: String,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}
impl OtherRiderInfo {
    pub fn is_same_ride(&self, other: &Self) -> bool {
        self.rider_username == other.rider_username
            && self.last_ride == other.last_ride
            && self.last_line == other.last_line
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct VehicleRideInfo {
    pub vehicle_number: VehicleNumber,
    pub coupling_mode: CouplingMode,
    pub last_ride: Option<LastRideInfo>,
    pub last_actual_ride_other_rider: Option<OtherRiderInfo>,
    pub last_ride_other_rider: Option<OtherRiderInfo>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideInfo {
    pub ride_id: i64,
    pub line: Option<String>,
    pub vehicles: Vec<VehicleRideInfo>,
}


#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct NewVehicleEntry {
    pub number: VehicleNumber,
    pub type_code: Option<String>,
    pub spec_position: i64,
    pub coupling_mode: CouplingMode,
    pub fixed_coupling_position: i64,
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct BimTypeStats {
    pub known_vehicles: usize,
    pub active_vehicles: usize,
    pub ridden_vehicles: usize,
}
impl BimTypeStats {
    pub fn new() -> Self {
        Self {
            known_vehicles: 0,
            active_vehicles: 0,
            ridden_vehicles: 0,
        }
    }

    pub fn active_known(&self) -> f64 {
        self.active_vehicles as f64 / self.known_vehicles as f64
    }

    pub fn ridden_known(&self) -> f64 {
        self.ridden_vehicles as f64 / self.known_vehicles as f64
    }

    pub fn ridden_active(&self) -> f64 {
        self.ridden_vehicles as f64 / self.active_vehicles as f64
    }
}


#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CompanyDefinition {
    pub name: String,
    pub country: String,
    pub bim_database_path: Option<String>,
    #[serde(with = "serde_opt_regex")] pub vehicle_number_regex: Option<Regex>,
    #[serde(with = "serde_opt_regex")] pub line_number_regex: Option<Regex>,
    #[serde(skip)] vehicle_and_line_regex: OnceCell<Regex>,
}
impl CompanyDefinition {
    pub fn vehicle_and_line_regex(&self) -> &Regex {
        if let Some(valr) = self.vehicle_and_line_regex.get() {
            return valr;
        }

        let vehicle_number_rstr = self.vehicle_number_regex
            .as_ref()
            .map(|r| r.as_str())
            .unwrap_or(".+");
        let line_number_rstr = self.line_number_regex
            .as_ref()
            .map(|r| r.as_str())
            .unwrap_or(".+");

        // {vehicle}
        // {vehicle}/{line}
        // {vehicle}+{vehicle}+{vehicle}
        // {vehicle}+{vehicle}!+{vehicle}
        // {vehicle}+{vehicle}+{vehicle}/line
        // {vehicle}+{vehicle}!+{vehicle}/line
        // {line}:{vehicle}
        // {line}:{vehicle}+{vehicle}+{vehicle}
        // {line}:{vehicle}+{vehicle}!+{vehicle}
        let valr_string = format!(
            concat!(
                "^",
                "(?:",
                    "(?:",
                        "(?P<vehicles>",
                            "(?:{}[!]?)",
                            "(?:",
                                "[+]",
                                "(?:{}[!]?)",
                            ")*",
                        ")",
                        "(?:",
                            "[/]",
                            "(?P<line>",
                                "(?:{})",
                            ")",
                        ")?",
                    ")",
                "|",
                    "(?:",
                        "(?P<line_lv>",
                            "(?:{})",
                        ")",
                        ":",
                        "(?P<vehicles_lv>",
                            "(?:{}[!]?)",
                            "(?:",
                                "[+]",
                                "(?:{}[!]?)",
                            ")*",
                        ")",
                    ")",
                ")",
                "$"
            ),
            vehicle_number_rstr, vehicle_number_rstr, line_number_rstr,
            line_number_rstr, vehicle_number_rstr, vehicle_number_rstr,
        );
        let valr = Regex::new(&valr_string)
            .expect("failed to assemble vehicle-and-line regex");
        self.vehicle_and_line_regex.set(valr)
            .expect("failed to set vehicle-and-line regex");
        self.vehicle_and_line_regex.get()
            .expect("failed to get freshly-set vehicle-and-line regex")
    }

    pub fn placeholder() -> Self {
        Self {
            name: "Placeholder".to_owned(),
            country: "aq".to_owned(),
            bim_database_path: None,
            vehicle_number_regex: None,
            line_number_regex: None,
            vehicle_and_line_regex: OnceCell::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LineOperatorInfo {
    pub canonical_line: String,
    pub operator_name: String,
    pub operator_abbrev: Option<String>,
}


#[derive(Clone, Debug)]
struct Config {
    company_to_definition: HashMap<String, CompanyDefinition>,
    default_company: String,
    manufacturer_mapping: HashMap<String, String>,
    ride_db_conn_string: String,
    allow_fixed_coupling_combos: bool,
    admin_usernames: HashSet<String>,
    max_edit_s: i64,
    achievements_active: bool,
    operator_databases: HashSet<String>,
    default_operator_region: String,
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct UpdateAchievementsData {
    pub channel: String,
    pub explicit: bool,
}


pub struct BimPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: Arc<RwLock<Config>>,
    achievement_update_sender: mpsc::UnboundedSender<UpdateAchievementsData>,
}
impl BimPlugin {
    fn load_bim_database(&self, config: &Config, company: &str) -> Option<HashMap<VehicleNumber, VehicleInfo>> {
        let path_opt = match config.company_to_definition.get(company) {
            Some(p) => p.bim_database_path.as_ref(),
            None => {
                error!("unknown company {:?}", company);
                return None;
            },
        };
        let path = match path_opt {
            Some(p) => p,
            None => return None, // valid company but no database
        };
        let f = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("failed to open bim database: {}", e);
                return None;
            },
        };
        let mut vehicles: Vec<VehicleInfo> = match ciborium::from_reader(f) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse bim database: {}", e);
                return None;
            },
        };
        let vehicle_hash_map: HashMap<VehicleNumber, VehicleInfo> = vehicles.drain(..)
            .map(|vi| (vi.number.clone(), vi))
            .collect();
        Some(vehicle_hash_map)
    }

    fn load_operator_databases(&self, config: &Config) -> Option<RegionToLineToOperator> {
        let mut region_to_line_to_operator: RegionToLineToOperator = HashMap::new();

        for db_path in &config.operator_databases {
            let f = match File::open(db_path) {
                Ok(f) => f,
                Err(e) => {
                    error!("failed to open operator database {:?}: {}", db_path, e);
                    return None;
                },
            };
            let this_region_to_line_to_operator: RegionToLineToOperator = match serde_json::from_reader(f) {
                Ok(v) => v,
                Err(e) => {
                    error!("failed to parse bim database {:?}: {}", db_path, e);
                    return None;
                },
            };
            for (this_region, this_line_to_operator) in this_region_to_line_to_operator {
                let line_to_operator = region_to_line_to_operator
                    .entry(this_region)
                    .or_insert_with(|| HashMap::new());
                for (this_line, this_operator) in this_line_to_operator {
                    line_to_operator.insert(this_line, this_operator);
                }
            }
        }

        Some(region_to_line_to_operator)
    }

    fn lookback_range_from_command(command: &CommandInstance) -> Option<LookbackRange> {
        let last_month =
            command.flags.contains("m")
            || command.flags.contains("last-month")
        ;
        let last_year =
            command.flags.contains("y")
            || command.flags.contains("last-year")
        ;
        let last_week =
            command.flags.contains("w")
            || command.flags.contains("last-week")
        ;
        let last_day =
            command.flags.contains("d")
            || command.flags.contains("last-day")
        ;

        match (last_year, last_month, last_week, last_day) {
            (true, false, false, false) => Some(LookbackRange::LastYear),
            (false, true, false, false) => Some(LookbackRange::LastMonth),
            (false, false, true, false) => Some(LookbackRange::LastWeek),
            (false, false, false, true) => Some(LookbackRange::LastDay),
            (false, false, false, false) => Some(LookbackRange::SinceBeginning),
            _ => None,
        }
    }

    async fn timestamp_query(
        conn: &tokio_postgres::Client,
        query_template: &str,
        timestamp_block: &str,
        no_timestamp_block: &str,
        lookback_range: LookbackRange,
        other_params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, tokio_postgres::Error> {
        let lookback_timestamp_opt = lookback_range.start_timestamp();

        let mut new_params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(other_params.len() + 1);
        new_params.extend(other_params);

        if let Some(lt) = lookback_timestamp_opt {
            new_params.push(&lt);
            let query = query_template.replace("{LOOKBACK_TIMESTAMP}", timestamp_block);
            conn.query(&query, &new_params).await
        } else {
            let query = query_template.replace("{LOOKBACK_TIMESTAMP}", no_timestamp_block);
            conn.query(&query, &new_params).await
        }
    }

    async fn channel_command_bim(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let number_str = command.rest.trim();
        let number = match number_str.parse() {
            Ok(n) => VehicleNumber::from_string(n),
            Err(e) => {
                error!("failed to parse {:?} as VehicleNumber: {}", number_str, e);
                return;
            },
        };

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(config_guard.default_company.as_str());

        let mut response = match self.load_bim_database(&config_guard, company) {
            None => "No vehicle database exists for this company.".to_owned(),
            Some(db) => {
                match db.get(&number) {
                    None => format!("Vehicle {} not found.", number),
                    Some(vehicle) => {
                        let mut db_response = format!(
                            "*{}*: type *{}*",
                            vehicle.number, vehicle.type_code,
                        );
                        match (&vehicle.in_service_since, &vehicle.out_of_service_since) {
                            (Some(service_from), Some(service_to)) => {
                                write_expect!(db_response, ", in service from {} to {}", service_from, service_to);
                            },
                            (Some(service_from), None) => {
                                write_expect!(db_response, ", in service since {}", service_from);
                            },
                            (None, Some(service_to)) => {
                                write_expect!(db_response, ", in service until {}", service_to);
                            },
                            (None, None) => {},
                        };

                        if let Some(manuf) = &vehicle.manufacturer {
                            let full_manuf = config_guard.manufacturer_mapping.get(manuf).unwrap_or(manuf);
                            write_expect!(db_response, "\n*hergestellt von* {}", full_manuf);
                        }

                        let mut other_props: Vec<(&str, &str)> = vehicle.other_data.iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();
                        other_props.sort_unstable();
                        for (key, val) in other_props {
                            write_expect!(db_response, "\n*{}*: {}", key, val);
                        }

                        if vehicle.fixed_coupling.len() > 0 {
                            let fixed_coupling_strings: Vec<String> = vehicle.fixed_coupling.iter()
                                .map(|num| num.to_string())
                                .collect();
                            let fixed_coupling_string = fixed_coupling_strings.join("+");
                            write_expect!(db_response, "\npart of fixed coupling: {}", fixed_coupling_string);
                        }

                        db_response
                    }
                }
            },
        };

        async fn get_last_ride(config: &Config, username: &str, company: &str, vehicle_number: &VehicleNumber) -> Option<String> {
            let ride_conn = match connect_ride_db(config).await {
                Ok(c) => c,
                Err(_) => return None,
            };

            // query ride count
            let count_row_opt_res = ride_conn.query_opt(
                "
                    SELECT
                        CAST(COALESCE(COUNT(*), 0) AS bigint) total_ride_count
                    FROM
                        bim.rides r
                        INNER JOIN bim.ride_vehicles rv
                            ON rv.ride_id = r.id
                    WHERE
                        r.company = $1
                        AND rv.vehicle_number = $2
                ",
                &[&company, &vehicle_number.as_str()],
            ).await;
            let count: i64 = match count_row_opt_res {
                Ok(Some(cr)) => cr.get(0),
                Ok(None) => 0,
                Err(e) => {
                    error!("failed to obtain ride count: {}", e);
                    return None;
                },
            };

            let mut ret = if count == 0 {
                format!("This vehicle has not been ridden yet.")
            } else {
                format!("This vehicle has been ridden {}.", BimPlugin::english_adverbial_number(count))
            };

            let now = Local::now();
            for (is_you, operator) in &[(true, "="), (false, "<>")] {
                let ride_row_opt_res = ride_conn.query_opt(
                    &format!(
                        "
                            SELECT
                                r.rider_username,
                                r.\"timestamp\",
                                r.line
                            FROM
                                bim.rides r
                                INNER JOIN bim.ride_vehicles rv
                                    ON rv.ride_id = r.id
                            WHERE
                                r.company = $1
                                AND rv.vehicle_number = $2
                                AND r.rider_username {} $3
                            ORDER BY
                                r.\"timestamp\" DESC
                            LIMIT 1
                        ",
                        operator,
                    ),
                    &[&company, &vehicle_number.as_str(), &username],
                ).await;
                match ride_row_opt_res {
                    Ok(Some(lrr)) => {
                        let last_rider_username: String = lrr.get(0);
                        let last_ride: DateTime<Local> = lrr.get(1);
                        let last_line: Option<String> = lrr.get(2);

                        write_expect!(ret,
                            " {} last rode it {}",
                            if *is_you { "You" } else { last_rider_username.as_str() },
                            BimPlugin::canonical_date_format_relative(&last_ride, &now, true, true),
                        );
                        if let Some(ll) = last_line {
                            write_expect!(ret, " on line {}", ll);
                        }
                        ret.push('.');
                    },
                    Ok(None) => {},
                    Err(e) => {
                        error!("failed to obtain last rider (is_you={:?}): {}", is_you, e);
                        return None;
                    },
                };
            }

            Some(ret)
        }
        if let Some(last_ride) = get_last_ride(&config_guard, &channel_message.message.sender.username, &company, &number).await {
            write_expect!(response, "\n{}", last_ride);
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_bimride(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let is_admin = config_guard.admin_usernames.contains(&channel_message.message.sender.username);
        let alternate_rider = command.options.get("rider")
            .or_else(|| command.options.get("r"));
        let alternate_timestamp_string = command.options.get("timestamp")
            .or_else(|| command.options.get("t"));
        let utc_timestamp = command.flags.contains("utc") || command.flags.contains("u");
        let sandbox = command.flags.contains("sandbox") || command.flags.contains("s");

        if (alternate_rider.is_some() || alternate_timestamp_string.is_some()) && !is_admin {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "-r/--rider and -t/--timestamp can only be used by `bim` administrators!",
            ).await;
            return;
        }

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(config_guard.default_company.as_str());

        let company_def = match config_guard.company_to_definition.get(company) {
            Some(cd) => cd,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Unknown company.",
                ).await;
                return;
            }
        };

        let ride_timestamp = if let Some(ats) = alternate_timestamp_string {
            let timestamp_opt = self.parse_user_timestamp(
                ats.as_str().expect("timestamp string not a string?!"),
                utc_timestamp,
                &channel_message.channel.name,
            ).await;
            match timestamp_opt {
                Some(t) => t,
                None => return, // error message already output
            }
        } else {
            channel_message.message.timestamp.with_timezone(&Local)
        };

        let rider_username = if let Some(ar) = alternate_rider {
            ar.as_str().expect("explicit rider not a string?!")
        } else {
            channel_message.message.sender.username.as_str()
        };

        let bim_database_opt = self.load_bim_database(&config_guard, company);
        let mut ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let increment_res = increment_rides_by_spec(
            &mut ride_conn,
            bim_database_opt.as_ref(),
            company,
            company_def,
            rider_username,
            ride_timestamp,
            &command.rest,
            config_guard.allow_fixed_coupling_combos,
            sandbox,
        ).await;
        let ride_table = match increment_res {
            Ok(lri) => lri,
            Err(IncrementBySpecError::SpecParseFailure(spec)) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to parse line specification {:?}.", spec),
                ).await;
                return;
            },
            Err(IncrementBySpecError::VehicleNumberParseFailure(vn, _error)) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to parse vehicle number {:?}.", vn),
                ).await;
                return;
            },
            Err(IncrementBySpecError::FixedCouplingCombo(vn)) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Vehicle with number {} is part of a fixed coupling and must therefore appear alone.", vn),
                ).await;
                return;
            },
            Err(e) => {
                error!("increment-by-spec error: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "A database error occurred. :disappointed:",
                ).await;
                return;
            },
        };

        // render the table
        let table_canvas = draw_ride_table(&ride_table);
        let mut png_buf = Vec::new();
        {
            let cursor = Cursor::new(&mut png_buf);
            const MARGIN: u32 = 8;
            map_to_png(cursor, &table_canvas, MARGIN, MARGIN, MARGIN, MARGIN)
                .expect("failed to write PNG data");
        }

        let attachment = Attachment::new(
            png_buf,
            format!("ride{}.png", ride_table.ride_id),
            "image/png".to_owned(),
            None,
        );
        interface.send_channel_message_with_attachment(
            &channel_message.channel.name,
            OutgoingMessageWithAttachmentBuilder::new(attachment)
                .build()
        ).await;

        // signal to update achievements
        if config_guard.achievements_active {
            let data = UpdateAchievementsData {
                channel: channel_message.channel.name.clone(),
                explicit: false,
            };
            let _ = self.achievement_update_sender.send(data);
        }
    }

    async fn channel_command_topbims(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let company_opt = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap());
        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };

        if let Some(c) = company_opt {
            if !config_guard.company_to_definition.contains_key(c) {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Unknown company.",
                ).await;
                return;
            }
        }

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let company_stored;
        let mut other_params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        let (company_block, timestamp_block) = if let Some(c) = company_opt {
            company_stored = c.to_owned();
            other_params.push(&company_stored);
            ("AND r.company = $1", "AND r.\"timestamp\" >= $2")
        } else {
            ("", "AND r.\"timestamp\" >= $1")
        };
        let query_template = format!(
            "
                WITH
                    total_rides(company, vehicle_number, total_ride_count) AS (
                        SELECT
                            r.company,
                            rv.vehicle_number,
                            CAST(COUNT(*) AS bigint) total_ride_count
                        FROM
                            bim.rides r
                            INNER JOIN bim.ride_vehicles rv
                                ON rv.ride_id = r.id
                        WHERE
                            rv.fixed_coupling_position = 0
                            {}
                            {{LOOKBACK_TIMESTAMP}}
                        GROUP BY
                            r.company,
                            rv.vehicle_number
                    ),
                    top_five_counts(total_ride_count) AS (
                        SELECT DISTINCT total_ride_count
                        FROM total_rides
                        ORDER BY total_ride_count DESC
                        LIMIT 5
                    )
                SELECT tr.company, tr.vehicle_number, tr.total_ride_count
                FROM total_rides tr
                WHERE tr.total_ride_count IN (SELECT total_ride_count FROM top_five_counts)
                ORDER BY tr.total_ride_count DESC, tr.vehicle_number USING OPERATOR(bim.<~<)
            ",
            company_block,
        );

        let rows_res = Self::timestamp_query(
            &ride_conn,
            &query_template,
            timestamp_block,
            "",
            lookback_range,
            other_params.as_slice(),
        ).await;
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most-ridden vehicles: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut count_to_vehicles: BTreeMap<i64, Vec<(String, String)>> = BTreeMap::new();
        for row in &rows {
            let company: String = row.get(0);
            let vehicle_number: String = row.get(1);
            let total_ride_count: i64 = row.get(2);

            count_to_vehicles
                .entry(total_ride_count)
                .or_insert_with(|| Vec::new())
                .push((company, vehicle_number));
        }

        let response_str = if count_to_vehicles.len() == 0 {
            format!("No vehicles have been ridden yet!")
        } else {
            let mut output = format!("The most ridden vehicles are:");
            for (&count, vehicle_numbers) in count_to_vehicles.iter().rev() {
                let times = Self::english_adverbial_number(count);

                let vehicle_number_strings: Vec<String> = vehicle_numbers.iter()
                    .map(|(comp, vn)|
                        if let Some(c) = company_opt {
                            if comp == c {
                                vn.to_string()
                            } else {
                                format!("{}/{}", comp, vn)
                            }
                        } else if comp == &config_guard.default_company {
                            vn.to_string()
                        } else {
                            format!("{}/{}", comp, vn)
                        }
                    )
                    .collect();
                let vehicle_numbers_str = phrase_join(&vehicle_number_strings, ", ", " and ");
                output.push_str(&format!("\n{}: {}", times, vehicle_numbers_str));
            }
            output
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
    }

    async fn channel_command_topriders(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let ride_rows_res = Self::timestamp_query(
            &ride_conn,
            "
                SELECT r.rider_username, CAST(COUNT(*) AS bigint) ride_count
                FROM bim.rides r
                {LOOKBACK_TIMESTAMP}
                GROUP BY r.rider_username
            ",
            "WHERE r.\"timestamp\" >= $1",
            "",
            lookback_range,
            &[],
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most active riders: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut rider_to_ride_and_vehicle_count = HashMap::new();
        for row in ride_rows {
            let rider_username: String = row.get(0);
            let ride_count: i64 = row.get(1);

            let rider_ride_and_vehicle_count = rider_to_ride_and_vehicle_count
                .entry(rider_username.clone())
                .or_insert((0i64, 0i64));
            rider_ride_and_vehicle_count.0 += ride_count;
        }

        let vehicle_rows_res = Self::timestamp_query(
            &ride_conn,
            "
                SELECT i.rider_username, CAST(COUNT(*) AS bigint) vehicle_count
                FROM (
                    SELECT DISTINCT r.rider_username, r.company, rv.vehicle_number
                    FROM bim.rides r
                    INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
                    WHERE rv.spec_position = 0
                    AND rv.fixed_coupling_position = 0
                    {LOOKBACK_TIMESTAMP}
                ) i
                GROUP BY i.rider_username
            ",
            "AND r.\"timestamp\" >= $1",
            "",
            lookback_range,
            &[],
        ).await;
        let vehicle_rows = match vehicle_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most active riders with vehicles: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        for row in vehicle_rows {
            let rider_username: String = row.get(0);
            let vehicle_count: i64 = row.get(1);

            let rider_ride_and_vehicle_count = rider_to_ride_and_vehicle_count
                .entry(rider_username.clone())
                .or_insert((0i64, 0i64));
            rider_ride_and_vehicle_count.1 += vehicle_count;
        }

        let mut rider_and_ride_and_vehicle_count: Vec<(String, i64, i64)> = rider_to_ride_and_vehicle_count
            .iter()
            .map(|(r, (rc, vc))| (r.clone(), *rc, *vc))
            .collect();
        rider_and_ride_and_vehicle_count.sort_unstable_by_key(|(r, rc, _vc)| (-*rc, r.clone()));
        let mut rider_strings: Vec<String> = rider_and_ride_and_vehicle_count.iter()
            .map(|(rider_name, ride_count, vehicle_count)| {
                let ride_text = if *ride_count == 1 {
                    "one ride".to_owned()
                } else {
                    format!("{} rides", ride_count)
                };
                let vehicle_text = if *vehicle_count == 1 {
                    "one vehicle".to_owned()
                } else {
                    format!("{} vehicles", vehicle_count)
                };
                let uniqueness_percentage = (*vehicle_count as f64) * 100.0 / (*ride_count as f64);

                format!("{} ({} in {}; {:.2}% unique)", rider_name, ride_text, vehicle_text, uniqueness_percentage)
            })
            .collect();
        let prefix = if rider_strings.len() < 6 {
            "Top riders:\n"
        } else {
            rider_strings.drain(5..);
            "Top 5 riders:\n"
        };
        let riders_string = rider_strings.join("\n");

        let response_string = if riders_string.len() == 0 {
            "No top riders.".to_owned()
        } else {
            format!("{}{}", prefix, riders_string)
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_string,
        ).await;
    }

    async fn channel_command_bimcompanies(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        if config_guard.company_to_definition.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "There are no companies.",
            ).await;
            return;
        }

        let mut country_to_company_to_name: BTreeMap<&String, BTreeMap<&String, &String>> = BTreeMap::new();
        for (company_id, company_def) in &config_guard.company_to_definition {
            country_to_company_to_name
                .entry(&company_def.country)
                .or_insert_with(|| BTreeMap::new())
                .insert(company_id, &company_def.name);
        }

        let mut response_str = "The following companies exist:".to_owned();
        for (country, company_to_name) in country_to_company_to_name {
            write_expect!(&mut response_str, "\n\n:flag_{}:", country);
            for (abbr, name) in company_to_name {
                write_expect!(&mut response_str, "\n* `{}` ({})", abbr, name);
            }
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
    }

    async fn channel_command_favbims(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };
        let rider_username_input = command.rest.trim();
        let rider_username_opt = if rider_username_input.len() == 0 {
            None
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => Some(ru),
                None => Some(rider_username_input.to_owned()),
            }
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let rows_res = Self::timestamp_query(
            &ride_conn,
            "
                WITH
                    rides_per_rider_vehicle(rider_username, company, vehicle_number, ride_count) AS (
                        SELECT r.rider_username, r.company, rv.vehicle_number, COUNT(*)
                        FROM bim.rides r
                        INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
                        WHERE rv.fixed_coupling_position = 0
                        {LOOKBACK_TIMESTAMP}
                        GROUP BY r.rider_username, r.company, rv.vehicle_number
                    ),
                    rider_top_ride_counts(rider_username, ride_count) AS (
                        SELECT rcsq.rider_username, rcsq.ride_count
                        FROM (
                            SELECT rprvuq.rider_username, rprvuq.ride_count, rank() OVER (PARTITION BY rprvuq.rider_username ORDER BY rprvuq.ride_count DESC) ride_count_rank
                            FROM (
                                SELECT DISTINCT rprv2.rider_username, rprv2.ride_count
                                FROM rides_per_rider_vehicle rprv2
                            ) rprvuq
                        ) rcsq
                        WHERE rcsq.ride_count_rank < 6
                    ),
                    fav_vehicles(rider_username, company, vehicle_number, ride_count) AS (
                        SELECT rprv.rider_username, rprv.company, rprv.vehicle_number, rprv.ride_count
                        FROM rides_per_rider_vehicle rprv
                        WHERE
                            ride_count > 1
                            AND ride_count IN (
                                SELECT rtrc.ride_count
                                FROM rider_top_ride_counts rtrc
                                WHERE rtrc.rider_username = rprv.rider_username
                            )
                    )
                SELECT rider_username, company, vehicle_number, CAST(ride_count AS bigint)
                FROM fav_vehicles
            ",
            "AND r.\"timestamp\" >= $1",
            "",
            lookback_range,
            &[],
        ).await;
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most active riders: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut rider_to_fav_vehicles: BTreeMap<String, BTreeSet<(String, VehicleNumber, i64)>> = BTreeMap::new();
        for row in rows {
            let rider_username: String = row.get(0);
            let company: String = row.get(1);
            let vehicle_number = VehicleNumber::from_string(row.get(2));
            let ride_count: i64 = row.get(3);

            if let Some(ru) = rider_username_opt.as_ref() {
                if &rider_username != ru {
                    continue;
                }
            }

            rider_to_fav_vehicles
                .entry(rider_username)
                .or_insert_with(|| BTreeSet::new())
                .insert((company, vehicle_number, ride_count));
        }

        let mut fav_vehicle_strings = Vec::new();
        if rider_username_opt.is_some() {
            // output all
            let mut db_rider_username = None;
            let mut ride_count_to_vehicles: BTreeMap<i64, BTreeSet<(String, VehicleNumber)>> = BTreeMap::new();
            for (rider, fav_vehicles) in rider_to_fav_vehicles.iter() {
                if db_rider_username.is_none() {
                    db_rider_username = Some(rider.clone());
                }
                for (comp, veh_no, ride_ct) in fav_vehicles {
                    ride_count_to_vehicles
                        .entry(*ride_ct)
                        .or_insert_with(|| BTreeSet::new())
                        .insert((comp.clone(), veh_no.clone()));
                }
            }

            if let Some(dbru) = db_rider_username {
                fav_vehicle_strings.push(format!("{}'s favorite vehicles:", dbru));
            } else {
                fav_vehicle_strings.push(format!("This rider has no favorite vehicles!"));
            }

            for (ride_count, vehicles) in ride_count_to_vehicles.iter().rev() {
                let vehicle_strs: Vec<String> = vehicles
                    .iter()
                    .map(|(comp, vnr)|
                        if comp == &config_guard.default_company {
                            format!("{}", vnr)
                        } else {
                            format!("{}/{}", comp, vnr)
                        }
                    )
                    .collect();
                let vehicles_str = vehicle_strs.join(", ");
                fav_vehicle_strings.push(format!("{}: {}", ride_count, vehicles_str));
            }
        } else {
            // only consider those that match the absolute maximum ride count
            for (_rider, fav_vehicles) in rider_to_fav_vehicles.iter_mut() {
                let max_ride_count = fav_vehicles
                    .iter()
                    .map(|(_comp, _veh_no, ride_count)| *ride_count)
                    .max()
                    .unwrap();
                fav_vehicles.retain(|(_comp, _veh_no, rc)| *rc == max_ride_count);
            }

            let mut rng = thread_rng();
            for (rider, fav_vehicles) in rider_to_fav_vehicles.iter() {
                let fav_vehicles_count = fav_vehicles.len();
                if fav_vehicles_count == 0 {
                    continue;
                }
                let index = rng.gen_range(0..fav_vehicles_count);
                let (fav_comp, fav_vehicle, ride_count) = fav_vehicles.iter().nth(index).unwrap();
                fav_vehicle_strings.push(format!(
                    "{}: {} ({} {})",
                    rider,
                    if fav_comp == &config_guard.default_company {
                        format!("{}", fav_vehicle)
                    } else {
                        format!("{}/{}", fav_comp, fav_vehicle)
                    },
                    ride_count,
                    if *ride_count == 1 { "ride" } else { "rides" },
                ));
            }
        }

        let response = fav_vehicle_strings.join("\n");
        if response.len() == 0 {
            return;
        }
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_topbimdays(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };
        let rider_username_input = command.rest.trim();
        let rider_username_opt = if rider_username_input.len() == 0 {
            None
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => Some(ru),
                None => Some(rider_username_input.to_owned()),
            }
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let query_template = "
            WITH
            rides_dates(ride_date) AS (
                SELECT
                    -- count rides before 04:00 to previous day
                    CAST(
                        CASE WHEN EXTRACT(HOUR FROM r.\"timestamp\") < 4
                        THEN r.\"timestamp\" - CAST('P1D' AS interval)
                        ELSE r.\"timestamp\"
                        END
                    AS date)
                FROM
                    bim.rides r
                {USERNAME_CRITERION}
                {LOOKBACK_TIMESTAMP}
            ),
            ride_date_count(ride_year, ride_month, ride_day, ride_count) AS (
                SELECT
                    CAST(EXTRACT(YEAR FROM ride_date) AS bigint),
                    CAST(EXTRACT(MONTH FROM ride_date) AS bigint),
                    CAST(EXTRACT(DAY FROM ride_date) AS bigint),
                    COUNT(*)
                FROM rides_dates
                GROUP BY ride_date
            )
            SELECT ride_year, ride_month, ride_day, CAST(ride_count AS bigint) ride_count
            FROM ride_date_count
            ORDER BY ride_count DESC, ride_year DESC, ride_month DESC, ride_day DESC
            LIMIT 6
        ";
        let rows_res = if let Some(ru) = &rider_username_opt {
            let query = query_template.replace("{USERNAME_CRITERION}", "WHERE LOWER(r.rider_username) = LOWER($1)");
            Self::timestamp_query(
                &ride_conn,
                &query,
                "AND r.\"timestamp\" >= $2",
                "",
                lookback_range,
                &[&ru],
            ).await
        } else {
            let query = query_template.replace("{USERNAME_CRITERION}", "");
            Self::timestamp_query(
                &ride_conn,
                &query,
                "WHERE r.\"timestamp\" >= $1",
                "",
                lookback_range,
                &[],
            ).await
        };
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query days with most rides: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut date_and_ride_count: Vec<(NaiveDate, i64)> = Vec::new();
        for row in rows {
            let year: i64 = row.get(0);
            let month: i64 = row.get(1);
            let day: i64 = row.get(2);
            let ride_count: i64 = row.get(3);

            let date = NaiveDate::from_ymd_opt(
                year.try_into().expect("invalid year"),
                month.try_into().expect("invalid month"),
                day.try_into().expect("invalid day"),
            ).unwrap();

            date_and_ride_count.push((date, ride_count));
        }

        let mut top_text = if date_and_ride_count.len() >= 6 {
            date_and_ride_count.drain(5..);
            "Top 5 days:"
        } else {
            "Top days:"
        }.to_owned();

        for (date, ride_count) in &date_and_ride_count {
            top_text.push_str(&format!(
                "\n{} {}: {} rides",
                Self::weekday_abbr2(date.weekday()),
                date.format("%Y-%m-%d"),
                ride_count,
            ));
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &top_text,
        ).await;
    }

    async fn channel_command_topbimlines(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };
        let rider_username_input = command.rest.trim();
        let rider_username_opt = if rider_username_input.len() == 0 {
            None
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => Some(ru),
                None => Some(rider_username_input.to_owned()),
            }
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let query_template = "
            WITH ride_counts(company, line, ride_count) AS (
                SELECT r.company, r.line, COUNT(*)
                FROM bim.rides r
                WHERE r.line IS NOT NULL
                {USERNAME_CRITERION}
                {LOOKBACK_TIMESTAMP}
                GROUP BY r.company, r.line
            ),
            top_ride_counts(ride_count) AS (
                SELECT DISTINCT ride_count
                FROM ride_counts
                ORDER BY ride_count DESC
                LIMIT 6
            )
            SELECT rc.company, rc.line, CAST(rc.ride_count AS bigint)
            FROM ride_counts rc
            WHERE EXISTS (
                SELECT 1
                FROM top_ride_counts trc
                WHERE trc.ride_count = rc.ride_count
            )
        ";
        let rows_res = if let Some(ru) = &rider_username_opt {
            let query = query_template.replace("{USERNAME_CRITERION}", "AND LOWER(r.rider_username) = LOWER($1)");
            Self::timestamp_query(
                &ride_conn,
                &query,
                "AND r.\"timestamp\" >= $2",
                "",
                lookback_range,
                &[&ru],
            ).await
        } else {
            let query = query_template.replace("{USERNAME_CRITERION}", "");
            Self::timestamp_query(
                &ride_conn,
                &query,
                "AND r.\"timestamp\" >= $1",
                "",
                lookback_range,
                &[],
            ).await
        };
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query lines with most rides: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut count_to_lines: BTreeMap<i64, BTreeSet<(String, String)>> = BTreeMap::new();
        for row in rows {
            let company: String = row.get(0);
            let line: String = row.get(1);
            let ride_count: i64 = row.get(2);

            count_to_lines.entry(ride_count)
                .or_insert_with(|| BTreeSet::new())
                .insert((company, line));
        }
        let mut max_counts: Vec<i64> = count_to_lines.keys()
            .map(|c| *c)
            .collect();
        max_counts.reverse();

        let mut top_text = if max_counts.len() == 0 {
            "No top lines."
        } else if max_counts.len() >= 6 {
            max_counts.drain(5..);
            "Top 5 lines:"
        } else {
            "Top lines:"
        }.to_owned();

        for &count in &max_counts {
            write!(&mut top_text, "\n{} rides: ", count).unwrap();
            let mut first = true;
            for (company, line) in count_to_lines.get(&count).unwrap() {
                if first {
                    first = false;
                } else {
                    top_text.push_str(", ");
                }
                write!(&mut top_text, "{}/{}", company, line).unwrap();
            }
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &top_text,
        ).await;
    }

    async fn channel_command_bimridertypes(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };
        let sort_by_number =
            command.flags.contains("n")
            || command.flags.contains("sort-by-number")
        ;
        let rider_username_input = command.rest.trim();
        let rider_username = if rider_username_input.len() == 0 {
            channel_message.message.sender.username.clone()
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => ru,
                None => rider_username_input.to_owned(),
            }
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let rows_res = Self::timestamp_query(
            &ride_conn,
            "
                SELECT
                    r.company,
                    rv.vehicle_number,
                    CAST(COUNT(*) AS bigint) ride_count
                FROM bim.rides r
                INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
                WHERE
                    LOWER(r.rider_username) = LOWER($1)
                    AND rv.coupling_mode <> 'F'
                    {LOOKBACK_TIMESTAMP}
                GROUP BY
                    r.company,
                    rv.vehicle_number
            ",
            "AND r.\"timestamp\" >= $2",
            "",
            lookback_range,
            &[&rider_username],
        ).await;
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query bim rider types: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut company_to_bim_database_opt = HashMap::new();
        let mut type_to_count: BTreeMap<(String, String), i64> = BTreeMap::new();
        for row in rows {
            let company: String = row.get(0);
            let vehicle_number = VehicleNumber::from_string(row.get(1));
            let ride_count: i64 = row.get(2);

            let bim_database_opt = company_to_bim_database_opt
                .entry(company.clone())
                .or_insert_with(|| self.load_bim_database(&config_guard, &company));
            let vehicle_type_opt = bim_database_opt
                .as_ref()
                .map(|bd| bd
                    .get(&vehicle_number)
                    .map(|vi| vi.type_code.clone())
                )
                .flatten();
            let vehicle_type = match vehicle_type_opt {
                Some(vt) => vt,
                None => continue,
            };

            let type_ride_count = type_to_count
                .entry((company, vehicle_type))
                .or_insert(0);
            *type_ride_count += ride_count;
        }

        let mut type_and_count: Vec<(&str, &str, i64)> = type_to_count
            .iter()
            .map(|((comp, tp), count)| (comp.as_str(), tp.as_str(), *count))
            .collect();
        if sort_by_number {
            type_and_count.sort_unstable_by_key(|(comp, tp, count)| (-*count, *comp, *tp));
        }
        let types_counts: Vec<String> = type_and_count.iter()
            .map(|(comp, tp, count)|
                if *comp == config_guard.default_company.as_str() {
                    format!("{}{}: {}", tp, INVISIBLE_JOINER, count)
                } else {
                    format!("{}/{}{}: {}", comp, tp, INVISIBLE_JOINER, count)
                }
            )
            .collect();

        let response = if types_counts.len() == 0 {
            format!("{} has not ridden any known vehicle types!", rider_username)
        } else {
            let rider_lines_string = types_counts.join("\n");
            format!("{} has ridden these vehicle types:\n{}", rider_username, rider_lines_string)
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_bimriderlines(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let sort_by_number =
            command.flags.contains("n")
            || command.flags.contains("sort-by-number")
        ;
        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };
        let rider_username_input = command.rest.trim();
        let rider_username = if rider_username_input.len() == 0 {
            channel_message.message.sender.username.clone()
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => ru,
                None => rider_username_input.to_owned(),
            }
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let rows_res = Self::timestamp_query(
            &ride_conn,
            "
                SELECT
                    r.company,
                    r.line,
                    CAST(COUNT(*) AS bigint) ride_count
                FROM bim.rides r
                WHERE
                    LOWER(r.rider_username) = LOWER($1)
                    AND r.line IS NOT NULL
                    {LOOKBACK_TIMESTAMP}
                GROUP BY
                    r.company,
                    r.line
            ",
            "AND r.\"timestamp\" >= $2",
            "",
            lookback_range,
            &[&rider_username],
        ).await;
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query bim rider types: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut line_to_count: BTreeMap<(String, String), i64> = BTreeMap::new();
        for row in rows {
            let company: String = row.get(0);
            let line: String = row.get(1);
            let ride_count: i64 = row.get(2);

            let line_ride_count = line_to_count
                .entry((company, line))
                .or_insert(0);
            *line_ride_count += ride_count;
        }

        let mut line_and_count: Vec<(&str, &str, i64)> = line_to_count
            .iter()
            .map(|((comp, ln), count)| (comp.as_str(), ln.as_str(), *count))
            .collect();
        if sort_by_number {
            line_and_count.sort_unstable_by_key(|(comp, ln, count)| (-*count, *comp, *ln));
        }
        let lines_counts: Vec<String> = line_and_count.iter()
            .map(|(comp, tp, count)|
                if *comp == config_guard.default_company.as_str() {
                    format!("{}{}: {}", tp, INVISIBLE_JOINER, count)
                } else {
                    format!("{}/{}{}: {}", comp, tp, INVISIBLE_JOINER, count)
                }
            )
            .collect();
        let response = if lines_counts.len() == 0 {
            format!("{} has not ridden any known lines!", rider_username)
        } else {
            let rider_lines_string = lines_counts.join("\n");
            format!("{} has ridden these lines:\n{}", rider_username, rider_lines_string)
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_bimranges(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(config_guard.default_company.as_str());
        if company.len() == 0 {
            return;
        }

        let wants_precise =
            command.flags.contains("precise")
            || command.flags.contains("p")
        ;

        let database = match self.load_bim_database(&config_guard, company) {
            Some(db) => db,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "No vehicle database exists for this company.",
                ).await;
                return;
            },
        };

        let lines: Vec<String> = if wants_precise {
            let mut type_to_ranges: BTreeMap<String, RangeSet<u64>> = BTreeMap::new();
            for (veh_id, veh_info) in database.iter() {
                let veh_id_u64 = match veh_id.parse() {
                    Ok(vi) => vi,
                    Err(_) => continue,
                };
                type_to_ranges
                    .entry(veh_info.type_code.clone())
                    .or_insert_with(|| RangeSet::new())
                    .insert(veh_id_u64);
            }

            type_to_ranges.iter()
                .map(|(tp, ranges)| {
                    let range_strings: Vec<String> = ranges.ranges()
                        .map(|r|
                            if r.range.start == r.range.end - 1 {
                                // single number
                                format!("{}", r.range.start)
                            } else {
                                format!("{}-{}", r.range.start, r.range.end - 1)
                            }
                        )
                        .collect();
                    let ranges_string = range_strings.join(", ");
                    format!("{}{}: {}", tp, INVISIBLE_JOINER, ranges_string)
                })
                .collect()
        } else {
            let mut type_to_range: BTreeMap<String, (VehicleNumber, VehicleNumber)> = BTreeMap::new();
            for (veh_id, veh_info) in database.iter() {
                type_to_range
                    .entry(veh_info.type_code.clone())
                    .and_modify(|(old_low, old_high)| {
                        if &*old_low > veh_id {
                            *old_low = veh_id.clone();
                        }
                        if &*old_high < veh_id {
                            *old_high = veh_id.clone();
                        }
                    })
                    .or_insert((veh_id.clone(), veh_id.clone()));
            }

            type_to_range.iter()
                .map(|(tp, (low, high))| format!("{}{}: {}-{}", tp, INVISIBLE_JOINER, low, high))
                .collect()
        };
        let response = lines.join("\n");

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_bimtypes(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(config_guard.default_company.as_str());
        if company.len() == 0 {
            return;
        }
        let company_name = match config_guard.company_to_definition.get(company) {
            Some(cd) => cd.name.as_str(),
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Unknown company.",
                ).await;
                return;
            },
        };

        let rider_username_input = command.rest.trim();
        let rider_username_opt = if rider_username_input.len() == 0 {
            None
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => Some(ru),
                None => Some(rider_username_input.to_owned()),
            }
        };

        let database = match self.load_bim_database(&config_guard, company) {
            Some(db) => db,
            None => HashMap::new(), // work with an empty database
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let query_template = "
            SELECT DISTINCT
                rv.vehicle_number
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv
                ON rv.ride_id = r.id
            WHERE
                r.company = $1
                {AND_RIDER_USERNAME}
        ";
        let (mut response, rows_res) = if let Some(ru) = rider_username_opt {
            (
                format!("Statistics for vehicles of {} ridden by {}:", company_name, ru),
                ride_conn.query(
                    &query_template.replace("{AND_RIDER_USERNAME}", "AND LOWER(r.rider_username) = LOWER($2)"),
                    &[&company, &ru],
                ).await
            )
        } else {
            (
                format!("General statistics for vehicles of {}:", company_name),
                ride_conn.query(
                    &query_template.replace("{AND_RIDER_USERNAME}", ""),
                    &[&company],
                ).await
            )
        };
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query bim types: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };
        let mut ridden_vehicles: HashSet<VehicleNumber> = HashSet::new();
        for row in rows {
            let vehicle_number = VehicleNumber::from_string(row.get(0));
            ridden_vehicles.insert(vehicle_number);
        }

        // run through database
        let mut type_to_stats: BTreeMap<String, BimTypeStats> = BTreeMap::new();
        for vehicle in database.values() {
            let type_stats = type_to_stats
                .entry(vehicle.type_code.clone())
                .or_insert_with(|| BimTypeStats::new());
            type_stats.known_vehicles += 1;
            if vehicle.in_service_since.is_some() && vehicle.out_of_service_since.is_none() {
                type_stats.active_vehicles += 1;
            }
            if ridden_vehicles.remove(&vehicle.number) {
                type_stats.ridden_vehicles += 1;
            }
        }

        // collate information
        if type_to_stats.len() == 0 {
            write_expect!(&mut response, "\nNo vehicle database.");
        } else {
            for (tp, stats) in &type_to_stats {
                if stats.active_vehicles == 0 {
                    write_expect!(
                        &mut response,
                        "\n{}{}: {} vehicles, none active, {} ridden ({:.2}%)",
                        tp, INVISIBLE_JOINER, stats.known_vehicles,
                        stats.ridden_vehicles, stats.ridden_known() * 100.0,
                    );
                } else {
                    write_expect!(
                        &mut response,
                        "\n{}{}: {} vehicles, {} active ({:.2}%), {} ridden ({:.2}% of total, {:.2}% of active)",
                        tp, INVISIBLE_JOINER, stats.known_vehicles,
                        stats.active_vehicles, stats.active_known() * 100.0,
                        stats.ridden_vehicles, stats.ridden_known() * 100.0, stats.ridden_active() * 100.0,
                    );
                }
            }
        }
        // we have been emptying ridden_vehicles while collecting stats
        // what remains are the unknown types
        if ridden_vehicles.len() > 0 {
            write_expect!(&mut response, "\n{} vehicles of unknown type ridden", ridden_vehicles.len());
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_fixbimride(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        // forbid anything trailing the command options
        if command.rest.trim().len() > 0 {
            return;
        }

        let sender_username = channel_message.message.sender.username.as_str();

        let delete = command.flags.contains("d") || command.flags.contains("delete");
        let utc_time = command.flags.contains("u") || command.flags.contains("utc");
        let ride_id = command.options.get("i")
            .or_else(|| command.options.get("id"))
            .map(|cv| cv.as_i64().unwrap());
        let rider_username = command.options.get("r")
            .or_else(|| command.options.get("rider"))
            .map(|cv| cv.as_str().unwrap())
            .unwrap_or(sender_username);

        let new_rider = command.options.get("R")
            .or_else(|| command.options.get("set-rider"))
            .map(|cv| cv.as_str().unwrap());
        let new_company = command.options.get("c")
            .or_else(|| command.options.get("set-company"))
            .map(|cv| cv.as_str().unwrap());
        let new_line = command.options.get("l")
            .or_else(|| command.options.get("set-line"))
            .map(|cv| cv.as_str().unwrap());
        let new_timestamp_str = command.options.get("t")
            .or_else(|| command.options.get("set-timestamp"))
            .map(|cv| cv.as_str().unwrap());
        let new_vehicles_str = command.options.get("v")
            .or_else(|| command.options.get("vehicles"))
            .map(|cv| cv.as_str().unwrap());

        let is_admin = config_guard.admin_usernames.contains(sender_username);

        // verify arguments
        if !is_admin {
            if rider_username != sender_username {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Only `bim` admins can modify other riders' rides.",
                ).await;
                return;
            }

            if new_rider.is_some() {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Only `bim` admins can modify a ride's rider.",
                ).await;
                return;
            }

            if new_timestamp_str.is_some() {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Only `bim` admins can modify a ride's timestamp.",
                ).await;
                return;
            }
        }

        if delete && (new_rider.is_some() || new_company.is_some() || new_line.is_some() || new_timestamp_str.is_some() || new_vehicles_str.is_some()) {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Doesn't make much sense to delete a ride _and_ change its properties at the same time.",
            ).await;
            return;
        }

        if !delete && new_rider.is_none() && new_company.is_none() && new_line.is_none() && new_timestamp_str.is_none() && new_vehicles_str.is_none() {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Nothing to change.",
            ).await;
            return;
        }

        if let Some(nc) = new_company {
            if !config_guard.company_to_definition.contains_key(nc) {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "That company does not exist.",
                ).await;
                return;
            }

            // FIXME: verify vehicle and line numbers against company-specific regexes?
        }

        let new_timestamp_opt = if let Some(nts) = new_timestamp_str {
            let nt = self.parse_user_timestamp(
                nts,
                utc_time,
                &channel_message.channel.name,
            ).await;
            match nt {
                Some(t) => Some(t),
                None => return, // error message already output
            }
        } else {
            None
        };

        // find the ride
        let mut ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };
        let ride_txn = match ride_conn.transaction().await {
            Ok(txn) => txn,
            Err(e) => {
                error!("failed to open database transaction: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database transaction. :disappointed:",
                ).await;
                return;
            },
        };

        let ride_row_opt_res = if let Some(rid) = ride_id {
            ride_txn.query_opt(
                "
                    SELECT id, rider_username, \"timestamp\", company FROM bim.rides
                    WHERE id=$1
                ",
                &[&rid],
            ).await
        } else {
            ride_txn.query_opt(
                "
                    SELECT id, rider_username, \"timestamp\", company FROM bim.rides
                    WHERE rider_username=$1
                    ORDER BY \"timestamp\" DESC, id DESC
                    LIMIT 1
                ",
                &[&rider_username],
            ).await
        };
        let ride_row = match ride_row_opt_res {
            Err(e) => {
                error!("failed to obtain ride to modify: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to obtain ride to modify. :disappointed:",
                ).await;
                return;
            },
            Ok(None) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Ride not found. :disappointed:",
                ).await;
                return;
            },
            Ok(Some(r)) => r,
        };

        let ride_id: i64 = ride_row.get(0);
        let rider_username: String = ride_row.get(1);
        let ride_timestamp: DateTime<Local> = ride_row.get(2);
        let ride_company: String = ride_row.get(3);

        if !is_admin {
            let max_edit_dur = Duration::seconds(config_guard.max_edit_s);
            let now = Local::now();
            if now - ride_timestamp > max_edit_dur {
                if config_guard.max_edit_s > 0 {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!("Ride {} is too old to be edited. Ask a `bim` admin for help.", ride_id),
                    ).await;
                } else {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "You cannot edit your own rides. Ask a `bim` admin for help.",
                    ).await;
                }
                return;
            }

            if rider_username != sender_username {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Only `bim` admins can modify other riders' rides.",
                ).await;
                return;
            }
        }

        if delete {
            if let Err(e) = ride_txn.execute("DELETE FROM bim.rides WHERE id=$1", &[&ride_id]).await {
                error!("failed to delete ride {}: {}", ride_id, e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to delete ride {}.", ride_id),
                ).await;
                return;
            }

            if let Err(e) = ride_txn.commit().await {
                error!("failed to commit changes on ride {}: {}", ride_id, e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to commit ride {} changes.", ride_id),
                ).await;
                return;
            }

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("Ride {} deleted.", ride_id),
            ).await;
            return;
        }

        // update what there is to update
        let mut props: Vec<String> = Vec::new();
        let mut values: Vec<&(dyn ToSql + Sync)> = Vec::new();

        let (remember_new_rider, remember_new_company, remember_new_line, remember_new_timestamp);
        if let Some(nr) = new_rider {
            remember_new_rider = nr.to_owned();
            props.push(format!("rider_username = ${}", props.len() + 1));
            values.push(&remember_new_rider);
        }
        if let Some(nc) = new_company {
            remember_new_company = nc.to_owned();
            props.push(format!("company = ${}", props.len() + 1));
            values.push(&remember_new_company);
        }
        if let Some(nl) = new_line {
            remember_new_line = nl.to_owned();
            props.push(format!("line = ${}", props.len() + 1));
            values.push(&remember_new_line);
        }
        if let Some(nts) = new_timestamp_opt {
            remember_new_timestamp = nts.clone();
            props.push(format!("\"timestamp\" = ${}", props.len() + 1));
            values.push(&remember_new_timestamp);
        }

        if props.len() > 0 {
            let props_string = props.join(", ");
            let query = format!("UPDATE bim.rides SET {} WHERE id = ${}", props_string, props.len() + 1);
            values.push(&ride_id);

            if let Err(e) = ride_txn.execute(&query, &values).await {
                error!("failed to modify ride {}: {}", ride_id, e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to modify ride {}.", ride_id),
                ).await;
                return;
            }
        }

        if let Some(nvs) = new_vehicles_str {
            let this_company = new_company
                .unwrap_or(ride_company.as_str());
            let this_bim_db_opt = self.load_bim_database(&config_guard, this_company);
            let vehicles_res = spec_to_vehicles(
                nvs,
                this_bim_db_opt.as_ref(),
                config_guard.allow_fixed_coupling_combos,
            );
            let vehicles = match vehicles_res {
                Ok(vehicles) => vehicles,
                Err(e) => {
                    error!("failed to parse vehicles of ride {}: {}", ride_id, e);
                    let response = format!("Failed to parse vehicles of ride {}.", ride_id);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &response,
                    ).await;
                    return;
                },
            };
            if let Err(e) = replace_ride_vehicles(&ride_txn, ride_id, &vehicles).await {
                error!("failed to replace vehicles of ride {}: {}", ride_id, e);
                let response = format!("Failed to replace vehicles of ride {}.", ride_id);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &response,
                ).await;
                return;
            }
        }

        if let Err(e) = ride_txn.commit().await {
            error!("failed to commit transaction while modifying ride {}: {}", ride_id, e);
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("Failed to commit transaction while modifying ride {}.", ride_id),
            ).await;
            return;
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("Ride {} modified.", ride_id),
        ).await;

        // enqueue achievement recalculation
        if config_guard.achievements_active {
            let data = UpdateAchievementsData {
                channel: channel_message.channel.name.clone(),
                explicit: false,
            };
            let _ = self.achievement_update_sender.send(data);
        }
    }

    async fn channel_command_widestbims(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let lookback_range = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let ride_rows_res = Self::timestamp_query(
            &ride_conn,
            "
                WITH vehicle_and_distinct_rider_count(company, vehicle_number, rider_count) AS (
                    SELECT rav.company, rav.vehicle_number, COUNT(DISTINCT rav.rider_username)
                    FROM bim.rides_and_vehicles rav
                    WHERE rav.fixed_coupling_position = 0
                    {LOOKBACK_TIMESTAMP}
                    GROUP BY rav.company, rav.vehicle_number
                )
                SELECT vadrc.company, vadrc.vehicle_number, CAST(vadrc.rider_count AS bigint) rc
                FROM vehicle_and_distinct_rider_count vadrc
                WHERE NOT EXISTS ( -- ensure it's the maximum
                    SELECT 1
                    FROM vehicle_and_distinct_rider_count vadrc2
                    WHERE vadrc2.rider_count > vadrc.rider_count
                )
            ",
            "AND rav.\"timestamp\" >= $1",
            "",
            lookback_range,
            &[],
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(rr) => rr,
            Err(e) => {
                error!("failed to obtain widest-audience vehicles: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to obtain widest-audience vehicles. :disappointed:",
                ).await;
                return;
            },
        };

        let mut default_company_widest_bims: BTreeSet<VehicleNumber> = BTreeSet::new();
        let mut company_to_widest_bims: BTreeMap<String, BTreeSet<VehicleNumber>> = BTreeMap::new();

        let mut max_rider_count_opt = None;
        for ride_row in ride_rows {
            let company: String = ride_row.get(0);
            let vehicle_number = VehicleNumber::from_string(ride_row.get(1));
            let rider_count: i64 = ride_row.get(2);

            max_rider_count_opt = Some(rider_count);

            if company == config_guard.default_company {
                default_company_widest_bims.insert(vehicle_number);
            } else {
                company_to_widest_bims
                    .entry(company)
                    .or_insert_with(|| BTreeSet::new())
                    .insert(vehicle_number);
            }
        }

        let max_rider_count = match max_rider_count_opt {
            Some(mrc) => mrc,
            None => {
                // if this value has never been set, it means nobody has ever ridden any vehicle
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Nobody has ever ridden any vehicle. :disappointed:",
                ).await;
                return;
            },
        };

        let mut default_company_bim_strings: Vec<String> = default_company_widest_bims.iter()
            .map(|b| b.to_string())
            .collect();
        let mut other_company_bim_strings: Vec<String> = company_to_widest_bims.iter()
            .flat_map(|(comp, bims)|
                bims.iter()
                    .map(move |bim| format!("{}/{}", comp, bim))
            )
            .collect();
        let mut all_bim_strings = Vec::with_capacity(default_company_bim_strings.len() + other_company_bim_strings.len());
        all_bim_strings.append(&mut default_company_bim_strings);
        all_bim_strings.append(&mut other_company_bim_strings);

        let bim_string = all_bim_strings.join(", ");
        let rider_str = if max_rider_count == 1 {
            "rider"
        } else {
            "riders"
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!(
                "The following vehicles have been ridden by {} {}:\n{}",
                max_rider_count, rider_str, bim_string,
            ),
        ).await;
    }

    async fn channel_command_refreshbimach(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let sender_username = channel_message.message.sender.username.as_str();
        if !config_guard.admin_usernames.contains(sender_username) {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "You are not a `bim` admin. :disappointed:",
            ).await;
            return;
        }

        if !config_guard.achievements_active {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Achievements are not active. :disappointed:",
            ).await;
            return;
        }

        let data = UpdateAchievementsData {
            channel: channel_message.channel.name.clone(),
            explicit: true,
        };
        let _ = self.achievement_update_sender.send(data);
    }

    async fn channel_command_bimop(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;

        let region = command.options.get("region")
            .or_else(|| command.options.get("r"))
            .map(|v| v.as_str().expect("region value not a string"))
            .unwrap_or_else(|| config_guard.default_operator_region.as_str());

        let region_to_line_to_operator = match self.load_operator_databases(&config_guard) {
            Some(rlo) => rlo,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to load the operators database. :disappointed:",
                ).await;
                return;
            }
        };

        let line = command.rest.trim().to_lowercase();
        let operator_opt = region_to_line_to_operator
            .get(region)
            .map(|lto| lto.get(&line))
            .flatten();

        match operator_opt {
            Some(o) => {
                let message = if let Some(abbrev) = o.operator_abbrev.as_ref() {
                    format!(
                        "Line *{}* is operated by *{}* (`{}`).",
                        o.canonical_line, o.operator_name, abbrev,
                    )
                } else {
                    format!(
                        "Line *{}* is operated by *{}*.",
                        o.canonical_line, o.operator_name,
                    )
                };
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &message,
                ).await;
            },
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to find operator for line *{}*.", line),
                ).await;
            },
        }
    }

    async fn channel_command_lastbims(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let ride_rows_res = ride_conn.query(
            "
                SELECT innerquery.rider_username, CAST(COUNT(*) AS bigint) vehicle_count
                FROM (
                    SELECT DISTINCT rav1.rider_username, rav1.company, rav1.vehicle_number
                    FROM bim.rides_and_vehicles rav1
                    WHERE NOT EXISTS (
                        -- same vehicle, later timestamp
                        SELECT 1
                        FROM bim.rides_and_vehicles rav2
                        WHERE rav2.company = rav1.company
                        AND rav2.vehicle_number = rav1.vehicle_number
                        AND rav2.\"timestamp\" > rav1.\"timestamp\"
                    )
                ) innerquery
                GROUP BY innerquery.rider_username
                ORDER BY
                    vehicle_count DESC,
                    rider_username
            ",
            &[],
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(rr) => rr,
            Err(e) => {
                error!("failed to obtain last vehicles: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to obtain last vehicles. :disappointed:",
                ).await;
                return;
            },
        };
        if ride_rows.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Nobody has ever ridden any vehicle. :disappointed:",
            ).await;
            return;
        }
        let mut text = "Last rider in this number of vehicles:".to_owned();
        for ride_row in ride_rows {
            let rider_username: String = ride_row.get(0);
            let vehicle_count: i64 = ride_row.get(1);
            write_expect!(text, "\n{}: {}", rider_username, vehicle_count);
        }
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &text,
        ).await;
    }

    async fn channel_command_lonebims(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let ride_rows_res = ride_conn.query(
            "
                SELECT innerquery.rider_username, CAST(COUNT(*) AS bigint) vehicle_count
                FROM (
                    SELECT DISTINCT rav1.rider_username, rav1.company, rav1.vehicle_number
                    FROM bim.rides_and_vehicles rav1
                    WHERE NOT EXISTS (
                        -- same vehicle, different rider
                        SELECT 1
                        FROM bim.rides_and_vehicles rav2
                        WHERE rav2.company = rav1.company
                        AND rav2.vehicle_number = rav1.vehicle_number
                        AND rav2.rider_username <> rav1.rider_username
                    )
                ) innerquery
                GROUP BY innerquery.rider_username
                ORDER BY
                    vehicle_count DESC,
                    rider_username
            ",
            &[],
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(rr) => rr,
            Err(e) => {
                error!("failed to obtain lone vehicles: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to obtain lone vehicles. :disappointed:",
                ).await;
                return;
            },
        };
        if ride_rows.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Nobody has ever ridden any vehicle. :disappointed:",
            ).await;
            return;
        }
        let mut text = "Lone rider in this number of vehicles:".to_owned();
        for ride_row in ride_rows {
            let rider_username: String = ride_row.get(0);
            let vehicle_count: i64 = ride_row.get(1);
            write_expect!(text, "\n{}: {}", rider_username, vehicle_count);
        }
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &text,
        ).await;
    }

    async fn channel_command_recentbimrides(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let rider_username_input = command.rest.trim();
        let rider_username_opt = if rider_username_input.len() == 0 {
            None
        } else {
            match interface.resolve_username(rider_username_input).await {
                Some(ru) => Some(ru),
                None => Some(rider_username_input.to_owned()),
            }
        };

        let config_guard = self.config.read().await;

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let mut query_values: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(1);
        let query_addendum = if let Some(rider_username) = &rider_username_opt {
            query_values.push(rider_username);
            "AND rider_username = $1"
        } else {
            ""
        };
        let ride_rows_res = ride_conn.query(
            &format!(
                "
                    SELECT
                        \"timestamp\", rider_username, line, vehicle_number,
                        id, coupling_mode
                    FROM
                        bim.rides_and_vehicles
                    WHERE
                        \"timestamp\" >= CURRENT_TIMESTAMP - CAST('P1D' AS interval)
                        {}
                    ORDER BY
                        \"timestamp\", id, spec_position, fixed_coupling_position
                ",
                query_addendum,
            ),
            &query_values,
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(rr) => rr,
            Err(e) => {
                error!("failed to obtain recent rides: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to obtain recent rides. :disappointed:",
                ).await;
                return;
            },
        };
        if ride_rows.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "No recent rides. :disappointed:",
            ).await;
            return;
        }
        let mut id_to_ride: HashMap<i64, (DateTime<Local>, String, Option<String>, String)> = HashMap::with_capacity(ride_rows.len());
        for ride_row in ride_rows {
            let timestamp: DateTime<Local> = ride_row.get(0);
            let rider_username: String = ride_row.get(1);
            let line: Option<String> = ride_row.get(2);
            let vehicle_number: String = ride_row.get(3);
            let ride_id: i64 = ride_row.get(4);
            let coupling_mode: String = ride_row.get(5);

            match id_to_ride.entry(ride_id) {
                hash_map::Entry::Occupied(mut oe) => {
                    if coupling_mode == "R" {
                        let vehicle_numbers = &mut oe.get_mut().3;
                        if vehicle_numbers.len() > 0 {
                            vehicle_numbers.push('+');
                        }
                        vehicle_numbers.push_str(&vehicle_number);
                    }
                },
                hash_map::Entry::Vacant(ve) => {
                    let vehicles = if coupling_mode == "R" {
                        vehicle_number
                    } else {
                        String::new()
                    };
                    ve.insert((
                        timestamp,
                        rider_username,
                        line,
                        vehicles,
                    ));
                },
            }
        }

        // find field lengths for beautiful alignment
        let max_username_len = id_to_ride.values()
            .map(|tuple| tuple.1.chars().count())
            .max()
            .unwrap();
        let max_line_len = id_to_ride.values()
            .filter_map(|tuple| tuple.2.as_ref().map(|line| line.chars().count()))
            .max()
            .unwrap_or(0);
        let max_vehicles_len = id_to_ride.values()
            .map(|tuple| tuple.3.chars().count())
            .max()
            .unwrap();

        let mut rides_sorted: Vec<_> = id_to_ride.iter()
            .map(|(k, v)| (
                *k,
                &v.0,
                v.1.as_str(),
                v.2.as_ref().map(|r| r.as_str()),
                v.3.as_str(),
            ))
            .collect();
        // sort by timestamp, then by ID
        rides_sorted.sort_by_key(|tuple| (tuple.1, tuple.0));

        // assemble ride lines
        let mut ride_lines = String::from("```");
        for ride in rides_sorted.iter() {
            let (id, timestamp, rider, line, vehicles) = ride;
            write!(ride_lines, "\n{}: ", timestamp.format("%H:%M")).unwrap();

            write!(ride_lines, "{}, ", rider).unwrap();
            for _ in 0..(max_username_len - rider.chars().count()) {
                ride_lines.push(' ');
            }

            if let Some(ln) = line {
                write!(ride_lines, "{}, ", ln).unwrap();
                for _ in 0..(max_line_len - ln.chars().count()) {
                    ride_lines.push(' ');
                }
            } else {
                if max_line_len > 0 {
                    // skip this field, including trailing comma and space
                    ride_lines.push_str("  ");
                    for _ in 0..max_line_len {
                        ride_lines.push(' ');
                    }
                }
                // otherwise, there is no line, so don't bother
            }

            if vehicles.len() > 0 {
                write!(ride_lines, "{}, ", vehicles).unwrap();
                for _ in 0..(max_vehicles_len - vehicles.chars().count()) {
                    ride_lines.push(' ');
                }
            } else {
                if max_vehicles_len > 0 {
                    // skip this field, including trailing comma and space
                    ride_lines.push_str("  ");
                    for _ in 0..max_vehicles_len {
                        ride_lines.push(' ');
                    }
                }
            }

            write!(ride_lines, "ride {}", id).unwrap();
        }

        ride_lines.push_str("\n```");

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &ride_lines,
        ).await;
    }

    async fn channel_command_bimfreshen(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;
        let is_admin = config_guard.admin_usernames.contains(sender_username);
        if !is_admin {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Only `bim` admins can use this command.",
            ).await;
            return;
        }

        let mut ride_ids = Vec::new();
        for ride_id_str_raw in command.rest.split(",") {
            let ride_id_str = ride_id_str_raw.trim();
            let ride_id: i64 = match ride_id_str.parse() {
                Ok(ri) => ri,
                Err(_) => {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!("failed to decode ride ID `{}`", ride_id_str),
                    ).await;
                    return;
                },
            };
            ride_ids.push(ride_id);
        }

        let mut ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let ride_txn = match ride_conn.transaction().await {
            Ok(txn) => txn,
            Err(e) => {
                error!("failed to open database transaction: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database transaction. :disappointed:",
                ).await;
                return;
            },
        };

        let select_ride_res = ride_txn.prepare(
            "
                SELECT id, company, vehicle_number, coupling_mode FROM bim.rides_and_vehicles
                WHERE id = $1
                AND coupling_mode <> 'F' -- ignore fixed coupling; this will be taken from the vehicle database
                ORDER BY spec_position
            "
        ).await;
        let select_ride = match select_ride_res {
            Ok(sr) => sr,
            Err(e) => {
                error!("failed to prepare select-ride statement: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to prepare select-ride query. :disappointed:",
                ).await;
                return;
            }
        };

        let mut ride_to_company: BTreeMap<i64, String> = BTreeMap::new();
        let mut ride_to_vehicle_spec: BTreeMap<i64, String> = BTreeMap::new();
        for ride_id in ride_ids {
            // query this ride
            let ride_rows = match ride_txn.query(&select_ride, &[&ride_id]).await {
                Ok(rr) => rr,
                Err(e) => {
                    error!("failed to query ride {}: {}", ride_id, e);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "Failed to query a ride. :disappointed:",
                    ).await;
                    return;
                },
            };

            for ride_row in ride_rows {
                let id: i64 = ride_row.get(0);
                let company: String = ride_row.get(1);
                let vehicle_number_str: String = ride_row.get(2);
                let coupling_mode: String = ride_row.get(3);

                let vehicle_number: VehicleNumber = vehicle_number_str.into();

                assert!(coupling_mode == "R" || coupling_mode == "E");
                let explicitly_ridden = coupling_mode == "R";

                ride_to_company.insert(id, company);
                let vehicle_spec = ride_to_vehicle_spec
                    .entry(id)
                    .or_insert_with(|| String::new());
                if vehicle_spec.len() > 0 {
                    vehicle_spec.push('+');
                }
                vehicle_spec.push_str(vehicle_number.as_str());
                if explicitly_ridden {
                    vehicle_spec.push('!');
                }
            }
        }

        let mut company_to_bim_database: HashMap<String, Option<_>> = HashMap::new();

        // run through each ride
        for (ride_id, company) in &ride_to_company {
            let vehicle_spec = ride_to_vehicle_spec.get(ride_id)
                .expect("ride has company but no vehicle spec");

            debug!("freshening ride {} of company {:?} with vehicle spec {:?}", ride_id, company, vehicle_spec);

            if !company_to_bim_database.contains_key(company) {
                let bim_database = self.load_bim_database(&config_guard, company);
                company_to_bim_database.insert(company.clone(), bim_database);
            }
            let bim_database_opt = company_to_bim_database.get(company).unwrap();
            let vehicles_res = spec_to_vehicles(
                vehicle_spec,
                bim_database_opt.as_ref(),
                config_guard.allow_fixed_coupling_combos,
            );
            let vehicles = match vehicles_res {
                Ok(veh) => veh,
                Err(e) => {
                    error!("failed to reconstruct vehicles of ride {} from {:?}: {}", ride_id, vehicle_spec, e);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!("Failed to reconstruct vehicles of ride {}.", ride_id),
                    ).await;
                    return;
                },
            };
            if let Err(e) = replace_ride_vehicles(&ride_txn, *ride_id, &vehicles).await {
                error!("failed to replace vehicles of ride {}: {}", ride_id, e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Failed to replace vehicles of ride {}.", ride_id),
                ).await;
                return;
            }
        }

        if let Err(e) = ride_txn.commit().await {
            error!("failed to commit transaction: {}", e);
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("Failed to commit transaction."),
            ).await;
            return;
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("Rides refreshed."),
        ).await;

        // enqueue achievement recalculation
        if config_guard.achievements_active {
            let data = UpdateAchievementsData {
                channel: channel_message.channel.name.clone(),
                explicit: false,
            };
            let _ = self.achievement_update_sender.send(data);
        }
    }

    #[inline]
    fn weekday_abbr2(weekday: Weekday) -> &'static str {
        match weekday {
            Weekday::Mon => "Mo",
            Weekday::Tue => "Tu",
            Weekday::Wed => "We",
            Weekday::Thu => "Th",
            Weekday::Fri => "Fr",
            Weekday::Sat => "Sa",
            Weekday::Sun => "Su",
        }
    }

    fn canonical_date_format<Tz: TimeZone>(date_time: &DateTime<Tz>, on_at: bool, seconds: bool) -> String
            where Tz::Offset: fmt::Display {
        let dow = Self::weekday_abbr2(date_time.weekday());
        let date_formatted = date_time.format("%Y-%m-%d");
        let time_formatted = if seconds {
            date_time.format("%H:%M:%S")
        } else {
            date_time.format("%H:%M")
        };
        if on_at {
            format!("on {} {} at {}", dow, date_formatted, time_formatted)
        } else {
            format!("{} {} {}", dow, date_formatted, time_formatted)
        }
    }

    fn canonical_date_format_relative<Tz: TimeZone, Tz2: TimeZone>(date_time: &DateTime<Tz>, relative_to: &DateTime<Tz2>, on_at: bool, seconds: bool) -> String
            where Tz::Offset: fmt::Display, Tz2::Offset: fmt::Display {
        let night_owl_date = get_night_owl_date(date_time);
        let night_owl_relative = get_night_owl_date(relative_to);
        if night_owl_date == night_owl_relative {
            // only output time
            let time_formatted = if seconds {
                date_time.format("%H:%M:%S")
            } else {
                date_time.format("%H:%M")
            };
            if on_at {
                format!("at {}", time_formatted)
            } else {
                time_formatted.to_string()
            }
        } else {
            BimPlugin::canonical_date_format(date_time, on_at, seconds)
        }
    }

    fn english_adverbial_number(num: i64) -> String {
        match num {
            1 => "once".to_owned(),
            2 => "twice".to_owned(),
            // "thrice" and above are already too poetic
            other => format!("{} times", other),
        }
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let company_to_definition: HashMap<String, CompanyDefinition> = serde_json::from_value(config["company_to_definition"].clone())
            .or_msg("failed to decode company definitions")?;
        let default_company = config["default_company"]
            .as_str().ok_or("default_company not a string")?
            .to_owned();

        let manufacturer_mapping = if config["manufacturer_mapping"].is_null() {
            HashMap::new()
        } else {
            let mut mapping = HashMap::new();
            let mapping_entries = config["manufacturer_mapping"]
                .entries().ok_or("manufacturer_mapping not an object")?;
            for (k, v) in mapping_entries {
                let v_str = v.as_str()
                    .ok_or("manufacturer_mapping value not a string")?;
                mapping.insert(k.to_owned(), v_str.to_owned());
            }
            mapping
        };

        let ride_db_conn_string = config["ride_db_conn_string"]
            .as_str().ok_or("ride_db_conn_string is not a string")?
            .to_owned();

        let allow_fixed_coupling_combos = if config["allow_fixed_coupling_combos"].is_null() {
            false
        } else {
            config["allow_fixed_coupling_combos"]
                .as_bool().ok_or("allow_fixed_coupling_combos not a boolean")?
        };

        let admin_usernames = if config["admin_usernames"].is_null() {
            HashSet::new()
        } else {
            let admin_username_values = config["admin_usernames"]
                .as_array().ok_or("admin_usernames not a list")?;
            let mut admin_usernames = HashSet::new();
            for admin_username_value in admin_username_values {
                let username = admin_username_value
                    .as_str().ok_or("admin_usernames entry not a string")?
                    .to_owned();
                admin_usernames.insert(username);
            }
            admin_usernames
        };

        let max_edit_s = if config["max_edit_s"].is_null() {
            0
        } else {
            config["max_edit_s"]
                .as_i64().ok_or("max_edit_s not an i64")?
        };

        let achievements_active = if config["achievements_active"].is_null() {
            false
        } else {
            config["achievements_active"]
                .as_bool().ok_or("achievements_active not a bool")?
        };

        let operator_databases = if config["operator_databases"].is_null() {
            HashSet::new()
        } else {
            let operator_database_values = config["operator_databases"]
                .as_array().ok_or("operator_databases not a list")?;
            let mut operator_databases = HashSet::new();
            for operator_database_value in operator_database_values {
                let database = operator_database_value
                    .as_str().ok_or("operator_databases entry not a string")?
                    .to_owned();
                operator_databases.insert(database);
            }
            operator_databases
        };

        let default_operator_region = if config["default_operator_region"].is_null() {
            String::new()
        } else {
            config["default_operator_region"]
                .as_str().ok_or("default_operator_region not a string")?
                .to_owned()
        };

        Ok(Config {
            company_to_definition,
            default_company,
            manufacturer_mapping,
            ride_db_conn_string,
            allow_fixed_coupling_combos,
            admin_usernames,
            max_edit_s,
            achievements_active,
            operator_databases,
            default_operator_region,
        })
    }

    async fn parse_user_timestamp(&self, timestamp_str: &str, utc_time: bool, channel_name: &str) -> Option<DateTime<Local>> {
        let interface = match self.interface.upgrade() {
            Some(rbi) => rbi,
            None => return None,
        };

        let ndt = match NaiveDateTime::parse_from_str(timestamp_str, TIMESTAMP_INPUT_FORMAT) {
            Ok(ndt) => ndt,
            Err(_) => {
                send_channel_message!(
                    interface,
                    channel_name,
                    &format!("Failed to parse timestamp; expected format `{}`.", TIMESTAMP_INPUT_FORMAT),
                ).await;
                return None;
            },
        };
        if utc_time {
            Some(Local.from_utc_datetime(&ndt))
        } else {
            match Local.from_local_datetime(&ndt) {
                LocalResult::None => {
                    send_channel_message!(
                        interface,
                        channel_name,
                        "That local time does not exist. You may wish to use the -u/--utc option.",
                    ).await;
                    None
                },
                LocalResult::Ambiguous(_, _) => {
                    send_channel_message!(
                        interface,
                        channel_name,
                        "That local time is ambiguous. Please specify it in UTC using the -u/--utc option.",
                    ).await;
                    None
                },
                LocalResult::Single(t) => Some(t),
            }
        }
    }
}
#[async_trait]
impl RocketBotPlugin for BimPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load configuration");
        let config_lock = Arc::new(RwLock::new(
            "BimPlugin::config",
            config_object,
        ));

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bim",
                "bim",
                "{cpfx}bim NUMBER",
                "Obtains information about the public transport vehicle with the given number.",
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimride",
                "bim",
                "{cpfx}bimride VEHICLE[!][+VEHICLE[!]...][/LINE]",
                "Registers a ride with the given vehicle(s) on the given line.",
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .add_option("rider", CommandValueType::String)
                .add_option("r", CommandValueType::String)
                .add_option("timestamp", CommandValueType::String)
                .add_option("t", CommandValueType::String)
                .add_flag("u")
                .add_flag("utc")
                .add_flag("s")
                .add_flag("sandbox")
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topbims",
                "bim",
                "{cpfx}topbims",
                "Returns the most-ridden vehicle(s).",
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topriders",
                "bim",
                "{cpfx}topriders",
                "Returns the most active rider(s).",
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimcompanies",
                "bim",
                "{cpfx}bimcompanies",
                "Returns known public-transport operators.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "favbims",
                "bim",
                "{cpfx}favbims",
                "Returns each rider's most-ridden vehicle.",
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topbimdays",
                "bim",
                "{cpfx}topbimdays",
                "Returns the days with the most vehicle rides.",
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topbimlines",
                "bim",
                "{cpfx}topbimlines",
                "Returns the lines with the most vehicle rides.",
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimridertypes",
                "bim",
                "{cpfx}bimridertypes [{sopfx}n] USERNAME",
                "Returns the types of vehicles ridden by a rider.",
            )
                .add_flag("n")
                .add_flag("sort-by-number")
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimriderlines",
                "bim",
                "{cpfx}bimriderlines [{sopfx}n] USERNAME",
                "Returns the lines ridden by a rider.",
            )
                .add_flag("n")
                .add_flag("sort-by-number")
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimranges",
                "bim",
                "{cpfx}bimranges [{sopfx}p] [{sopfx}c COMPANY]",
                "Returns the number ranges of each vehicle type.",
            )
                .add_flag("precise")
                .add_flag("p")
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimtypes",
                "bim",
                "{cpfx}bimtypes [{sopfx}c COMPANY] [USERNAME]",
                "Returns statistics about vehicle types.",
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "fixbimride",
                "bim",
                "{cpfx}fixbimride OPTIONS",
                "Fix some data in a bim ride.",
            )
                .add_flag("d")
                .add_flag("delete")
                .add_flag("u")
                .add_flag("utc")
                .add_option("i", CommandValueType::Integer)
                .add_option("id", CommandValueType::Integer)
                .add_option("r", CommandValueType::String)
                .add_option("rider", CommandValueType::String)
                .add_option("R", CommandValueType::String)
                .add_option("set-rider", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .add_option("set-company", CommandValueType::String)
                .add_option("l", CommandValueType::String)
                .add_option("set-line", CommandValueType::String)
                .add_option("t", CommandValueType::String)
                .add_option("set-timestamp", CommandValueType::String)
                .add_option("v", CommandValueType::String)
                .add_option("vehicles", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "widestbims",
                "bim",
                "{cpfx}widestbims",
                "Lists vehicles that have served the widest selection of riders.",
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "refreshbimach",
                "bim",
                "{cpfx}refreshbimach",
                "Refreshes achievements and outputs newly unlocked ones.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimop",
                "bim",
                "{cpfx}bimop LINE",
                "Obtains the name of the company operating the given line.",
            )
                .add_option("r", CommandValueType::String)
                .add_option("region", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "lastbims",
                "bim",
                "{cpfx}lastbims",
                "How many vehicles has each rider been the last rider of?",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "lonebims",
                "bim",
                "{cpfx}lonebims",
                "How many vehicles has each rider been the only one to ride?",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "recentbimrides",
                "bim",
                "{cpfx}recentbimrides [USERNAME]",
                "A list of recent rides.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimfreshen",
                "bim",
                "{cpfx}bimfreshen RIDEID[,RIDEID...]",
                "Updates the given rides' vehicle information according to the current vehicle database.",
            )
                .build()
        ).await;

        // set up the achievement update loop
        let (achievement_update_sender, mut achievement_update_receiver) = mpsc::unbounded_channel();
        let achievement_update_interface = Weak::clone(&interface);
        let achievement_config_lock = Arc::downgrade(&config_lock);
        tokio::spawn(async move {
            loop {
                // wait for us to be told the details
                let data: UpdateAchievementsData = match achievement_update_receiver.recv().await {
                    Some(c) => c,
                    None => return, // sender has been dropped; no more channel names will ever reach us
                };

                // try to connect to the database
                let db_conn = {
                    // try to get a handle on the config lock
                    let config_lock = match Weak::upgrade(&achievement_config_lock) {
                        Some(cl) => cl,
                        None => return, // config lock is gone
                    };
                    let config_guard = config_lock
                        .read().await;
                    match connect_ride_db(&config_guard).await {
                        Ok(dbc) => dbc,
                        Err(e) => {
                            error!("failed to open postgres connection: {}", e);
                            // try again later
                            continue;
                        },
                    }
                };

                // try to get the interface to write to the channel
                let interface = match Weak::upgrade(&achievement_update_interface) {
                    Some(i) => i,
                    None => return, // that's not coming back either
                };

                // run the achievement update process
                do_update_achievements(&*interface, &db_conn, &data).await;
            }
        });

        Self {
            interface,
            config: config_lock,
            achievement_update_sender,
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "bim" {
            self.channel_command_bim(channel_message, command).await
        } else if command.name == "bimride" {
            self.channel_command_bimride(channel_message, command).await
        } else if command.name == "topbims" {
            self.channel_command_topbims(channel_message, command).await
        } else if command.name == "topriders" {
            self.channel_command_topriders(channel_message, command).await
        } else if command.name == "bimcompanies" {
            self.channel_command_bimcompanies(channel_message, command).await
        } else if command.name == "favbims" {
            self.channel_command_favbims(channel_message, command).await
        } else if command.name == "topbimdays" {
            self.channel_command_topbimdays(channel_message, command).await
        } else if command.name == "topbimlines" {
            self.channel_command_topbimlines(channel_message, command).await
        } else if command.name == "bimridertypes" {
            self.channel_command_bimridertypes(channel_message, command).await
        } else if command.name == "bimriderlines" {
            self.channel_command_bimriderlines(channel_message, command).await
        } else if command.name == "bimranges" {
            self.channel_command_bimranges(channel_message, command).await
        } else if command.name == "bimtypes" {
            self.channel_command_bimtypes(channel_message, command).await
        } else if command.name == "fixbimride" {
            self.channel_command_fixbimride(channel_message, command).await
        } else if command.name == "widestbims" {
            self.channel_command_widestbims(channel_message, command).await
        } else if command.name == "refreshbimach" {
            self.channel_command_refreshbimach(channel_message, command).await
        } else if command.name == "bimop" {
            self.channel_command_bimop(channel_message, command).await
        } else if command.name == "lastbims" {
            self.channel_command_lastbims(channel_message, command).await
        } else if command.name == "lonebims" {
            self.channel_command_lonebims(channel_message, command).await
        } else if command.name == "recentbimrides" {
            self.channel_command_recentbimrides(channel_message, command).await
        } else if command.name == "bimfreshen" {
            self.channel_command_bimfreshen(channel_message, command).await
        }
    }

    async fn plugin_name(&self) -> String {
        "bim".to_owned()
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "bim" {
            Some(include_str!("../help/bim.md").to_owned())
        } else if command_name == "bimride" {
            Some(include_str!("../help/bimride.md").to_owned())
        } else if command_name == "topbims" {
            Some(include_str!("../help/topbims.md").to_owned())
        } else if command_name == "topriders" {
            Some(include_str!("../help/topriders.md").to_owned())
        } else if command_name == "bimcompanies" {
            Some(include_str!("../help/bimcompanies.md").to_owned())
        } else if command_name == "favbims" {
            Some(include_str!("../help/favbims.md").to_owned())
        } else if command_name == "topbimdays" {
            Some(include_str!("../help/topbimdays.md").to_owned())
        } else if command_name == "topbimlines" {
            Some(include_str!("../help/topbimlines.md").to_owned())
        } else if command_name == "bimridertypes" {
            Some(include_str!("../help/bimridertypes.md").to_owned())
        } else if command_name == "bimriderlines" {
            Some(include_str!("../help/bimriderlines.md").to_owned())
        } else if command_name == "bimranges" {
            Some(include_str!("../help/bimranges.md").to_owned())
        } else if command_name == "bimtypes" {
            Some(include_str!("../help/bimtypes.md").to_owned())
        } else if command_name == "fixbimride" {
            Some(include_str!("../help/fixbimride.md").to_owned())
        } else if command_name == "widestbims" {
            Some(include_str!("../help/widestbims.md").to_owned())
        } else if command_name == "refreshbimach" {
            Some(include_str!("../help/refreshbimach.md").to_owned())
        } else if command_name == "bimop" {
            Some(include_str!("../help/bimop.md").to_owned())
        } else if command_name == "lastbims" {
            Some(include_str!("../help/lastbims.md").to_owned())
        } else if command_name == "lonebims" {
            Some(include_str!("../help/lonebims.md").to_owned())
        } else if command_name == "recentbimrides" {
            Some(include_str!("../help/recentbimrides.md").to_owned())
        } else if command_name == "bimfreshen" {
            Some(include_str!("../help/bimfreshen.md").to_owned())
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


async fn connect_ride_db(config: &Config) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
    let (client, connection) = match tokio_postgres::connect(&config.ride_db_conn_string, NoTls).await {
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


async fn do_update_achievements(
    interface: &dyn RocketBotInterface,
    ride_conn: &tokio_postgres::Client,
    data: &UpdateAchievementsData,
) {
    let prev_users_achievements = get_all_achievements(
        &ride_conn,
    ).await
        .unwrap_or_else(|e| {
            error!("failed to obtain previous achievements: {}", e);
            BTreeMap::new()
        });

    if let Err(e) = recalculate_achievements(&ride_conn).await {
        error!("failed to recalculate achievements: {}", e);
        send_channel_message!(
            interface,
            &data.channel,
            "Failed to recalculate achievements. :disappointed:",
        ).await;
        return;
    }

    let new_users_achievements = get_all_achievements(
        &ride_conn,
    ).await
        .unwrap_or_else(|e| {
            error!("failed to obtain current achievements: {}", e);
            BTreeMap::new()
        });

    let empty_achievements = BTreeMap::new();
    for (user, new_achievements) in new_users_achievements {
        let prev_achievements = prev_users_achievements.get(&user)
            .unwrap_or_else(|| &empty_achievements);

        for (ach_id, new_ach_state) in new_achievements {
            let new_timestamp = match new_ach_state.timestamp {
                Some(ts) => ts.0,
                None => continue, // locked achievements aren't interesting
            };

            let had_timestamp_previously = prev_achievements
                .get(&ach_id)
                .map(|st8| st8.timestamp.is_some())
                .unwrap_or(false);
            if !had_timestamp_previously {
                // newly unlocked achievement!
                let ach_def_opt = ACHIEVEMENT_DEFINITIONS
                    .iter()
                    .filter(|ad| ad.id == ach_id)
                    .nth(0);
                if let Some(ach_def) = ach_def_opt {
                    info!(
                        "achievement unlocked! rider {:?} unlocked achievement {} at {}",
                        user,
                        ach_id,
                        new_timestamp,
                    );

                    // find how many users had this achievement previously
                    let previous_count = prev_users_achievements
                        .values()
                        .filter_map(|user_achievements|
                            user_achievements.get(&ach_id)
                        )
                        .filter(|ach_state| ach_state.timestamp.is_some())
                        .count();

                    let mut message = format!(
                        "{} unlocked *{}* ({}){} {}",
                        user,
                        ach_def.name,
                        ach_def.description,
                        if data.explicit { ", fulfilling the criteria" } else { "" },
                        BimPlugin::canonical_date_format(&new_timestamp, true, false),
                    );
                    match previous_count {
                        0 => if data.explicit {
                            write!(message, ", among the first to do so!")
                        } else {
                            write!(message, ", the first to do so!")
                        },
                        1 => write!(message, ", the second to do so!"),
                        more => write!(message, ", joining the ranks of {} riders before them!", more),
                    }.unwrap();

                    send_channel_message!(
                        interface,
                        &data.channel,
                        &message,
                    ).await;
                }
            }
        }
    }
}


const INSERT_VEHICLE_STMT_STR: &str = "
    INSERT INTO bim.ride_vehicles
        (ride_id, vehicle_number, vehicle_type, spec_position, coupling_mode, fixed_coupling_position)
    VALUES
        ($1, $2, $3, $4, $5, $6)
";


pub async fn add_ride(
    ride_conn: &tokio_postgres::Transaction<'_>,
    company: &str,
    vehicles: &[NewVehicleEntry],
    rider_username: &str,
    timestamp: DateTime<Local>,
    line: Option<&str>,
    sandbox: bool,
) -> Result<(i64, Vec<RideTableVehicle>), tokio_postgres::Error> {
    let prev_my_same_count_stmt = ride_conn.prepare(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username = $3
                AND rv.coupling_mode = 'R'
        ",
    ).await?;
    let prev_my_same_row_stmt = ride_conn.prepare(
        "
            SELECT r.\"timestamp\", r.line
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username = $3
                AND rv.coupling_mode = 'R'
            ORDER BY r.\"timestamp\" DESC
            LIMIT 1
        ",
    ).await?;
    let prev_my_coupled_count_stmt = ride_conn.prepare(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username = $3
                AND rv.coupling_mode <> 'R'
        ",
    ).await?;
    let prev_my_coupled_row_stmt = ride_conn.prepare(
        "
            SELECT r.\"timestamp\", r.line
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username = $3
                AND rv.coupling_mode <> 'R'
            ORDER BY r.\"timestamp\" DESC
            LIMIT 1
        ",
    ).await?;
    let prev_other_same_count_stmt = ride_conn.prepare(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username <> $3
                AND rv.coupling_mode = 'R'
        ",
    ).await?;
    let prev_other_same_row_stmt = ride_conn.prepare(
        "
            SELECT r.\"timestamp\", r.line, r.rider_username
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username <> $3
                AND rv.coupling_mode = 'R'
            ORDER BY r.\"timestamp\" DESC
            LIMIT 1
        ",
    ).await?;
    let prev_other_coupled_count_stmt = ride_conn.prepare(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username <> $3
                AND rv.coupling_mode <> 'R'
        ",
    ).await?;
    let prev_other_coupled_row_stmt = ride_conn.prepare(
        "
            SELECT r.\"timestamp\", r.line, r.rider_username
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username <> $3
                AND rv.coupling_mode <> 'R'
            ORDER BY r.\"timestamp\" DESC
            LIMIT 1
        ",
    ).await?;

    let mut vehicle_data = Vec::new();
    for vehicle in vehicles {
        let prev_my_same_count: i64 = {
            let prev_my_same_count_row = ride_conn.query_one(
                &prev_my_same_count_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            prev_my_same_count_row.get(0)
        };
        let (prev_my_same_timestamp, prev_my_same_line): (Option<DateTime<Local>>, Option<String>) = {
            let prev_my_same_row_opt = ride_conn.query_opt(
                &prev_my_same_row_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            let prev_my_same_timestamp = prev_my_same_row_opt.as_ref().map(|r| r.get(0));
            let prev_my_same_line = prev_my_same_row_opt.as_ref().map(|r| r.get(1)).flatten();
            (prev_my_same_timestamp, prev_my_same_line)
        };

        let prev_my_coupled_count: i64 = {
            let prev_my_coupled_count_row = ride_conn.query_one(
                &prev_my_coupled_count_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            prev_my_coupled_count_row.get(0)
        };
        let (prev_my_coupled_timestamp, prev_my_coupled_line): (Option<DateTime<Local>>, Option<String>) = {
            let prev_my_coupled_row_opt = ride_conn.query_opt(
                &prev_my_coupled_row_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            let prev_my_coupled_timestamp = prev_my_coupled_row_opt.as_ref().map(|r| r.get(0));
            let prev_my_coupled_line = prev_my_coupled_row_opt.as_ref().map(|r| r.get(1)).flatten();
            (prev_my_coupled_timestamp, prev_my_coupled_line)
        };

        let prev_other_same_count: i64 = {
            let prev_other_same_count_row = ride_conn.query_one(
                &prev_other_same_count_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            prev_other_same_count_row.get(0)
        };
        let (prev_other_same_timestamp, prev_other_same_line, prev_other_same_rider): (Option<DateTime<Local>>, Option<String>, Option<String>) = {
            let prev_other_same_row_opt = ride_conn.query_opt(
                &prev_other_same_row_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            let prev_other_same_timestamp = prev_other_same_row_opt.as_ref().map(|r| r.get(0));
            let prev_other_same_line = prev_other_same_row_opt.as_ref().map(|r| r.get(1)).flatten();
            let prev_other_same_rider = prev_other_same_row_opt.as_ref().map(|r| r.get(2));
            (prev_other_same_timestamp, prev_other_same_line, prev_other_same_rider)
        };

        let prev_other_coupled_count: i64 = {
            let prev_other_coupled_count_row = ride_conn.query_one(
                &prev_other_coupled_count_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            prev_other_coupled_count_row.get(0)
        };
        let (prev_other_coupled_timestamp, prev_other_coupled_line, prev_other_coupled_rider): (Option<DateTime<Local>>, Option<String>, Option<String>) = {
            let prev_other_coupled_row_opt = ride_conn.query_opt(
                &prev_other_coupled_row_stmt,
                &[&company, &vehicle.number.as_str(), &rider_username],
            ).await?;
            let prev_other_coupled_timestamp = prev_other_coupled_row_opt.as_ref().map(|r| r.get(0));
            let prev_other_coupled_line = prev_other_coupled_row_opt.as_ref().map(|r| r.get(1)).flatten();
            let prev_other_coupled_rider = prev_other_coupled_row_opt.as_ref().map(|r| r.get(2));
            (prev_other_coupled_timestamp, prev_other_coupled_line, prev_other_coupled_rider)
        };

        if vehicle.coupling_mode != CouplingMode::FixedCoupling {
            vehicle_data.push(RideTableVehicle {
                vehicle_number: vehicle.number.clone().into_string(),
                vehicle_type: vehicle.type_code.clone(),
                my_same_count: prev_my_same_count,
                my_same_last: prev_my_same_timestamp.map(|timestamp| Ride {
                    timestamp,
                    line: prev_my_same_line,
                }),
                my_coupled_count: prev_my_coupled_count,
                my_coupled_last: prev_my_coupled_timestamp.map(|timestamp| Ride {
                    timestamp,
                    line: prev_my_coupled_line,
                }),
                other_same_count: prev_other_same_count,
                other_same_last: prev_other_same_timestamp.map(|timestamp| UserRide {
                    rider_username: prev_other_same_rider.unwrap(),
                    ride: Ride {
                        timestamp,
                        line: prev_other_same_line,
                    },
                }),
                other_coupled_count: prev_other_coupled_count,
                other_coupled_last: prev_other_coupled_timestamp.map(|timestamp| UserRide {
                    rider_username: prev_other_coupled_rider.unwrap(),
                    ride: Ride {
                        timestamp,
                        line: prev_other_coupled_line,
                    },
                }),
            });
        }
    }

    let ride_id: i64 = if sandbox {
        -1
    } else {
        let id_row = ride_conn.query_one(
            "
                INSERT INTO bim.rides
                    (id, company, rider_username, \"timestamp\", line)
                VALUES
                    (DEFAULT, $1, $2, $3, $4)
                RETURNING id
            ",
            &[&company, &rider_username, &timestamp, &line],
        ).await?;
        let ride_id: i64 = id_row.get(0);

        let insert_vehicle_stmt = ride_conn.prepare(INSERT_VEHICLE_STMT_STR).await?;
        for vehicle in vehicles {
            ride_conn.execute(
                &insert_vehicle_stmt,
                &[
                    &ride_id,
                    &vehicle.number.as_str(),
                    &vehicle.type_code,
                    &vehicle.spec_position,
                    &vehicle.coupling_mode.as_db_str(),
                    &vehicle.fixed_coupling_position,
                ],
            ).await?;
        }

        ride_id
    };

    Ok((ride_id, vehicle_data))
}


async fn replace_ride_vehicles(
    ride_conn: &tokio_postgres::Transaction<'_>,
    ride_id: i64,
    vehicles: &[NewVehicleEntry],
) -> Result<(), tokio_postgres::Error> {
    let remove_vehicles_stmt = ride_conn.prepare(
        "DELETE FROM bim.ride_vehicles WHERE ride_id = $1",
    ).await?;
    let insert_vehicle_stmt = ride_conn.prepare(INSERT_VEHICLE_STMT_STR).await?;

    ride_conn.execute(
        &remove_vehicles_stmt,
        &[&ride_id],
    ).await?;

    for vehicle in vehicles {
        ride_conn.execute(
            &insert_vehicle_stmt,
            &[
                &ride_id,
                &vehicle.number.as_str(),
                &vehicle.type_code,
                &vehicle.spec_position,
                &vehicle.coupling_mode.as_db_str(),
                &vehicle.fixed_coupling_position,
            ],
        ).await?;
    }

    Ok(())
}

#[derive(Debug)]
pub enum IncrementBySpecError {
    SpecParseFailure(String),
    VehicleNumberParseFailure(String, ParseIntError),
    FixedCouplingCombo(VehicleNumber),
    DatabaseQuery(String, Vec<NewVehicleEntry>, Option<String>, tokio_postgres::Error),
    DatabaseBeginTransaction(tokio_postgres::Error),
    DatabaseCommitTransaction(tokio_postgres::Error),
}
impl fmt::Display for IncrementBySpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpecParseFailure(spec) => write!(f, "failed to parse spec {:?}", spec),
            Self::VehicleNumberParseFailure(num_str, e) => write!(f, "failed to parse vehicle number {:?}: {}", num_str, e),
            Self::FixedCouplingCombo(coupled_number) => write!(f, "vehicle number {} is part of a fixed coupling and cannot be ridden in combination with other vehicles", coupled_number),
            Self::DatabaseQuery(rider, vehicle_nums, line_opt, e) => write!(f, "database query error registering {} riding on vehicles {:?} on line {:?}: {}", rider, vehicle_nums, line_opt, e),
            Self::DatabaseBeginTransaction(e) => write!(f, "database error beginning transaction: {}", e),
            Self::DatabaseCommitTransaction(e) => write!(f, "database error committing transaction: {}", e),
        }
    }
}
impl std::error::Error for IncrementBySpecError {
}

fn spec_to_vehicles(
    vehicles_str: &str,
    bim_database_opt: Option<&HashMap<VehicleNumber, VehicleInfo>>,
    allow_fixed_coupling_combos: bool,
) -> Result<Vec<NewVehicleEntry>, IncrementBySpecError> {
    let vehicle_num_strs: Vec<&str> = vehicles_str.split("+").collect();
    let mut vehicle_nums_and_modes = Vec::new();
    for &vehicle_num in &vehicle_num_strs {
        let (no_exclamation_vehicle_num, coupling_mode) = if let Some(nevn) = vehicle_num.strip_suffix("!") {
            (nevn, CouplingMode::Ridden)
        } else {
            let coupling_mode = if vehicle_num_strs.len() == 1 {
                CouplingMode::Ridden
            } else {
                CouplingMode::Explicit
            };
            (vehicle_num, coupling_mode)
        };
        let vehicle_num_owned = VehicleNumber::from_string(no_exclamation_vehicle_num.to_owned());
        if !allow_fixed_coupling_combos {
            if let Some(bim_database) = bim_database_opt {
                if let Some(veh) = bim_database.get(&vehicle_num_owned) {
                    if veh.fixed_coupling.len() > 0 && vehicle_num_strs.len() > 1 {
                        // this vehicle is in a fixed coupling but we have more than one vehicle
                        // this is forbidden
                        return Err(IncrementBySpecError::FixedCouplingCombo(vehicle_num_owned));
                    }
                }
            }
        }
        vehicle_nums_and_modes.push((no_exclamation_vehicle_num, coupling_mode));
    }

    // also count vehicles ridden in a fixed coupling with the given vehicle
    let mut all_vehicles: Vec<NewVehicleEntry> = Vec::new();
    let explicit_vehicle_num_to_mode: HashMap<VehicleNumber, CouplingMode> = vehicle_nums_and_modes.iter()
        .map(|(vn, cm)| ((*vn).to_owned().into(), *cm))
        .collect();
    let mut seen_vehicles: HashSet<VehicleNumber> = HashSet::new();
    for (spec_pos, (vehicle_num, _coupling_mode)) in vehicle_nums_and_modes.iter().enumerate() {
        let mut added_fixed_coupling = false;
        let mut type_code = None;
        if let Some(bim_database) = bim_database_opt {
            let vehicle_num_owned = VehicleNumber::from_string((*vehicle_num).to_owned());
            if let Some(veh) = bim_database.get(&vehicle_num_owned) {
                type_code = Some(veh.type_code.clone());

                for (fc_pos, fc) in veh.fixed_coupling.iter().enumerate() {
                    if !seen_vehicles.insert(fc.clone()) {
                        // we've seen this vehicle before
                        continue;
                    }

                    let fc_type_code = bim_database.get(fc)
                        .map(|veh| veh.type_code.clone());
                    let coupling_mode = explicit_vehicle_num_to_mode.get(fc)
                        .map(|cm| *cm)
                        .unwrap_or(CouplingMode::FixedCoupling);
                    let vehicle = NewVehicleEntry {
                        number: fc.clone(),
                        type_code: fc_type_code,
                        spec_position: spec_pos.try_into().unwrap(),
                        coupling_mode,
                        fixed_coupling_position: fc_pos.try_into().unwrap(),
                    };
                    all_vehicles.push(vehicle);
                    added_fixed_coupling = true;
                }
            }
        }

        if !added_fixed_coupling {
            if !seen_vehicles.insert((*vehicle_num).to_owned().into()) {
                // we've seen this vehicle before
                continue;
            }

            let vehicle_num_owned = (*vehicle_num).to_owned().into();
            let coupling_mode = explicit_vehicle_num_to_mode.get(&vehicle_num_owned)
                .map(|cm| *cm)
                .unwrap_or(CouplingMode::FixedCoupling);
            let vehicle = NewVehicleEntry {
                number: vehicle_num_owned,
                type_code,
                spec_position: spec_pos.try_into().unwrap(),
                coupling_mode,
                fixed_coupling_position: 0,
            };
            all_vehicles.push(vehicle);
        }
    }

    Ok(all_vehicles)
}

pub async fn increment_rides_by_spec(
    ride_conn: &mut tokio_postgres::Client,
    bim_database_opt: Option<&HashMap<VehicleNumber, VehicleInfo>>,
    company: &str,
    company_def: &CompanyDefinition,
    rider_username: &str,
    timestamp: DateTime<Local>,
    rides_spec: &str,
    allow_fixed_coupling_combos: bool,
    sandbox: bool,
) -> Result<RideTableData, IncrementBySpecError> {
    let spec_no_spaces = rides_spec.replace(" ", "");

    let vehicle_and_line_regex = company_def.vehicle_and_line_regex();
    let mut vehicle_cap_names = Vec::new();
    let mut line_cap_names = Vec::new();
    for cap_name in vehicle_and_line_regex.capture_names() {
        if let Some(cn) = cap_name {
            if cn.starts_with("vehicles") {
                vehicle_cap_names.push(cn);
            } else if cn.starts_with("line") {
                line_cap_names.push(cn);
            }
        }
    }

    let caps = match vehicle_and_line_regex.captures(&spec_no_spaces) {
        Some(c) => c,
        None => return Err(IncrementBySpecError::SpecParseFailure(spec_no_spaces)),
    };

    let vehicles_str_opt = vehicle_cap_names
        .iter()
        .filter_map(|cn| caps.name(cn))
        .map(|cap| cap.as_str())
        .nth(0);
    let vehicles_str = vehicles_str_opt.expect("failed to capture vehicles");

    let line_str_opt = line_cap_names
        .iter()
        .filter_map(|cn| caps.name(cn))
        .map(|cap| cap.as_str())
        .nth(0);

    let all_vehicles = spec_to_vehicles(
        vehicles_str,
        bim_database_opt,
        allow_fixed_coupling_combos,
    )?;

    let (ride_id, vehicles) = {
        let xact = ride_conn.transaction().await
            .map_err(|e| IncrementBySpecError::DatabaseBeginTransaction(e))?;

        let (rid, vehicles) = add_ride(
            &xact,
            company,
            &all_vehicles,
            rider_username,
            timestamp,
            line_str_opt,
            sandbox,
        )
            .await.map_err(|e|
                IncrementBySpecError::DatabaseQuery(rider_username.to_owned(), all_vehicles.clone(), line_str_opt.map(|l| l.to_owned()), e)
            )?;

        xact.commit().await
            .map_err(|e| IncrementBySpecError::DatabaseCommitTransaction(e))?;

        (rid, vehicles)
    };

    Ok(RideTableData {
        ride_id,
        line: line_str_opt.map(|l| l.to_owned()),
        rider_username: rider_username.to_owned(),
        vehicles,
        relative_time: Some(timestamp),
    })
}


/// Returns the Night Owl Time date for the given date.
///
/// With Night Owl Time, hours 0, 1, 2 and 3 are counted towards the previous day.
fn get_night_owl_date<D: Datelike + Timelike>(date_time: &D) -> NaiveDate {
    let naive_date = NaiveDate::from_ymd_opt(date_time.year(), date_time.month(), date_time.day())
        .unwrap();
    if date_time.hour() < 4 {
        naive_date.pred_opt().unwrap()
    } else {
        naive_date
    }
}
