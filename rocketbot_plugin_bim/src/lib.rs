use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Write};
use std::fs::File;
use std::num::ParseIntError;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{DateTime, Local};
use indexmap::IndexSet;
use log::error;
use once_cell::sync::Lazy;
use rand::{Rng, thread_rng};
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, phrase_join, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_postgres::NoTls;


pub type VehicleNumber = u32;


pub static BIMRIDE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^(?P<vehicles>[0-9]+(?:[+][0-9]+)*)(?:/(?P<line>[0-9A-Z]+|Sonderzug))?$"
).expect("failed to parse bimride regex"));


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


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LastRideInfo {
    pub ride_count: usize,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtherRiderInfo {
    pub rider_username: String,
    pub last_ride: DateTime<Local>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehicleRideInfo {
    pub vehicle_number: VehicleNumber,
    pub ridden_within_fixed_coupling: bool,
    pub last_ride: Option<LastRideInfo>,
    pub last_ride_other_rider: Option<OtherRiderInfo>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideInfo {
    pub line: Option<String>,
    pub vehicles: Vec<VehicleRideInfo>,
}


pub struct BimPlugin {
    interface: Weak<dyn RocketBotInterface>,
    company_to_bim_database_path: HashMap<String, Option<String>>,
    default_company: String,
    manufacturer_mapping: HashMap<String, String>,
    ride_db_conn_string: String,
    allow_fixed_coupling_combos: bool,
}
impl BimPlugin {
    fn load_bim_database(&self, company: &str) -> Option<HashMap<VehicleNumber, VehicleInfo>> {
        let path_opt = match self.company_to_bim_database_path.get(company) {
            Some(p) => p,
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
                                write!(db_response, ", in service from {} to {}", service_from, service_to).expect("failed to write");
                            },
                            (Some(service_from), None) => {
                                write!(db_response, ", in service since {}", service_from).expect("failed to write");
                            },
                            (None, Some(service_to)) => {
                                write!(db_response, ", in service until {}", service_to).expect("failed to write");
                            },
                            (None, None) => {},
                        };

                        if let Some(manuf) = &vehicle.manufacturer {
                            let full_manuf = self.manufacturer_mapping.get(manuf).unwrap_or(manuf);
                            write!(db_response, "\n*hergestellt von* {}", full_manuf).expect("failed to write");
                        }

                        let mut other_props: Vec<(&str, &str)> = vehicle.other_data.iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();
                        other_props.sort_unstable();
                        for (key, val) in other_props {
                            write!(db_response, "\n*{}*: {}", key, val).expect("failed to write");
                        }

                        if vehicle.fixed_coupling.len() > 0 {
                            let fixed_coupling_strings: Vec<String> = vehicle.fixed_coupling.iter()
                                .map(|num| num.to_string())
                                .collect();
                            let fixed_coupling_string = fixed_coupling_strings.join("+");
                            write!(db_response, "\npart of fixed coupling: {}", fixed_coupling_string).expect("failed to write");
                        }

                        db_response
                    }
                }
            },
        };

        async fn get_last_ride(me: &BimPlugin, company: &str, vehicle_number: VehicleNumber) -> Option<String> {
            let ride_conn = match me.connect_ride_db().await {
                Ok(c) => c,
                Err(_) => return None,
            };
            let vehicle_number_i64: i64 = vehicle_number.into();

            // query ride count
            let count_row_opt_res = ride_conn.query_opt(
                "SELECT CAST(COALESCE(COUNT(*), 0) AS bigint) total_ride_count FROM bim.rides WHERE company = $1 AND vehicle_number = $2",
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
            } else if count == 1 {
                format!("This vehicle has been ridden once.")
            } else {
                format!("This vehicle has been ridden {} times.", count)
            };

            // query last rider
            let ride_row_opt_res = ride_conn.query_opt(
                "
                    SELECT rider_username, \"timestamp\", line
                    FROM bim.rides
                    WHERE company = $1 AND vehicle_number = $2
                    ORDER BY \"timestamp\" DESC
                    LIMIT 1
                ",
                &[&company, &vehicle_number_i64],
            ).await;
            match ride_row_opt_res {
                Ok(Some(lrr)) => {
                    let last_rider_username: String = lrr.get(0);
                    let last_ride: DateTime<Local> = lrr.get(1);
                    let last_line: Option<String> = lrr.get(2);

                    write!(ret,
                        " Last rider: {} {}",
                        last_rider_username, last_ride.format("on %Y-%m-%d at %H:%M:%S"),
                    ).expect("failed to write");
                    if let Some(ll) = last_line {
                        write!(ret, " on line {}", ll).expect("failed to write");
                    }
                    ret.push('.');
                },
                Ok(None) => {},
                Err(e) => {
                    error!("failed to obtain last rider: {}", e);
                    return None;
                },
            };

            Some(ret)
        }
        if let Some(last_ride) = get_last_ride(&self, &company, number).await {
            write!(response, "\n{}", last_ride).expect("failed to write");
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

        if !self.company_to_bim_database_path.contains_key(company) {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "Unknown company.",
            ).await;
            return;
        }

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

        let increment_res = increment_rides_by_spec(
            &mut ride_conn,
            bim_database_opt.as_ref(),
            company,
            &channel_message.message.sender.username,
            channel_message.message.timestamp.with_timezone(&Local),
            &command.rest,
            self.allow_fixed_coupling_combos,
        ).await;
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
                    lr.last_ride.format("on %Y-%m-%d at %H:%M"),
                );
                if let Some(ln) = &lr.last_line {
                    write_expect!(&mut resp, " on line {}", ln);
                }
                write_expect!(&mut resp, ").");
            } else {
                write_expect!(&mut resp, "This is their first ride in this vehicle");
                if let Some(or) = &vehicle_ride.last_ride_other_rider {
                    write_expect!(
                        &mut resp,
                        ", but {} has previously ridden it {}",
                        or.rider_username,
                        or.last_ride.format("on %Y-%m-%d at %H:%M"),
                    );
                    if let Some(ln) = &or.last_line {
                        write_expect!(&mut resp, " on line {}", ln);
                    }
                }
                write_expect!(&mut resp, "!");
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
                    write_expect!(&mut resp, "first time");
                    if let Some(or) = &vehicle_ride.last_ride_other_rider {
                        write_expect!(
                            &mut resp,
                            " since {} {}",
                            or.rider_username,
                            or.last_ride.format("on %Y-%m-%d at %H:%M"),
                        );
                        if let Some(ln) = &or.last_line {
                            write_expect!(&mut resp, " on line {}", ln);
                        }
                    }
                    write_expect!(&mut resp, "!");
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

    async fn channel_command_topbims(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let company = command.options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|v| v.as_str().unwrap())
            .unwrap_or(self.default_company.as_str());

        if !self.company_to_bim_database_path.contains_key(company) {
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

        let rows_res = ride_conn.query(
            "
                WITH total_rides(vehicle_number, total_ride_count) AS (
                    SELECT b.vehicle_number, CAST(COUNT(*) AS bigint) total_ride_count
                    FROM bim.rides b
                    WHERE b.company = $1
                    GROUP BY b.vehicle_number
                )
                SELECT tr.vehicle_number, tr.total_ride_count
                FROM total_rides tr
                WHERE NOT EXISTS (
                    SELECT 1 FROM total_rides tr2
                    WHERE tr2.total_ride_count > tr.total_ride_count
                )
                ORDER BY tr.vehicle_number
            ",
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

        let mut most_ridden_count_opt = None;
        let mut vehicle_numbers = Vec::with_capacity(rows.len());
        for row in &rows {
            let vehicle_number: i64 = row.get(0);
            let total_ride_count: i64 = row.get(1);

            vehicle_numbers.push(vehicle_number);
            most_ridden_count_opt = Some(total_ride_count);
        }

        let response_str = if vehicle_numbers.len() == 0 {
            format!("No vehicles have been ridden yet!")
        } else {
            let most_ridden_count = most_ridden_count_opt.unwrap();
            let times = if most_ridden_count == 1 {
                "once".to_owned()
            } else if most_ridden_count == 2 {
                "twice".to_owned()
            } else {
                // "thrice" and above are already too poetic
                format!("{} times", most_ridden_count)
            };

            if vehicle_numbers.len() == 1 {
                format!("The most ridden vehicle is {}, which has been ridden {}.", vehicle_numbers[0], times)
            } else {
                let vehicle_number_strings: Vec<String> = vehicle_numbers.iter()
                    .map(|vn| vn.to_string())
                    .collect();
                let vehicle_numbers_str = phrase_join(&vehicle_number_strings, ", ", " and ");
                format!("The most ridden vehicles are {}, which have been ridden {}.", vehicle_numbers_str, times)
            }
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
    }

    async fn channel_command_topriders(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
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

        let rows_res = ride_conn.query(
            "
                SELECT company, vehicle_number, rider_username, COUNT(*)
                FROM bim.rides
                GROUP BY company, vehicle_number, rider_username
            ",
            &[]
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

        let mut company_to_bim_database_opt = HashMap::new();
        let mut rider_to_ride_count = HashMap::new();
        for row in rows {
            let company: String = row.get(0);
            let vehicle_number_i64: i64 = row.get(1);
            let rider_username: String = row.get(2);
            let ride_count: i64 = row.get(3);

            if let Ok(vehicle_number_vn) = VehicleNumber::try_from(vehicle_number_i64) {
                let bim_database_opt = company_to_bim_database_opt
                    .entry(company.clone())
                    .or_insert_with(|| self.load_bim_database(&company));
                if let Some(bim_database) = bim_database_opt {
                    if let Some(vehicle) = bim_database.get(&vehicle_number_vn) {
                        if vehicle.fixed_coupling.first().map(|&f| f != vehicle_number_vn).unwrap_or(false) {
                            // only count the first vehicle in a fixed coupling
                            continue;
                        }
                    }
                }
            }

            let rider_ride_count = rider_to_ride_count
                .entry(rider_username.clone())
                .or_insert(0);
            *rider_ride_count += ride_count;
        }

        let mut rider_and_ride_count: Vec<(String, i64)> = rider_to_ride_count
            .iter()
            .map(|(r, rc)| (r.clone(), *rc))
            .collect();
        rider_and_ride_count.sort_unstable_by_key(|(r, rc)| (-*rc, r.clone()));
        let mut rider_strings: Vec<String> = rider_and_ride_count.iter()
            .map(|(rider_name, ride_count)| {
                if *ride_count == 1 {
                    format!("{} (one ride)", rider_name)
                } else {
                    format!("{} ({} rides)", rider_name, ride_count)
                }
            })
            .collect();
        let prefix = if rider_strings.len() < 6 {
            "Top riders: "
        } else {
            rider_strings.drain(5..);
            "Top 5 riders: "
        };
        let rider_string = rider_strings.join(", ");

        let response_string = if rider_string.len() == 0 {
            "No top riders.".to_owned()
        } else {
            format!("{}{}", prefix, rider_string)
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

        let mut company_names: Vec<&String> = self.company_to_bim_database_path
            .keys()
            .collect();
        if self.company_to_bim_database_path.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "There are no companies.",
            ).await;
            return;
        }

        company_names.sort_unstable();

        let mut response_str = "The following companies exist:".to_owned();
        for company_name in company_names {
            write_expect!(&mut response_str, "\n* `{}`", company_name);
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
    }

    async fn channel_command_favbims(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
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

        let rows_res = ride_conn.query(
            "
                WITH
                    rides_per_rider_vehicle(rider_username, company, vehicle_number, ride_count) AS (
                        SELECT rider_username, company, vehicle_number, COUNT(*)
                        FROM bim.rides
                        GROUP BY rider_username, company, vehicle_number
                    ),
                    fav_vehicle(rider_username, company, vehicle_number, ride_count) AS (
                        SELECT rider_username, company, vehicle_number, ride_count
                        FROM rides_per_rider_vehicle rprv
                        WHERE NOT EXISTS (
                            SELECT 1
                            FROM rides_per_rider_vehicle rprv2
                            WHERE rprv2.rider_username = rprv.rider_username
                            AND rprv2.ride_count > rprv.ride_count
                        )
                    )
                SELECT rider_username, company, vehicle_number, CAST(ride_count AS bigint)
                FROM fav_vehicle
            ",
            &[]
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

        let mut rider_to_fav_vehicles: BTreeMap<String, BTreeSet<(String, u32, i64)>> = BTreeMap::new();
        for row in rows {
            let rider_username: String = row.get(0);
            let company: String = row.get(1);
            let vehicle_number_i64: i64 = row.get(2);
            let vehicle_number_u32: u32 = match vehicle_number_i64.try_into() {
                Ok(vn) => vn,
                Err(_) => continue,
            };
            let ride_count: i64 = row.get(3);

            rider_to_fav_vehicles
                .entry(rider_username)
                .or_insert_with(|| BTreeSet::new())
                .insert((company, vehicle_number_u32, ride_count));
        }

        let mut fav_vehicle_strings = Vec::new();
        {
            let mut rng = thread_rng();
            for (rider, fav_vehicles) in rider_to_fav_vehicles.iter() {
                let fav_vehicles_count = fav_vehicles.len();
                if fav_vehicles_count == 0 {
                    continue;
                }
                let index = rng.gen_range(0..fav_vehicles_count);
                let (fav_comp, fav_vehicle, ride_count) = fav_vehicles.iter().nth(index).unwrap();
                fav_vehicle_strings.push(format!(
                    "{}: {}/{} ({} rides)",
                    rider, fav_comp, fav_vehicle, ride_count,
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

        let mut company_to_bim_database_path = HashMap::new();
        let c2db_map = config["company_to_bim_database_path"].as_object().expect("company_to_bim_database_path not an object");
        for (company, db_path_value) in c2db_map {
            let db_path = if db_path_value.is_null() {
                None
            } else {
                Some(db_path_value.as_str().expect("company_to_bim_database_path value not a string").to_owned())
            };
            company_to_bim_database_path.insert(company.to_owned(), db_path);
        }

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
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "topriders".to_owned(),
                "bim".to_owned(),
                "{cpfx}topriders".to_owned(),
                "Returns the most active rider(s).".to_owned(),
            )
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
                .build()
        ).await;

        Self {
            interface,
            company_to_bim_database_path,
            default_company,
            manufacturer_mapping,
            ride_db_conn_string,
            allow_fixed_coupling_combos,
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
        } else {
            None
        }
    }
}


pub async fn increment_last_ride(ride_conn: &tokio_postgres::Transaction<'_>, company: &str, vehicle_number: VehicleNumber, rider_username: &str, timestamp: DateTime<Local>, line: Option<&str>) -> Result<(Option<LastRideInfo>, Option<OtherRiderInfo>), tokio_postgres::Error> {
    let vehicle_number_i64: i64 = vehicle_number.into();

    let prev_my_count_row = ride_conn.query_one(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides
            WHERE
                company = $1
                AND vehicle_number = $2
                AND rider_username = $3
        ",
        &[&company, &vehicle_number_i64, &rider_username],
    ).await?;
    let prev_my_count: i64 = prev_my_count_row.get(0);

    let prev_my_row_opt = ride_conn.query_opt(
        "
            SELECT \"timestamp\", line
            FROM bim.rides
            WHERE
                company = $1
                AND vehicle_number = $2
                AND rider_username = $3
            ORDER BY \"timestamp\" DESC
            LIMIT 1
        ",
        &[&company, &vehicle_number_i64, &rider_username],
    ).await?;
    let prev_my_timestamp: Option<DateTime<Local>> = prev_my_row_opt.as_ref().map(|r| r.get(0));
    let prev_my_line: Option<String> = prev_my_row_opt.as_ref().map(|r| r.get(1)).flatten();

    let prev_other_count_row = ride_conn.query_one(
        "
            SELECT CAST(COUNT(*) AS bigint)
            FROM bim.rides
            WHERE
                company = $1
                AND vehicle_number = $2
                AND rider_username <> $3
        ",
        &[&company, &vehicle_number_i64, &rider_username],
    ).await?;
    let prev_other_count: i64 = prev_other_count_row.get(0);

    let prev_other_row_opt = ride_conn.query_opt(
        "
            SELECT \"timestamp\", line, rider_username
            FROM bim.rides
            WHERE
                company = $1
                AND vehicle_number = $2
                AND rider_username <> $3
            ORDER BY \"timestamp\" DESC
            LIMIT 1
        ",
        &[&company, &vehicle_number_i64, &rider_username],
    ).await?;
    let prev_other_timestamp: Option<DateTime<Local>> = prev_other_row_opt.as_ref().map(|r| r.get(0));
    let prev_other_line: Option<String> = prev_other_row_opt.as_ref().map(|r| r.get(1)).flatten();
    let prev_other_rider: Option<String> = prev_other_row_opt.as_ref().map(|r| r.get(2));

    ride_conn.execute(
        "
            INSERT INTO bim.rides AS br
                (id, company, vehicle_number, rider_username, \"timestamp\", line)
            VALUES
                (DEFAULT, $1, $2, $3, $4, $5)
        ",
        &[&company, &vehicle_number_i64, &rider_username, &timestamp, &line],
    ).await?;

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

    Ok((last_info, other_info))
}

#[derive(Debug)]
pub enum IncrementBySpecError {
    SpecParseFailure(String),
    VehicleNumberParseFailure(String, ParseIntError),
    FixedCouplingCombo(VehicleNumber),
    DatabaseQuery(String, VehicleNumber, Option<String>, tokio_postgres::Error),
    DatabaseBeginTransaction(tokio_postgres::Error),
    DatabaseCommitTransaction(tokio_postgres::Error),
}
impl fmt::Display for IncrementBySpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpecParseFailure(spec) => write!(f, "failed to parse spec {:?}", spec),
            Self::VehicleNumberParseFailure(num_str, e) => write!(f, "failed to parse vehicle number {:?}: {}", num_str, e),
            Self::FixedCouplingCombo(coupled_number) => write!(f, "vehicle number {} is part of a fixed coupling and cannot be ridden in combination with other vehicles", coupled_number),
            Self::DatabaseQuery(rider, vehicle_num, line_opt, e) => write!(f, "database query error registering {} riding on vehicle {} on line {:?}: {}", rider, vehicle_num, line_opt, e),
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
    rider_username: &str,
    timestamp: DateTime<Local>,
    rides_spec: &str,
    allow_fixed_coupling_combos: bool,
) -> Result<RideInfo, IncrementBySpecError> {
    let spec_no_spaces = rides_spec.replace(" ", "");
    let caps = match BIMRIDE_RE.captures(&spec_no_spaces) {
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
    let mut all_vehicle_nums: Vec<(VehicleNumber, bool)> = Vec::new();
    for &vehicle_num in &vehicle_nums {
        let mut added_fixed_coupling = false;
        if let Some(bim_database) = bim_database_opt {
            if let Some(veh) = bim_database.get(&vehicle_num) {
                for &fc in &veh.fixed_coupling {
                    all_vehicle_nums.push((fc, vehicle_num != fc));
                    added_fixed_coupling = true;
                }
            }
        }

        if !added_fixed_coupling {
            all_vehicle_nums.push((vehicle_num, false));
        }
    }

    let vehicle_ride_infos = {
        let xact = ride_conn.transaction().await
            .map_err(|e| IncrementBySpecError::DatabaseBeginTransaction(e))?;

        let mut vehicle_ride_infos = Vec::new();
        for &(vehicle_num, is_fixed_coupling) in &all_vehicle_nums {
            let (lri_opt, ori_opt) = increment_last_ride(&xact, company, vehicle_num, rider_username, timestamp, line_str_opt).await
                .map_err(|e| IncrementBySpecError::DatabaseQuery(rider_username.to_owned(), vehicle_num, line_str_opt.map(|l| l.to_owned()), e))?;
            vehicle_ride_infos.push(VehicleRideInfo {
                vehicle_number: vehicle_num,
                ridden_within_fixed_coupling: is_fixed_coupling,
                last_ride: lri_opt,
                last_ride_other_rider: ori_opt,
            });
        }

        xact.commit().await
            .map_err(|e| IncrementBySpecError::DatabaseCommitTransaction(e))?;

        vehicle_ride_infos
    };

    Ok(RideInfo {
        line: line_str_opt.map(|s| s.to_owned()),
        vehicles: vehicle_ride_infos,
    })
}
