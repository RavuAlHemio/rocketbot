mod range_set;


use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::TryInto;
use std::fmt::{self, Write};
use std::fs::File;
use std::num::ParseIntError;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, DateTime, Duration, Local, NaiveDate, TimeZone, Weekday};
use indexmap::IndexSet;
use log::error;
use once_cell::sync::OnceCell;
use rand::{Rng, thread_rng};
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, phrase_join, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::serde::serde_opt_regex;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_postgres::NoTls;
use tokio_postgres::types::ToSql;

use crate::range_set::RangeSet;


const INVISIBLE_JOINER: &str = "\u{2060}"; // WORD JOINER


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


pub type VehicleNumber = u32;


macro_rules! write_expect {
    ($dst:expr, $($arg:tt)*) => {
        write!($dst, $($arg)*).expect("write failed")
    };
}


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehicleInfo {
    pub number: VehicleNumber,
    pub type_code: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub other_data: BTreeMap<String, String>,
    pub fixed_coupling: IndexSet<VehicleNumber>,
}
impl VehicleInfo {
    pub fn new(number: VehicleNumber, type_code: String) -> Self {
        Self {
            number,
            type_code,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: BTreeMap::new(),
            fixed_coupling: IndexSet::new(),
        }
    }
}


#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct LastRideInfo {
    pub ride_count: usize,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct OtherRiderInfo {
    pub rider_username: String,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct VehicleRideInfo {
    pub vehicle_number: VehicleNumber,
    pub ridden_within_fixed_coupling: bool,
    pub last_ride: Option<LastRideInfo>,
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
    pub as_part_of_fixed_coupling: bool,
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

        let valr_string = format!(
            concat!(
                "^",
                "(?P<vehicles>",
                    "(?:{})",
                    "(?:",
                        "[+]",
                        "(?:{})",
                    ")*",
                ")",
                "(?:",
                    "[/]",
                    "(?P<line>",
                        "(?:{})",
                    ")",
                ")?",
                "$"
            ),
            vehicle_number_rstr, vehicle_number_rstr, line_number_rstr,
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
            bim_database_path: None,
            vehicle_number_regex: None,
            line_number_regex: None,
            vehicle_and_line_regex: OnceCell::new(),
        }
    }
}


pub struct BimPlugin {
    interface: Weak<dyn RocketBotInterface>,
    company_to_definition: HashMap<String, CompanyDefinition>,
    default_company: String,
    manufacturer_mapping: HashMap<String, String>,
    ride_db_conn_string: String,
    allow_fixed_coupling_combos: bool,
    admin_usernames: HashSet<String>,
    max_edit_s: i64,
}
impl BimPlugin {
    fn load_bim_database(&self, company: &str) -> Option<HashMap<VehicleNumber, VehicleInfo>> {
        let path_opt = match self.company_to_definition.get(company) {
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
        let mut vehicles: Vec<VehicleInfo> = match serde_json::from_reader(f) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse bim database: {}", e);
                return None;
            },
        };
        let vehicle_hash_map: HashMap<VehicleNumber, VehicleInfo> = vehicles.drain(..)
            .map(|vi| (vi.number, vi))
            .collect();
        Some(vehicle_hash_map)
    }

    async fn connect_ride_db(&self) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
        let (client, connection) = match tokio_postgres::connect(&self.ride_db_conn_string, NoTls).await {
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

        let number_str = command.rest.trim();
        let number: VehicleNumber = match number_str.parse() {
            Ok(n) => n,
            Err(e) => {
                error!("failed to parse {:?} as VehicleNumber: {}", number_str, e);
                return;
            },
        };

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(self.default_company.as_str());

        let mut response = match self.load_bim_database(company) {
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
                            let full_manuf = self.manufacturer_mapping.get(manuf).unwrap_or(manuf);
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

        async fn get_last_ride(me: &BimPlugin, username: &str, company: &str, vehicle_number: VehicleNumber) -> Option<String> {
            let ride_conn = match me.connect_ride_db().await {
                Ok(c) => c,
                Err(_) => return None,
            };
            let vehicle_number_i64: i64 = vehicle_number.into();

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
                &[&company, &vehicle_number_i64],
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
                    &[&company, &vehicle_number_i64, &username],
                ).await;
                match ride_row_opt_res {
                    Ok(Some(lrr)) => {
                        let last_rider_username: String = lrr.get(0);
                        let last_ride: DateTime<Local> = lrr.get(1);
                        let last_line: Option<String> = lrr.get(2);

                        write_expect!(ret,
                            " {} last rode it {}",
                            if *is_you { "You" } else { last_rider_username.as_str() },
                            BimPlugin::canonical_date_format(&last_ride, true, true),
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
        if let Some(last_ride) = get_last_ride(&self, &channel_message.message.sender.username, &company, number).await {
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

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(self.default_company.as_str());

        let company_def = match self.company_to_definition.get(company) {
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

        let bim_database_opt = self.load_bim_database(company);
        let mut ride_conn = match self.connect_ride_db().await {
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

        let increment_res = {
            increment_rides_by_spec(
                &mut ride_conn,
                bim_database_opt.as_ref(),
                company,
                company_def,
                &channel_message.message.sender.username,
                channel_message.message.timestamp.with_timezone(&Local),
                &command.rest,
                self.allow_fixed_coupling_combos,
            ).await
        };
        let mut last_ride_infos = match increment_res {
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

        // do not output fixed-coupling vehicles
        last_ride_infos.vehicles.retain(|v| !v.ridden_within_fixed_coupling);

        let response_str = if last_ride_infos.vehicles.len() == 1 {
            let vehicle_ride = &last_ride_infos.vehicles[0];
            let vehicle_type = if let Some(bd) = &bim_database_opt {
                bd
                    .get(&vehicle_ride.vehicle_number)
                    .map(|vi| vi.type_code.as_str())
            } else {
                None
            }.unwrap_or("vehicle");

            let mut resp = format!(
                "{} is currently riding {} number {}",
                channel_message.message.sender.username,
                vehicle_type,
                vehicle_ride.vehicle_number,
            );
            if let Some(line) = last_ride_infos.line {
                write_expect!(&mut resp, " on line {}", line);
            }
            write_expect!(&mut resp, ". ");

            if let Some(lr) = &vehicle_ride.last_ride {
                write_expect!(
                    &mut resp,
                    "This is their {}{} ride in this vehicle (previously {}",
                    lr.ride_count + 1,
                    Self::english_ordinal(lr.ride_count + 1),
                    BimPlugin::canonical_date_format(&lr.last_ride, true, false),
                );
                if let Some(ln) = &lr.last_line {
                    write_expect!(&mut resp, " on line {}", ln);
                }
                write_expect!(&mut resp, ")");
                if let Some(or) = &vehicle_ride.last_ride_other_rider {
                    write_expect!(
                        &mut resp,
                        " and {} has also ridden it {}",
                        or.rider_username,
                        BimPlugin::canonical_date_format(&or.last_ride, true, false),
                    );
                    if let Some(ln) = &or.last_line {
                        write_expect!(&mut resp, " on line {}", ln);
                    }
                }
                write_expect!(&mut resp, ".");
            } else {
                write_expect!(&mut resp, "This is their first ride in this vehicle");
                if let Some(or) = &vehicle_ride.last_ride_other_rider {
                    write_expect!(
                        &mut resp,
                        ", but {} has previously ridden it {}",
                        or.rider_username,
                        BimPlugin::canonical_date_format(&or.last_ride, true, false),
                    );
                    if let Some(ln) = &or.last_line {
                        write_expect!(&mut resp, " on line {}", ln);
                    }
                }
                write_expect!(&mut resp, "!");
            }

            write_expect!(&mut resp, " (ride {})", last_ride_infos.ride_id);

            resp
        } else {
            // multiple vehicles
            let mut resp = format!(
                "{} is currently riding:",
                channel_message.message.sender.username,
            );
            for vehicle_ride in &last_ride_infos.vehicles {
                let vehicle_type = if let Some(bd) = &bim_database_opt {
                    bd
                        .get(&vehicle_ride.vehicle_number)
                        .map(|vi| vi.type_code.as_str())
                } else {
                    None
                }.unwrap_or("vehicle");

                write_expect!(&mut resp, "\n* {} number {} (", vehicle_type, vehicle_ride.vehicle_number);
                if let Some(lr) = &vehicle_ride.last_ride {
                    write_expect!(
                        &mut resp,
                        "{}{} time, previously {}",
                        lr.ride_count + 1,
                        Self::english_ordinal(lr.ride_count + 1),
                        BimPlugin::canonical_date_format(&lr.last_ride, false, false),
                    );
                    if let Some(ln) = &lr.last_line {
                        write_expect!(&mut resp, " on line {}", ln);
                    }
                } else {
                    write_expect!(&mut resp, "first time");
                    if let Some(or) = &vehicle_ride.last_ride_other_rider {
                        write_expect!(
                            &mut resp,
                            " since {} {}",
                            or.rider_username,
                            BimPlugin::canonical_date_format(&or.last_ride, false, false),
                        );
                        if let Some(ln) = &or.last_line {
                            write_expect!(&mut resp, " on line {}", ln);
                        }
                    }
                    write_expect!(&mut resp, "!");
                }
                write_expect!(&mut resp, ")");
            }
            resp.push_str("\n");
            if let Some(ln) = &last_ride_infos.line {
                write_expect!(&mut resp, "on line {} ", ln);
            }
            write_expect!(&mut resp, "(ride {})", last_ride_infos.ride_id);
            resp
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
    }

    async fn channel_command_topbims(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(self.default_company.as_str());
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

        if !self.company_to_definition.contains_key(company) {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Unknown company.",
            ).await;
            return;
        }

        let ride_conn = match self.connect_ride_db().await {
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
                    total_rides(vehicle_number, total_ride_count) AS (
                        SELECT
                            rv.vehicle_number,
                            CAST(COUNT(*) AS bigint) total_ride_count
                        FROM
                            bim.rides r
                            INNER JOIN bim.ride_vehicles rv
                                ON rv.ride_id = r.id
                        WHERE
                            r.company = $1
                            AND rv.fixed_coupling_position = 0
                            {LOOKBACK_TIMESTAMP}
                        GROUP BY
                            rv.vehicle_number
                    ),
                    top_five_counts(total_ride_count) AS (
                        SELECT DISTINCT total_ride_count
                        FROM total_rides
                        ORDER BY total_ride_count DESC
                        LIMIT 5
                    )
                SELECT tr.vehicle_number, tr.total_ride_count
                FROM total_rides tr
                WHERE tr.total_ride_count IN (SELECT total_ride_count FROM top_five_counts)
                ORDER BY tr.total_ride_count DESC, tr.vehicle_number
            ",
            "AND r.\"timestamp\" >= $2",
            "",
            lookback_range,
            &[&company],
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

        let mut count_to_vehicles: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for row in &rows {
            let vehicle_number: i64 = row.get(0);
            let total_ride_count: i64 = row.get(1);

            count_to_vehicles
                .entry(total_ride_count)
                .or_insert_with(|| Vec::new())
                .push(vehicle_number);
        }

        let response_str = if count_to_vehicles.len() == 0 {
            format!("No vehicles have been ridden yet!")
        } else {
            let mut output = format!("The most ridden vehicles are:");
            for (&count, vehicle_numbers) in count_to_vehicles.iter().rev() {
                let times = Self::english_adverbial_number(count);

                let vehicle_number_strings: Vec<String> = vehicle_numbers.iter()
                    .map(|vn| vn.to_string())
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

        let ride_conn = match self.connect_ride_db().await {
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

        if self.company_to_definition.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "There are no companies.",
            ).await;
            return;
        }

        let mut company_names: Vec<(&String, &String)> = self.company_to_definition
            .iter()
            .map(|(abbr, cd)| (abbr, &cd.name))
            .collect();
        company_names.sort_unstable();

        let mut response_str = "The following companies exist:".to_owned();
        for (abbr, company_name) in company_names {
            write_expect!(&mut response_str, "\n* `{}` ({})", abbr, company_name);
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

        let ride_conn = match self.connect_ride_db().await {
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
            let vehicle_number_i64: i64 = row.get(2);
            let vehicle_number: VehicleNumber = match vehicle_number_i64.try_into() {
                Ok(vn) => vn,
                Err(_) => continue,
            };
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
                        .insert((comp.clone(), *veh_no));
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
                        if comp == &self.default_company {
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
                    if fav_comp == &self.default_company {
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

        let ride_conn = match self.connect_ride_db().await {
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

            let date = NaiveDate::from_ymd(
                year.try_into().expect("invalid year"),
                month.try_into().expect("invalid month"),
                day.try_into().expect("invalid day"),
            );

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

    async fn channel_command_bimridertypes(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

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

        let ride_conn = match self.connect_ride_db().await {
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
                    AND rv.as_part_of_fixed_coupling = FALSE
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
            let vehicle_number_i64: i64 = row.get(1);
            let vehicle_number: VehicleNumber = match vehicle_number_i64.try_into() {
                Ok(vn) => vn,
                Err(_) => continue,
            };
            let ride_count: i64 = row.get(2);

            let bim_database_opt = company_to_bim_database_opt
                .entry(company.clone())
                .or_insert_with(|| self.load_bim_database(&company));
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
                if *comp == self.default_company.as_str() {
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

        let ride_conn = match self.connect_ride_db().await {
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
                if *comp == self.default_company.as_str() {
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

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(self.default_company.as_str());
        if company.len() == 0 {
            return;
        }

        let wants_precise =
            command.flags.contains("precise")
            || command.flags.contains("p")
        ;

        let database = match self.load_bim_database(company) {
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
            let mut type_to_ranges: BTreeMap<String, RangeSet<VehicleNumber>> = BTreeMap::new();
            for (&veh_id, veh_info) in database.iter() {
                type_to_ranges
                    .entry(veh_info.type_code.clone())
                    .or_insert_with(|| RangeSet::new())
                    .insert(veh_id);
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
            for (&veh_id, veh_info) in database.iter() {
                type_to_range
                    .entry(veh_info.type_code.clone())
                    .and_modify(|(old_low, old_high)| {
                        if *old_low > veh_id {
                            *old_low = veh_id;
                        }
                        if *old_high < veh_id {
                            *old_high = veh_id;
                        }
                    })
                    .or_insert((veh_id, veh_id));
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

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(self.default_company.as_str());
        if company.len() == 0 {
            return;
        }
        let company_name = match self.company_to_definition.get(company) {
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

        let database = match self.load_bim_database(company) {
            Some(db) => db,
            None => HashMap::new(), // work with an empty database
        };

        let ride_conn = match self.connect_ride_db().await {
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
            let vehicle_number_i64: i64 = row.get(0);
            let vehicle_number: VehicleNumber = match vehicle_number_i64.try_into() {
                Ok(vn) => vn,
                Err(_) => continue,
            };
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

        // forbid anything trailing the command options
        if command.rest.trim().len() > 0 {
            return;
        }

        let sender_username = channel_message.message.sender.username.as_str();

        let delete = command.flags.contains("d") || command.flags.contains("delete");
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

        let is_admin = self.admin_usernames.contains(sender_username);

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
        }

        if delete && (new_rider.is_some() || new_company.is_some() || new_line.is_some()) {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Doesn't make much sense to delete a ride _and_ change its properties at the same time.",
            ).await;
            return;
        }

        if !delete && new_rider.is_none() && new_company.is_none() && new_line.is_none() {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Nothing to change.",
            ).await;
            return;
        }

        if let Some(nc) = new_company {
            if !self.company_to_definition.contains_key(nc) {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "That company does not exist.",
                ).await;
                return;
            }

            // FIXME: verify vehicle and line numbers against company-specific regexes?
        }

        // find the ride
        let ride_conn = match self.connect_ride_db().await {
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

        let ride_row_opt_res = if let Some(rid) = ride_id {
            ride_conn.query_opt(
                "
                    SELECT id, rider_username, \"timestamp\" FROM bim.rides
                    WHERE id=$1
                ",
                &[&rid],
            ).await
        } else {
            ride_conn.query_opt(
                "
                    SELECT id, rider_username, \"timestamp\" FROM bim.rides
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

        if !is_admin {
            let max_edit_dur = Duration::seconds(self.max_edit_s);
            let now = Local::now();
            if now - ride_timestamp > max_edit_dur {
                if self.max_edit_s > 0 {
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
            let response = match ride_conn.execute("DELETE FROM bim.rides WHERE id=$1", &[&ride_id]).await {
                Ok(_) => format!("Ride {} deleted.", ride_id),
                Err(e) => {
                    error!("failed to delete ride {}: {}", ride_id, e);
                    format!("Failed to delete ride {}.", ride_id)
                },
            };
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &response,
            ).await;
            return;
        }

        // update what there is to update
        let mut props: Vec<String> = Vec::new();
        let mut values: Vec<&(dyn ToSql + Sync)> = Vec::new();

        let (remember_new_rider, remember_new_company, remember_new_line);
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

        let props_string = props.join(", ");
        let query = format!("UPDATE bim.rides SET {} WHERE id = ${}", props_string, props.len() + 1);
        values.push(&ride_id);

        let response = match ride_conn.execute(&query, &values).await {
            Ok(_) => format!("Ride {} modified.", ride_id),
            Err(e) => {
                error!("failed to modify ride {}: {}", ride_id, e);
                format!("Failed to modify ride {}.", ride_id)
            },
        };
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn channel_command_widestbims(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

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

        let ride_conn = match self.connect_ride_db().await {
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
            let vehicle_number_i64: i64 = ride_row.get(1);
            let vehicle_number: VehicleNumber = vehicle_number_i64.try_into().unwrap();
            let rider_count: i64 = ride_row.get(2);

            max_rider_count_opt = Some(rider_count);

            if company == self.default_company {
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

    fn english_ordinal(num: usize) -> &'static str {
        let by_hundred = num % 100;
        if by_hundred > 10 && by_hundred < 14 {
            // teens are "th"
            return "th";
        }

        let by_one = num % 10;
        if by_one == 1 {
            "st"
        } else if by_one == 2 {
            "nd"
        } else if by_one == 3 {
            "rd"
        } else {
            "th"
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
}
#[async_trait]
impl RocketBotPlugin for BimPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let company_to_definition: HashMap<String, CompanyDefinition> = serde_json::from_value(config["company_to_definition"].clone())
            .expect("failed to decode company definitions");
        let default_company = config["default_company"]
            .as_str().expect("default_company not a string")
            .to_owned();

        let manufacturer_mapping = if config["manufacturer_mapping"].is_null() {
            HashMap::new()
        } else {
            let mut mapping = HashMap::new();
            for (k, v) in config["manufacturer_mapping"].entries().expect("manufacturer_mapping not an object") {
                let v_str = v.as_str().expect("manufacturer_mapping value not a string");
                mapping.insert(k.to_owned(), v_str.to_owned());
            }
            mapping
        };

        let ride_db_conn_string = config["ride_db_conn_string"]
            .as_str().expect("ride_db_conn_string is not a string")
            .to_owned();

        let allow_fixed_coupling_combos = if config["allow_fixed_coupling_combos"].is_null() {
            false
        } else {
            config["allow_fixed_coupling_combos"]
                .as_bool().expect("allow_fixed_coupling_combos not a boolean")
        };

        let admin_usernames = if config["admin_usernames"].is_null() {
            HashSet::new()
        } else {
            config["admin_usernames"]
                .as_array().expect("admin_usernames not a list")
                .iter()
                .map(|e|
                    e
                        .as_str().expect("admin_usernames entry not a string")
                        .to_owned()
                )
                .collect()
        };

        let max_edit_s = if config["max_edit_s"].is_null() {
            0
        } else {
            config["max_edit_s"]
                .as_i64().expect("max_edit_s not an i64")
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bim".to_owned(),
                "bim".to_owned(),
                "{cpfx}bim NUMBER".to_owned(),
                "Obtains information about the public transport vehicle with the given number.".to_owned(),
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimride".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimride VEHICLE[+VEHICLE...][/LINE]".to_owned(),
                "Registers a ride with the given vehicle(s) on the given line.".to_owned(),
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topbims".to_owned(),
                "bim".to_owned(),
                "{cpfx}topbims".to_owned(),
                "Returns the most-ridden vehicle(s).".to_owned(),
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topriders".to_owned(),
                "bim".to_owned(),
                "{cpfx}topriders".to_owned(),
                "Returns the most active rider(s).".to_owned(),
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimcompanies".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimcompanies".to_owned(),
                "Returns known public-transport operators.".to_owned(),
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "favbims".to_owned(),
                "bim".to_owned(),
                "{cpfx}favbims".to_owned(),
                "Returns each rider's most-ridden vehicle.".to_owned(),
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topbimdays".to_owned(),
                "bim".to_owned(),
                "{cpfx}topbimdays".to_owned(),
                "Returns the days with the most vehicle rides.".to_owned(),
            )
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimridertypes".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimridertypes [-n] USERNAME".to_owned(),
                "Returns the types of vehicles ridden by a rider.".to_owned(),
            )
                .add_flag("n")
                .add_flag("sort-by-number")
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimriderlines".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimriderlines [-n] USERNAME".to_owned(),
                "Returns the lines ridden by a rider.".to_owned(),
            )
                .add_flag("n")
                .add_flag("sort-by-number")
                .add_lookback_flags()
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimranges".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimranges [-p] [-c COMPANY]".to_owned(),
                "Returns the number ranges of each vehicle type.".to_owned(),
            )
                .add_flag("precise")
                .add_flag("p")
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimtypes".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimtypes [-c COMPANY] [USERNAME]".to_owned(),
                "Returns statistics about vehicle types.".to_owned(),
            )
                .add_option("company", CommandValueType::String)
                .add_option("c", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "fixbimride".to_owned(),
                "bim".to_owned(),
                "{cpfx}fixbimride OPTIONS".to_owned(),
                "Fix some data in a bim ride.".to_owned(),
            )
                .add_flag("d")
                .add_flag("delete")
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
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "widestbims".to_owned(),
                "bim".to_owned(),
                "{cpfx}widestbims".to_owned(),
                "Lists vehicles that have served the widest selection of riders.".to_owned(),
            )
                .add_lookback_flags()
                .build()
        ).await;

        Self {
            interface,
            company_to_definition,
            default_company,
            manufacturer_mapping,
            ride_db_conn_string,
            allow_fixed_coupling_combos,
            admin_usernames,
            max_edit_s,
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
        } else {
            None
        }
    }
}


pub async fn add_ride(
    ride_conn: &tokio_postgres::Transaction<'_>,
    company: &str,
    vehicles: &[NewVehicleEntry],
    rider_username: &str,
    timestamp: DateTime<Local>,
    line: Option<&str>,
) -> Result<(i64, Vec<(Option<LastRideInfo>, Option<OtherRiderInfo>)>), tokio_postgres::Error> {
    let prev_my_count_stmt = ride_conn.prepare(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username = $3
        ",
    ).await?;
    let prev_my_row_stmt = ride_conn.prepare(
        "
            SELECT r.\"timestamp\", r.line
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username = $3
            ORDER BY r.\"timestamp\" DESC
            LIMIT 1
        ",
    ).await?;
    let prev_other_count_stmt = ride_conn.prepare(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username <> $3
        ",
    ).await?;
    let prev_other_row_stmt = ride_conn.prepare(
        "
            SELECT r.\"timestamp\", r.line, r.rider_username
            FROM bim.rides r
            INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
            WHERE
                r.company = $1
                AND rv.vehicle_number = $2
                AND r.rider_username <> $3
            ORDER BY r.\"timestamp\" DESC
            LIMIT 1
        ",
    ).await?;

    let mut vehicle_results = Vec::new();
    for vehicle in vehicles {
        let vehicle_number_i64: i64 = vehicle.number.into();

        let prev_my_count_row = ride_conn.query_one(
            &prev_my_count_stmt,
            &[&company, &vehicle_number_i64, &rider_username],
        ).await?;
        let prev_my_count: i64 = prev_my_count_row.get(0);

        let prev_my_row_opt = ride_conn.query_opt(
            &prev_my_row_stmt,
            &[&company, &vehicle_number_i64, &rider_username],
        ).await?;
        let prev_my_timestamp: Option<DateTime<Local>> = prev_my_row_opt.as_ref().map(|r| r.get(0));
        let prev_my_line: Option<String> = prev_my_row_opt.as_ref().map(|r| r.get(1)).flatten();

        let prev_other_count_row = ride_conn.query_one(
            &prev_other_count_stmt,
            &[&company, &vehicle_number_i64, &rider_username],
        ).await?;
        let prev_other_count: i64 = prev_other_count_row.get(0);

        let prev_other_row_opt = ride_conn.query_opt(
            &prev_other_row_stmt,
            &[&company, &vehicle_number_i64, &rider_username],
        ).await?;
        let prev_other_timestamp: Option<DateTime<Local>> = prev_other_row_opt.as_ref().map(|r| r.get(0));
        let prev_other_line: Option<String> = prev_other_row_opt.as_ref().map(|r| r.get(1)).flatten();
        let prev_other_rider: Option<String> = prev_other_row_opt.as_ref().map(|r| r.get(2));

        let last_info = if prev_my_count > 0 {
            let pmc_usize: usize = prev_my_count.try_into().unwrap();
            Some(LastRideInfo {
                ride_count: pmc_usize,
                last_ride: prev_my_timestamp.unwrap(),
                last_line: prev_my_line,
            })
        } else {
            None
        };

        let other_info = if prev_other_count > 0 {
            Some(OtherRiderInfo {
                rider_username: prev_other_rider.unwrap(),
                last_ride: prev_other_timestamp.unwrap(),
                last_line: prev_other_line,
            })
        } else {
            None
        };

        vehicle_results.push((last_info, other_info));
    }

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

    let insert_vehicle_stmt = ride_conn.prepare(
        "
            INSERT INTO bim.ride_vehicles
                (ride_id, vehicle_number, vehicle_type, spec_position, as_part_of_fixed_coupling, fixed_coupling_position)
            VALUES
                ($1, $2, $3, $4, $5, $6)
        ",
    ).await?;

    for vehicle in vehicles {
        let vehicle_number_i64: i64 = vehicle.number.into();
        ride_conn.execute(
            &insert_vehicle_stmt,
            &[
                &ride_id,
                &vehicle_number_i64,
                &vehicle.type_code,
                &vehicle.spec_position,
                &vehicle.as_part_of_fixed_coupling,
                &vehicle.fixed_coupling_position,
            ],
        ).await?;
    }

    Ok((ride_id, vehicle_results))
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

pub async fn increment_rides_by_spec(
    ride_conn: &mut tokio_postgres::Client,
    bim_database_opt: Option<&HashMap<VehicleNumber, VehicleInfo>>,
    company: &str,
    company_def: &CompanyDefinition,
    rider_username: &str,
    timestamp: DateTime<Local>,
    rides_spec: &str,
    allow_fixed_coupling_combos: bool,
) -> Result<RideInfo, IncrementBySpecError> {
    let spec_no_spaces = rides_spec.replace(" ", "");
    let caps = match company_def.vehicle_and_line_regex().captures(&spec_no_spaces) {
        Some(c) => c,
        None => return Err(IncrementBySpecError::SpecParseFailure(spec_no_spaces)),
    };

    let vehicles_str = caps.name("vehicles").expect("failed to capture vehicles").as_str();
    let line_str_opt = caps.name("line").map(|l| l.as_str());

    let vehicle_num_strs: Vec<&str> = vehicles_str.split("+").collect();
    let mut vehicle_nums = Vec::new();
    for &vehicle_num_str in &vehicle_num_strs {
        let vehicle_num: VehicleNumber = match vehicle_num_str.parse() {
            Ok(vn) => vn,
            Err(e) => return Err(IncrementBySpecError::VehicleNumberParseFailure(vehicle_num_str.to_owned(), e)),
        };
        if !allow_fixed_coupling_combos {
            if let Some(bim_database) = bim_database_opt {
                if let Some(veh) = bim_database.get(&vehicle_num) {
                    if veh.fixed_coupling.len() > 0 && vehicle_num_strs.len() > 1 {
                        // this vehicle is in a fixed coupling but we have more than one vehicle
                        // this is forbidden
                        return Err(IncrementBySpecError::FixedCouplingCombo(vehicle_num));
                    }
                }
            }
        }
        vehicle_nums.push(vehicle_num);
    }

    // also count vehicles ridden in a fixed coupling with the given vehicle
    let mut all_vehicles: Vec<NewVehicleEntry> = Vec::new();
    let explicit_vehicle_num_set: HashSet<VehicleNumber> = vehicle_nums.iter()
        .map(|vn| *vn)
        .collect();
    let mut seen_vehicles: HashSet<VehicleNumber> = HashSet::new();
    for (spec_pos, &vehicle_num) in vehicle_nums.iter().enumerate() {
        let mut added_fixed_coupling = false;
        let mut type_code = None;
        if let Some(bim_database) = bim_database_opt {
            if let Some(veh) = bim_database.get(&vehicle_num) {
                type_code = Some(veh.type_code.clone());

                for (fc_pos, &fc) in veh.fixed_coupling.iter().enumerate() {
                    if !seen_vehicles.insert(fc) {
                        // we've seen this vehicle before
                        continue;
                    }

                    let fc_type_code = bim_database.get(&fc)
                        .map(|veh| veh.type_code.clone());
                    let vehicle = NewVehicleEntry {
                        number: fc,
                        type_code: fc_type_code,
                        spec_position: spec_pos.try_into().unwrap(),
                        as_part_of_fixed_coupling: !explicit_vehicle_num_set.contains(&fc),
                        fixed_coupling_position: fc_pos.try_into().unwrap(),
                    };
                    all_vehicles.push(vehicle);
                    added_fixed_coupling = true;
                }
            }
        }

        if !added_fixed_coupling {
            if !seen_vehicles.insert(vehicle_num) {
                // we've seen this vehicle before
                continue;
            }

            let vehicle = NewVehicleEntry {
                number: vehicle_num,
                type_code,
                spec_position: spec_pos.try_into().unwrap(),
                as_part_of_fixed_coupling: !explicit_vehicle_num_set.contains(&vehicle_num),
                fixed_coupling_position: 0,
            };
            all_vehicles.push(vehicle);
        }
    }

    let (ride_id, vehicle_ride_infos) = {
        let xact = ride_conn.transaction().await
            .map_err(|e| IncrementBySpecError::DatabaseBeginTransaction(e))?;

        let (rid, mut ride_info_vec) = add_ride(
            &xact,
            company,
            &all_vehicles,
            rider_username,
            timestamp,
            line_str_opt,
        )
            .await.map_err(|e|
                IncrementBySpecError::DatabaseQuery(rider_username.to_owned(), all_vehicles.clone(), line_str_opt.map(|l| l.to_owned()), e)
            )?;

        let mut vehicle_ride_infos = Vec::new();
        for (new_vehicle, (lri_opt, ori_opt)) in all_vehicles.iter().zip(ride_info_vec.drain(..)) {
            vehicle_ride_infos.push(VehicleRideInfo {
                vehicle_number: new_vehicle.number,
                ridden_within_fixed_coupling: new_vehicle.as_part_of_fixed_coupling,
                last_ride: lri_opt,
                last_ride_other_rider: ori_opt,
            });
        }

        xact.commit().await
            .map_err(|e| IncrementBySpecError::DatabaseCommitTransaction(e))?;

        (rid, vehicle_ride_infos)
    };

    Ok(RideInfo {
        ride_id,
        line: line_str_opt.map(|s| s.to_owned()),
        vehicles: vehicle_ride_infos,
    })
}
