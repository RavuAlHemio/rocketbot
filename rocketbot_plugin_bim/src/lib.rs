use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::{self, Write};
use std::fs::File;
use std::num::ParseIntError;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{DateTime, Local};
use log::error;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_postgres::NoTls;


pub static BIMRIDE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^(?P<vehicles>[0-9]+(?:[+][0-9]+)*)(?:/(?P<line>[0-9A-Z]+|Sonderzug))?$"
).expect("failed to parse bimride regex"));
pub const UPSERT_RIDE_QUERY: &'static str = "
INSERT INTO bim.last_rides AS blr
    (vehicle_number, rider_username, ride_count, last_ride, last_line)
VALUES
    ($1, $2, 1, $3, $4)
ON CONFLICT (vehicle_number, rider_username) DO UPDATE
    SET
        ride_count = blr.ride_count + 1,
        last_ride = $3,
        last_line = $4
RETURNING
    (
        SELECT prev.ride_count
        FROM bim.last_rides prev
        WHERE prev.vehicle_number = blr.vehicle_number
        AND prev.rider_username = blr.rider_username
    ) ride_count,
    (
        SELECT prev.last_ride
        FROM bim.last_rides prev
        WHERE prev.vehicle_number = blr.vehicle_number
        AND prev.rider_username = blr.rider_username
    ) last_ride,
    (
        SELECT prev.last_line
        FROM bim.last_rides prev
        WHERE prev.vehicle_number = blr.vehicle_number
        AND prev.rider_username = blr.rider_username
    ) last_line
";


macro_rules! write_expect {
    ($dst:expr, $($arg:tt)*) => {
        write!($dst, $($arg)*).expect("write failed")
    };
}


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehicleInfo {
    pub number: u32,
    pub type_code: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub other_data: HashMap<String, String>,
}
impl VehicleInfo {
    pub fn new(number: u32, type_code: String) -> Self {
        Self {
            number,
            type_code,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: HashMap::new(),
        }
    }
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LastRideInfo {
    pub ride_count: usize,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehicleRideInfo {
    pub vehicle_number: u32,
    pub last_ride: Option<LastRideInfo>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideInfo {
    pub line: Option<String>,
    pub vehicles: Vec<VehicleRideInfo>,
}


pub struct BimPlugin {
    interface: Weak<dyn RocketBotInterface>,
    bim_database_path: String,
    company_mapping: HashMap<String, String>,
    ride_db_conn_string: String,
}
impl BimPlugin {
    fn load_bim_database(&self) -> Option<HashMap<u32, VehicleInfo>> {
        let f = match File::open(&self.bim_database_path) {
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
        let vehicle_hash_map: HashMap<u32, VehicleInfo> = vehicles.drain(..)
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

    async fn channel_command_bim(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let number_str = command.rest.trim();
        let number: u32 = match number_str.parse() {
            Ok(n) => n,
            Err(e) => {
                error!("failed to parse {:?} as u32: {}", number_str, e);
                return;
            },
        };

        let database = match self.load_bim_database() {
            Some(db) => db,
            None => {
                return;
            },
        };
        let vehicle = match database.get(&number) {
            Some(v) => v,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Vehicle {} not found.", number),
                ).await;
                return;
            },
        };

        let mut response = format!(
            "*{}*: type *{}*",
            vehicle.number, vehicle.type_code,
        );
        match (&vehicle.in_service_since, &vehicle.out_of_service_since) {
            (Some(service_from), Some(service_to)) => {
                write!(response, ", in service from {} to {}", service_from, service_to).expect("failed to write");
            },
            (Some(service_from), None) => {
                write!(response, ", in service since {}", service_from).expect("failed to write");
            },
            (None, Some(service_to)) => {
                write!(response, ", in service until {}", service_to).expect("failed to write");
            },
            (None, None) => {},
        };

        if let Some(manuf) = &vehicle.manufacturer {
            let full_manuf = self.company_mapping.get(manuf).unwrap_or(manuf);
            write!(response, "\n*hergestellt von* {}", full_manuf).expect("failed to write");
        }

        let mut other_props: Vec<(&str, &str)> = vehicle.other_data.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        other_props.sort_unstable();
        for (key, val) in other_props {
            write!(response, "\n*{}*: {}", key, val).expect("failed to write");
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

        let bim_database_opt = self.load_bim_database();
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

        let increment_res = increment_rides_by_spec(
            &mut ride_conn,
            &channel_message.message.sender.username,
            channel_message.message.timestamp.with_timezone(&Local),
            &command.rest,
        ).await;
        let last_ride_infos = match increment_res {
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
                    lr.last_ride.format("on %Y-%m-%d at %H:%M"),
                );
                if let Some(ln) = &lr.last_line {
                    write_expect!(&mut resp, " on line {}", ln);
                }
                write_expect!(&mut resp, ").");
            } else {
                write_expect!(&mut resp, "This is their first ride in this vehicle!");
            }

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
                        lr.last_ride.format("on %Y-%m-%d at %H:%M"),
                    );
                    if let Some(ln) = &lr.last_line {
                        write_expect!(&mut resp, " on line {}", ln);
                    }
                } else {
                    write_expect!(&mut resp, "first time!");
                }
                write_expect!(&mut resp, ")");
            }
            if let Some(ln) = &last_ride_infos.line {
                write_expect!(&mut resp, "\non line {}", ln);
            }
            resp
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
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
}
#[async_trait]
impl RocketBotPlugin for BimPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let bim_database_path = config["bim_database_path"]
            .as_str().expect("bim_database_path is not a string")
            .to_owned();

        let company_mapping = if config["company_mapping"].is_null() {
            HashMap::new()
        } else {
            let mut mapping = HashMap::new();
            for (k, v) in config["company_mapping"].entries().expect("company_mapping not an object") {
                let v_str = v.as_str().expect("company_mapping value not a string");
                mapping.insert(k.to_owned(), v_str.to_owned());
            }
            mapping
        };

        let ride_db_conn_string = config["ride_db_conn_string"]
            .as_str().expect("ride_db_conn_string is not a string")
            .to_owned();

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bim".to_owned(),
                "bim".to_owned(),
                "{cpfx}bim NUMBER".to_owned(),
                "Obtains information about the public transport vehicle with the given number.".to_owned(),
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bimride".to_owned(),
                "bim".to_owned(),
                "{cpfx}bimride VEHICLE[+VEHICLE...][/LINE]".to_owned(),
                "Registers a ride with the given vehicle(s) on the given line.".to_owned(),
            )
                .build()
        ).await;

        Self {
            interface,
            bim_database_path,
            company_mapping,
            ride_db_conn_string,
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "bim" {
            self.channel_command_bim(channel_message, command).await
        } else if command.name == "bimride" {
            self.channel_command_bimride(channel_message, command).await
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
        } else {
            None
        }
    }
}


pub async fn increment_last_ride(ride_conn: &tokio_postgres::Transaction<'_>, vehicle_number: u32, rider_username: &str, timestamp: DateTime<Local>, line: Option<&str>) -> Result<Option<LastRideInfo>, tokio_postgres::Error> {
    let vehicle_number_i64: i64 = vehicle_number.into();

    let row = ride_conn.query_one(UPSERT_RIDE_QUERY, &[&vehicle_number_i64, &rider_username, &timestamp, &line]).await?;
    let prev_ride_count: Option<i64> = row.get(0);
    let prev_last_ride: Option<DateTime<Local>> = row.get(1);
    let prev_last_line: Option<String> = row.get(2);

    if let Some(prc) = prev_ride_count {
        let prc_usize: usize = prc.try_into().unwrap();

        Ok(Some(LastRideInfo {
            ride_count: prc_usize,
            last_ride: prev_last_ride.unwrap(),
            last_line: prev_last_line,
        }))
    } else {
        Ok(None)
    }
}

#[derive(Debug)]
pub enum IncrementBySpecError {
    SpecParseFailure(String),
    VehicleNumberParseFailure(String, ParseIntError),
    DatabaseQuery(String, u32, Option<String>, tokio_postgres::Error),
    DatabaseBeginTransaction(tokio_postgres::Error),
    DatabaseCommitTransaction(tokio_postgres::Error),
}
impl fmt::Display for IncrementBySpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpecParseFailure(spec) => write!(f, "failed to parse spec {:?}", spec),
            Self::VehicleNumberParseFailure(num_str, e) => write!(f, "failed to parse vehicle number {:?}: {}", num_str, e),
            Self::DatabaseQuery(rider, vehicle_num, line_opt, e) => write!(f, "database query error registering {} riding on vehicle {} on line {:?}: {}", rider, vehicle_num, line_opt, e),
            Self::DatabaseBeginTransaction(e) => write!(f, "database error beginning transaction: {}", e),
            Self::DatabaseCommitTransaction(e) => write!(f, "database error committing transaction: {}", e),
        }
    }
}
impl std::error::Error for IncrementBySpecError {
}

pub async fn increment_rides_by_spec(ride_conn: &mut tokio_postgres::Client, rider_username: &str, timestamp: DateTime<Local>, rides_spec: &str) -> Result<RideInfo, IncrementBySpecError> {
    let spec_no_spaces = rides_spec.replace(" ", "");
    let caps = match BIMRIDE_RE.captures(&spec_no_spaces) {
        Some(c) => c,
        None => return Err(IncrementBySpecError::SpecParseFailure(spec_no_spaces)),
    };

    let vehicles_str = caps.name("vehicles").expect("failed to capture vehicles").as_str();
    let line_str_opt = caps.name("line").map(|l| l.as_str());

    let mut vehicle_nums = Vec::new();
    for vehicle_num_str in vehicles_str.split("+") {
        let vehicle_num: u32 = match vehicle_num_str.parse() {
            Ok(vn) => vn,
            Err(e) => return Err(IncrementBySpecError::VehicleNumberParseFailure(vehicle_num_str.to_owned(), e)),
        };
        vehicle_nums.push(vehicle_num);
    }

    let vehicle_ride_infos = {
        let xact = ride_conn.transaction().await
            .map_err(|e| IncrementBySpecError::DatabaseBeginTransaction(e))?;

        let mut vehicle_ride_infos = Vec::new();
        for &vehicle_num in &vehicle_nums {
            let lri_opt = increment_last_ride(&xact, vehicle_num, rider_username, timestamp, line_str_opt).await
                .map_err(|e| IncrementBySpecError::DatabaseQuery(rider_username.to_owned(), vehicle_num, line_str_opt.map(|l| l.to_owned()), e))?;
            vehicle_ride_infos.push(VehicleRideInfo {
                vehicle_number: vehicle_num,
                last_ride: lri_opt,
            });
        }

        xact.commit().await
            .map_err(|e| IncrementBySpecError::DatabaseBeginTransaction(e))?;

        vehicle_ride_infos
    };

    Ok(RideInfo {
        line: line_str_opt.map(|s| s.to_owned()),
        vehicles: vehicle_ride_infos,
    })
}
