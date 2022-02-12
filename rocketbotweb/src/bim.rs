use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::{Infallible, TryInto};
use std::fs::File;
use std::borrow::Cow;

use chrono::{Datelike, DateTime, Local, NaiveDate, Weekday, TimeZone};
use hyper::{Body, Method, Request, Response};
use log::{error, warn};
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeStruct;
use tokio_postgres::types::ToSql;

use crate::{
    connect_to_db, get_bot_config, get_query_pairs, render_json, render_template, return_400,
    return_405, return_500,
};


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct RideRow {
    company: String,
    vehicle_type_opt: Option<String>,
    vehicle_numbers: Vec<u32>,
    ride_count: i64,
    last_line: Option<String>,
}
impl RideRow {
    pub fn new(
        company: String,
        vehicle_type_opt: Option<String>,
        vehicle_numbers: Vec<u32>,
        ride_count: i64,
        last_line: Option<String>,
    ) -> Self {
        Self {
            company,
            vehicle_type_opt,
            vehicle_numbers,
            ride_count,
            last_line,
        }
    }

    pub fn sort_key(&self) -> (String, Vec<u32>, i64, Option<String>) {
        let mut sorted_vehicle_numbers = self.vehicle_numbers.clone();
        sorted_vehicle_numbers.sort_unstable();

        (
            self.company.clone(),
            sorted_vehicle_numbers,
            self.ride_count,
            self.last_line.clone(),
        )
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TypeStats {
    pub type_code: String,
    pub total_count: usize,
    pub active_count: usize,
    pub ridden_count: usize,
    pub rider_ridden_counts: BTreeMap<String, usize>,
}
impl TypeStats {
    pub fn new<R: Iterator<Item = S>, S: AsRef<str>>(type_code: String, rider_names: R) -> Self {
        let rider_ridden_counts = rider_names
            .map(|rn| (rn.as_ref().to_owned(), 0))
            .collect();
        Self {
            type_code,
            total_count: 0,
            active_count: 0,
            ridden_count: 0,
            rider_ridden_counts,
        }
    }
}
impl Serialize for TypeStats {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        assert!(self.total_count > 0);
        let mut state = serializer.serialize_struct("TypeStats", 10)?;
        state.serialize_field("type_code", &self.type_code)?;
        state.serialize_field("total_count", &self.total_count)?;
        state.serialize_field("active_count", &self.active_count)?;
        state.serialize_field("ridden_count", &self.ridden_count)?;
        state.serialize_field("rider_ridden_counts", &self.rider_ridden_counts)?;

        let active_per_total = (self.active_count as f64) / (self.total_count as f64);
        state.serialize_field("active_per_total", &active_per_total)?;

        let ridden_per_total = (self.ridden_count as f64) / (self.total_count as f64);
        state.serialize_field("ridden_per_total", &ridden_per_total)?;

        let ridden_per_active = if self.active_count > 0 {
            Some((self.ridden_count as f64) / (self.active_count as f64))
        } else {
            None
        };
        state.serialize_field("ridden_per_active", &ridden_per_active)?;

        let mut rider_ridden_per_total = BTreeMap::new();
        let mut rider_ridden_per_active = BTreeMap::new();
        for (rider, &rider_ridden_count) in &self.rider_ridden_counts {
            let rpt = (rider_ridden_count as f64) / (self.total_count as f64);
            rider_ridden_per_total.insert(rider.clone(), rpt);

            let rpa = if self.active_count > 0 {
                Some((rider_ridden_count as f64) / (self.active_count as f64))
            } else {
                None
            };
            rider_ridden_per_active.insert(rider.clone(), rpa);
        }
        state.serialize_field("rider_ridden_per_total", &rider_ridden_per_total)?;
        state.serialize_field("rider_ridden_per_active", &rider_ridden_per_active)?;

        state.end()
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CompanyTypeStats {
    pub company: String,
    pub type_to_stats: BTreeMap<String, TypeStats>,
    pub unknown_type_count: usize,
    pub rider_to_unknown_type_count: BTreeMap<String, usize>,
}
impl CompanyTypeStats {
    pub fn new<R: Iterator<Item = S>, S: AsRef<str>>(company: String, rider_names: R) -> Self {
        let rider_to_unknown_type_count = rider_names
            .map(|rn| (rn.as_ref().to_owned(), 0))
            .collect();
        Self {
            company,
            type_to_stats: BTreeMap::new(),
            unknown_type_count: 0,
            rider_to_unknown_type_count,
        }
    }
}


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct VehicleDatabaseExtract {
    pub company_to_vehicle_to_fixed_coupling: HashMap<String, HashMap<u32, Vec<u32>>>,
    pub company_to_vehicle_to_type: HashMap<String, HashMap<u32, String>>,
}
impl VehicleDatabaseExtract {
    pub fn new(
        company_to_vehicle_to_fixed_coupling: HashMap<String, HashMap<u32, Vec<u32>>>,
        company_to_vehicle_to_type: HashMap<String, HashMap<u32, String>>,
    ) -> Self {
        Self {
            company_to_vehicle_to_fixed_coupling,
            company_to_vehicle_to_type,
        }
    }
}
impl Default for VehicleDatabaseExtract {
    fn default() -> Self {
        Self {
            company_to_vehicle_to_fixed_coupling: HashMap::new(),
            company_to_vehicle_to_type: HashMap::new(),
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RideInfo {
    pub rider: String,
    pub timestamp: DateTime<Local>,
    pub line: Option<String>,
}
impl Serialize for RideInfo {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let weekday = match self.timestamp.weekday() {
            Weekday::Mon => "Mo",
            Weekday::Tue => "Tu",
            Weekday::Wed => "We",
            Weekday::Thu => "Th",
            Weekday::Fri => "Fr",
            Weekday::Sat => "Sa",
            Weekday::Sun => "Su",
        };
        let timestamp = format!(
            "{} {}",
            weekday, self.timestamp.format("%Y-%m-%d %H:%M:%S"),
        );

        let mut state = serializer.serialize_struct("RideInfo", 3)?;
        state.serialize_field("rider", &self.rider)?;
        state.serialize_field("timestamp", &timestamp)?;
        state.serialize_field("line", &self.line)?;
        state.end()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct VehicleProfile {
    pub type_code: Option<String>,
    pub manufacturer: Option<String>,
    pub active_from: Option<String>,
    pub active_to: Option<String>,
    pub add_info: BTreeMap<String, String>,
    pub ride_count: usize,
    pub rider_to_ride_count: BTreeMap<String, usize>,
    pub first_ride: Option<RideInfo>,
    pub latest_ride: Option<RideInfo>,
}


#[inline]
fn cow_empty_to_none<'a, 'b>(val: Option<&'a Cow<'b, str>>) -> Option<&'a Cow<'b, str>> {
    match val {
        None => None,
        Some(x) => if x.len() > 0 { Some(x) } else { None },
    }
}


async fn render(template: &str, query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>, ctx: tera::Context) -> Result<Response<Body>, Infallible> {
    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let ctx_json = ctx.into_json();
        match render_json(&ctx_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        match render_template(template, &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}


async fn obtain_company_to_bim_database() -> Option<BTreeMap<String, Option<BTreeMap<u32, serde_json::Value>>>> {
    let bot_config = match get_bot_config().await {
        Some(bc) => bc,
        None => return None,
    };

    let plugins = match bot_config["plugins"].as_array() {
        Some(ps) => ps,
        None => {
            warn!("failed to read plugins array from bot config");
            return None;
        },
    };
    let bim_plugin_opt = plugins.iter()
        .filter(|p| p["enabled"].as_bool().unwrap_or(false))
        .filter(|p| p["name"].as_str().map(|n| n == "bim").unwrap_or(false))
        .nth(0);
    let bim_plugin = match bim_plugin_opt {
        Some(bp) => bp,
        None => {
            warn!("no enabled bim plugin found in bot config");
            return None;
        },
    };

    let company_to_definition = match bim_plugin["config"]["company_to_definition"].as_object() {
        Some(ctd) => ctd,
        None => {
            warn!("no company_to_definition value found in bot config");
            return None;
        },
    };
    let mut company_to_database = BTreeMap::new();
    for (company, definition) in company_to_definition.iter() {
        let bim_database_path_object = &definition["bim_database_path"];
        if bim_database_path_object.is_null() {
            company_to_database.insert(
                company.clone(),
                None,
            );
            continue;
        }

        let bim_database_path = match bim_database_path_object.as_str() {
            Some(bdp) => bdp,
            None => continue,
        };
        let file = match File::open(bim_database_path) {
            Ok(f) => f,
            Err(e) => {
                error!("failed to open bim database file {:?}: {}", bim_database_path, e);
                continue;
            },
        };
        let bim_database: serde_json::Value = match serde_json::from_reader(file) {
            Ok(bd) => bd,
            Err(e) => {
                error!("failed to parse bim database file {:?}: {}", bim_database_path, e);
                continue;
            }
        };
        let bim_array = match bim_database.as_array() {
            Some(ba) => ba,
            None => {
                error!("bim database file {:?} not a list", bim_database_path);
                continue;
            },
        };

        let mut bim_map: BTreeMap<u32, serde_json::Value> = BTreeMap::new();
        for bim in bim_array {
            let number_i64 = match bim["number"].as_i64() {
                Some(bn) => bn,
                None => {
                    error!("number in {} in {:?} bim database file {:?} not an i64", bim, company, bim_database_path);
                    continue;
                },
            };
            let number_u32: u32 = match number_i64.try_into() {
                Ok(n) => n,
                Err(_) => {
                    error!("number {} in {:?} bim database file {:?} not convertible to u32", number_i64, company, bim_database_path);
                    continue;
                },
            };
            bim_map.insert(number_u32, bim.clone());
        }
        company_to_database.insert(
            company.clone(),
            Some(bim_map),
        );
    }

    Some(company_to_database)
}


async fn obtain_vehicle_extract() -> VehicleDatabaseExtract {
    let mut company_to_vehicle_to_fixed_coupling = HashMap::new();
    let mut company_to_vehicle_to_type = HashMap::new();

    let company_to_bim_database = match obtain_company_to_bim_database().await {
        Some(ctbd) => ctbd,
        None => return VehicleDatabaseExtract::default(),
    };

    for (company, database_opt) in company_to_bim_database.iter() {
        let database = match database_opt {
            Some(db) => db,
            None => continue,
        };

        for (&number, bim) in database {
            if let Some(type_code) = bim["type_code"].as_str() {
                company_to_vehicle_to_type.entry(company.clone())
                    .or_insert_with(|| HashMap::new())
                    .insert(number, type_code.to_owned());
            };

            let fixed_coupling = match bim["fixed_coupling"].as_array() {
                Some(fc) => fc,
                None => {
                    error!("fixed_coupling in {} in {:?} bim database file not an array", bim, company);
                    continue;
                },
            };
            let mut fixed_coupling_u32s: Vec<u32> = Vec::new();
            for fixed_coupling_value in fixed_coupling {
                let fc_i64 = match fixed_coupling_value.as_i64() {
                    Some(n) => n,
                    None => {
                        error!("fixed coupling value {} in {:?} bim database file not an i64", fixed_coupling_value, company);
                        continue;
                    },
                };
                let fc_u32: u32 = match fc_i64.try_into() {
                    Ok(n) => n,
                    Err(_) => {
                        error!("fixed coupling value {} in {:?} bim database file not a u32", fc_i64, company);
                        continue;
                    },
                };
                fixed_coupling_u32s.push(fc_u32);
            }
            if fixed_coupling_u32s.len() > 0 {
                company_to_vehicle_to_fixed_coupling.entry(company.clone())
                    .or_insert_with(|| HashMap::new())
                    .insert(number, fixed_coupling_u32s);
            }
        }
    }

    VehicleDatabaseExtract::new(
        company_to_vehicle_to_fixed_coupling,
        company_to_vehicle_to_type,
    )
}


pub(crate) async fn handle_bim_rides(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let vehicle_extract = obtain_vehicle_extract()
        .await;

    let query_res = db_conn.query("
        SELECT r.company, rv.vehicle_number, CAST(COUNT(*) AS bigint), MAX(r.line)
        FROM bim.rides r
        INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
        WHERE rv.fixed_coupling_position = 0
        GROUP BY r.company, rv.vehicle_number
        ORDER BY r.company, rv.vehicle_number
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut company_to_known_fixed_couplings: HashMap<String, HashSet<Vec<u32>>> = HashMap::new();
    let mut rides: Vec<RideRow> = Vec::new();
    let mut has_any_vehicle_type = false;
    for row in rows {
        let company: String = row.get(0);
        let vehicle_number_i64: i64 = row.get(1);
        let ride_count: i64 = row.get(2);
        let last_line: Option<String> = row.get(3);

        let vehicle_number_u32: u32 = vehicle_number_i64.try_into().unwrap();

        let vehicle_type_opt = vehicle_extract
            .company_to_vehicle_to_type
            .get(&company)
            .and_then(|v2t| v2t.get(&vehicle_number_u32))
            .map(|t| t.clone());
        if !has_any_vehicle_type && vehicle_type_opt.is_some() {
            has_any_vehicle_type = true;
        }

        let fixed_coupling_opt = vehicle_extract
            .company_to_vehicle_to_fixed_coupling
            .get(&company)
            .and_then(|v2fc| v2fc.get(&vehicle_number_u32));
        if let Some(fixed_coupling) = fixed_coupling_opt {
            let known_fixed_couplings = company_to_known_fixed_couplings
                .entry(company.clone())
                .or_insert_with(|| HashSet::new());
            if known_fixed_couplings.contains(fixed_coupling) {
                // we've already output this one
                continue;
            }

            // remember this coupling
            known_fixed_couplings.insert(fixed_coupling.clone());

            rides.push(RideRow::new(company, vehicle_type_opt, fixed_coupling.clone(), ride_count, last_line));
        } else {
            // not a fixed coupling; output 1:1
            rides.push(RideRow::new(company, vehicle_type_opt, vec![vehicle_number_u32], ride_count, last_line));
        }
    }

    rides.sort_unstable_by_key(|entry| entry.sort_key());

    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let rides_json = serde_json::to_value(rides)
            .expect("failed to JSON-serialize rides");
        match render_json(&rides_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        let mut ctx = tera::Context::new();
        ctx.insert("has_any_vehicle_type", &has_any_vehicle_type);
        ctx.insert("rides", &rides);
        match render_template("bimrides.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}


pub(crate) async fn handle_bim_types(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let company_to_bim_database = match obtain_company_to_bim_database().await {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };

    let query_res = db_conn.query("
        SELECT DISTINCT r.rider_username, r.company, rv.vehicle_number
        FROM bim.rides r
        INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut company_to_vehicle_to_riders: HashMap<String, HashMap<u32, BTreeSet<String>>> = HashMap::new();
    let mut all_riders: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        let rider_username: String = row.get(0);
        let company: String = row.get(1);
        let vehicle_number_i64: i64 = row.get(2);

        let vehicle_number_u32: u32 = vehicle_number_i64.try_into().unwrap();

        company_to_vehicle_to_riders
            .entry(company)
            .or_insert_with(|| HashMap::new())
            .entry(vehicle_number_u32)
            .or_insert_with(|| BTreeSet::new())
            .insert(rider_username.clone());
        all_riders.insert(rider_username);
    }

    let mut company_to_stats: BTreeMap<String, CompanyTypeStats> = BTreeMap::new();
    for (company, bims_opt) in &company_to_bim_database {
        let mut stats = CompanyTypeStats::new(company.clone(), all_riders.iter());

        let mut no_riders = HashMap::new();
        let vehicle_to_riders = company_to_vehicle_to_riders
            .get_mut(company)
            .unwrap_or(&mut no_riders);

        if let Some(bims) = bims_opt {
            for (&bim_number, bim_data) in bims {
                let type_code = match bim_data["type_code"].as_str() {
                    Some(tc) => tc,
                    None => continue,
                };
                let is_active =
                    !bim_data["in_service_since"].is_null()
                    && bim_data["out_of_service_since"].is_null()
                ;

                let riders = vehicle_to_riders
                    .remove(&bim_number)
                    .unwrap_or_else(|| BTreeSet::new());

                let type_stats = stats.type_to_stats
                    .entry(type_code.to_owned())
                    .or_insert_with(|| TypeStats::new(type_code.to_owned(), all_riders.iter()));

                type_stats.total_count += 1;
                if is_active {
                    type_stats.active_count += 1;
                }
                if riders.len() > 0 {
                    type_stats.ridden_count += 1;
                }
                for rider in &riders {
                    *type_stats.rider_ridden_counts.get_mut(rider).unwrap() += 1;
                }
            }
        }

        // we have been removing from company_and_vehicle_to_riders
        // whatever is left has an unknown type
        for riders in vehicle_to_riders.values() {
            stats.unknown_type_count += 1;

            for rider in riders {
                let rut_count = stats.rider_to_unknown_type_count
                    .get_mut(rider)
                    .unwrap();
                *rut_count += 1;
            }
        }

        company_to_stats.insert(company.clone(), stats);
    }

    let mut ctx = tera::Context::new();
    ctx.insert("company_to_stats", &company_to_stats);
    ctx.insert("all_riders", &all_riders);

    render("bimtypes.html.tera", &query_pairs, ctx).await
}


pub(crate) async fn handle_bim_vehicles(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);
    let per_rider = query_pairs.get("per-rider").map(|pr| pr == "1").unwrap_or(false);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<u32, serde_json::Value>> = BTreeMap::new();
    for (company, bim_database_opt) in company_to_bim_database_opts.into_iter() {
        company_to_bim_database.insert(company, bim_database_opt.unwrap_or_else(|| BTreeMap::new()));
    }

    let mut company_to_vehicle_to_ride_info: BTreeMap<String, BTreeMap<u32, (i64, BTreeMap<String, i64>, RideInfo, RideInfo)>> = BTreeMap::new();

    let vehicles_res = db_conn.query(
        "
            WITH vehicle_ride_counts(company, vehicle_number, ride_count) AS (
                SELECT fravc.company, fravc.vehicle_number, COUNT(*)
                FROM bim.rides_and_vehicles fravc
                GROUP BY fravc.company, fravc.vehicle_number
            )
            SELECT
                vrc.company, vrc.vehicle_number, CAST(vrc.ride_count AS bigint),
                frav.rider_username, frav.\"timestamp\", frav.line,
                lrav.rider_username, lrav.\"timestamp\", lrav.line
            FROM vehicle_ride_counts vrc
            INNER JOIN bim.rides_and_vehicles frav
                ON frav.company = vrc.company
                AND frav.vehicle_number = vrc.vehicle_number
                AND NOT EXISTS (
                    SELECT 1
                    FROM bim.rides_and_vehicles frav2
                    WHERE
                        frav2.company = frav.company
                        AND frav2.vehicle_number = frav.vehicle_number
                        AND frav2.\"timestamp\" < frav.\"timestamp\"
                )
            INNER JOIN bim.rides_and_vehicles lrav
                ON lrav.company = vrc.company
                AND lrav.vehicle_number = vrc.vehicle_number
                AND NOT EXISTS (
                    SELECT 1
                    FROM bim.rides_and_vehicles lrav2
                    WHERE
                        lrav2.company = lrav.company
                        AND lrav2.vehicle_number = lrav.vehicle_number
                        AND lrav2.\"timestamp\" > lrav.\"timestamp\"
                )
        ",
        &[],
    ).await;
    let vehicle_rows = match vehicles_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query vehicles: {}", e);
            return return_500();
        },
    };
    for vehicle_row in vehicle_rows {
        let company: String = vehicle_row.get(0);
        let vehicle_number_i64: i64 = vehicle_row.get(1);
        let vehicle_number_u32: u32 = match vehicle_number_i64.try_into() {
            Ok(vn) => vn,
            Err(_) => {
                error!("failed to convert vehicle number {} to u32", vehicle_number_i64);
                continue;
            },
        };
        let ride_count: i64 = vehicle_row.get(2);
        let first_ride = RideInfo {
            rider: vehicle_row.get(3),
            timestamp: vehicle_row.get(4),
            line: vehicle_row.get(5),
        };
        let latest_ride = RideInfo {
            rider: vehicle_row.get(6),
            timestamp: vehicle_row.get(7),
            line: vehicle_row.get(8),
        };

        company_to_vehicle_to_ride_info
            .entry(company)
            .or_insert_with(|| BTreeMap::new())
            .insert(vehicle_number_u32, (ride_count, BTreeMap::new(), first_ride, latest_ride));
    }

    let rider_vehicles_res = db_conn.query(
        "
            SELECT rav.company, rav.vehicle_number, rav.rider_username, CAST(COUNT(*) AS bigint)
            FROM bim.rides_and_vehicles rav
            GROUP BY rav.company, rav.vehicle_number, rav.rider_username
        ",
        &[],
    ).await;
    let rider_vehicle_rows = match rider_vehicles_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query vehicles and riders: {}", e);
            return return_500();
        },
    };
    let mut all_riders: BTreeSet<String> = BTreeSet::new();
    for rider_vehicle_row in rider_vehicle_rows {
        let company: String = rider_vehicle_row.get(0);
        let vehicle_number_i64: i64 = rider_vehicle_row.get(1);
        let vehicle_number_u32: u32 = match vehicle_number_i64.try_into() {
            Ok(vn) => vn,
            Err(_) => {
                error!("failed to convert vehicle number {} to u32", vehicle_number_i64);
                continue;
            },
        };
        let rider_username: String = rider_vehicle_row.get(2);
        let ride_count: i64 = rider_vehicle_row.get(3);

        all_riders.insert(rider_username.clone());
        company_to_vehicle_to_ride_info
            .get_mut(&company).expect("company not found")
            .get_mut(&vehicle_number_u32).expect("vehicle not found")
            .1
            .insert(rider_username, ride_count);
    }

    let mut company_to_vehicle_to_profile: BTreeMap<String, BTreeMap<u32, VehicleProfile>> = BTreeMap::new();
    for (company, bim_database) in &company_to_bim_database {
        let vehicle_to_profile = company_to_vehicle_to_profile
            .entry(company.clone())
            .or_insert_with(|| BTreeMap::new());

        for (&vn, bim_value) in bim_database {
            let type_code = bim_value["type_code"].as_str().map(|s| s.to_owned());
            let active_from = bim_value["in_service_since"].as_str().map(|s| s.to_owned());
            let active_to = bim_value["out_of_service_since"].as_str().map(|s| s.to_owned());
            let manufacturer = bim_value["manufacturer"].as_str().map(|s| s.to_owned());

            let mut add_info = BTreeMap::new();
            if let Some(od) = bim_value["other_data"].as_object() {
                for (k, v) in od {
                    if let Some(v_str) = v.as_str() {
                        add_info.insert(k.clone(), v_str.to_owned());
                    }
                }
            }

            let (ride_count, rider_to_ride_count, first_ride_opt, latest_ride_opt) = company_to_vehicle_to_ride_info
                .get(company)
                .map(|vtri| vtri.get(&vn))
                .flatten()
                .map(|(rc, r2rc, fr, lr)| {
                    let r2rc_usize: BTreeMap<String, usize> = r2rc.iter()
                        .map(|(r, rrc)| (r.clone(), *rrc as usize))
                        .collect();
                    (*rc as usize, r2rc_usize, Some(fr.clone()), Some(lr.clone()))
                })
                .unwrap_or((0, BTreeMap::new(), None, None));

            let profile = VehicleProfile {
                type_code,
                manufacturer,
                active_from,
                active_to,
                add_info,
                ride_count,
                rider_to_ride_count,
                first_ride: first_ride_opt,
                latest_ride: latest_ride_opt,
            };
            vehicle_to_profile.insert(vn, profile);
        }

        // add those that are missing in the bim database
        if let Some(vtri) = company_to_vehicle_to_ride_info.get(company) {
            for (&vn, (ride_count, rider_to_ride_count, first_ride, last_ride)) in vtri {
                let rtrc_usize = rider_to_ride_count.iter()
                    .map(|(r, rrc)| (r.clone(), *rrc as usize))
                    .collect();
                vehicle_to_profile
                    .entry(vn)
                    .or_insert_with(|| VehicleProfile {
                        type_code: None,
                        manufacturer: None,
                        active_from: None,
                        active_to: None,
                        add_info: BTreeMap::new(),
                        ride_count: *ride_count as usize,
                        rider_to_ride_count: rtrc_usize,
                        first_ride: Some(first_ride.clone()),
                        latest_ride: Some(last_ride.clone()),
                    });

                // don't do anything if the entry already exists
            }
        }
    }

    let mut ctx = tera::Context::new();
    ctx.insert("all_riders", &all_riders);
    ctx.insert("company_to_vehicle_to_profile", &company_to_vehicle_to_profile);
    ctx.insert("per_rider", &per_rider);

    render("bimvehicles.html.tera", &query_pairs, ctx).await
}

pub(crate) async fn handle_bim_coverage(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let merge_types = query_pairs.get("merge-types")
        .map(|qp| qp == "1")
        .unwrap_or(false);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let (template, ctx) = if let Some(rider_name) = query_pairs.get("rider") {
        let mut conditions: Vec<String> = Vec::with_capacity(2);
        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(2);

        if rider_name != "!ALL" {
            conditions.push(format!("rider_username = ${}", conditions.len() + 1));
            params.push(&rider_name);
        }

        let local_timestamp: DateTime<Local>;
        if let Some(date_str) = cow_empty_to_none(query_pairs.get("to-date")) {
            let input_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => return return_400("invalid date format, expected yyyy-mm-dd").await,
            };

            // end of that day is actually next day at 04:00
            let naive_timestamp = input_date.succ().and_hms(4, 0, 0);
            local_timestamp = match Local.from_local_datetime(&naive_timestamp).earliest() {
                Some(lts) => lts,
                None => return return_400("failed to convert timestamp to local time").await,
            };

            conditions.push(format!("\"timestamp\" <= ${}", conditions.len() + 1));
            params.push(&local_timestamp);
        }

        let conditions_string = if conditions.len() > 0 {
            let mut conds_string = conditions.join(" AND ");
            conds_string.insert_str(0, "WHERE ");
            conds_string
        } else {
            String::new()
        };

        let query = format!(
            "
                SELECT company, vehicle_number, CAST(COUNT(*) AS bigint)
                FROM bim.rides_and_vehicles
                {}
                GROUP BY company, vehicle_number
            ",
            conditions_string,
        );

        // get ridden vehicles for rider
        let vehicles_res = db_conn.query(&query, &params).await;
        let vehicle_rows = match vehicles_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query vehicles: {}", e);
                return return_500();
            },
        };
        let mut company_to_vehicles_ridden: HashMap<String, HashMap<u32, i64>> = HashMap::new();
        let mut max_ride_count: i64 = 0;
        for vehicle_row in vehicle_rows {
            let company: String = vehicle_row.get(0);
            let vehicle_number_i64: i64 = vehicle_row.get(1);
            let vehicle_number_u32: u32 = match vehicle_number_i64.try_into() {
                Ok(vn) => vn,
                Err(_) => {
                    error!("failed to convert vehicle number {} to u32", vehicle_number_i64);
                    continue;
                },
            };
            let ride_count: i64 = vehicle_row.get(2);
            if max_ride_count < ride_count {
                max_ride_count = ride_count;
            }
            company_to_vehicles_ridden
                .entry(company)
                .or_insert_with(|| HashMap::new())
                .insert(vehicle_number_u32, ride_count);
        }

        // get vehicle database
        let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
            Some(ctbdb) => ctbdb,
            None => return return_500(),
        };
        let company_to_bim_database: BTreeMap<String, BTreeMap<u32, serde_json::Value>> = company_to_bim_database_opts.iter()
            .filter_map(|(comp, db_opt)| {
                if let Some(db) = db_opt.as_ref() {
                    Some((comp.clone(), db.clone()))
                } else {
                    None
                }
            })
            .collect();

        // run through vehicles
        let mut company_to_type_to_block_to_vehicles: BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<serde_json::Value>>>> = BTreeMap::new();
        let no_ridden_vehicles = HashMap::new();
        for (company, number_to_vehicle) in &company_to_bim_database {
            let ridden_vehicles = company_to_vehicles_ridden.get(company)
                .unwrap_or(&no_ridden_vehicles);

            let mut type_to_block_to_vehicles = BTreeMap::new();
            for (&number, vehicle) in number_to_vehicle {
                let full_number_string = number.to_string();
                let (block_str, number_str) = if full_number_string.len() >= 6 {
                    // assume first four digits are block
                    full_number_string.split_at(4)
                } else {
                    ("", full_number_string.as_str())
                };

                let type_code = match vehicle["type_code"].as_str() {
                    Some(tc) => tc.to_owned(),
                    None => continue,
                };
                let type_code_key = if merge_types {
                    String::new()
                } else {
                    type_code.clone()
                };

                // is the vehicle active?
                let from_known = vehicle["in_service_since"].is_string();
                let to_known = vehicle["out_of_service_since"].is_string();
                let is_active = from_known && !to_known;
                let ride_count = ridden_vehicles.get(&number).map(|c| *c).unwrap_or(0);

                let vehicle_data = serde_json::json!({
                    "block_str": block_str,
                    "number_str": number_str,
                    "type_code": type_code,
                    "full_number_str": full_number_string,
                    "is_active": is_active,
                    "ride_count": ride_count,
                });
                type_to_block_to_vehicles
                    .entry(type_code_key)
                    .or_insert_with(|| BTreeMap::new())
                    .entry(block_str.to_owned())
                    .or_insert_with(|| Vec::new())
                    .push(vehicle_data);
            }

            company_to_type_to_block_to_vehicles.insert(company.clone(), type_to_block_to_vehicles);
        }

        let mut ctx = tera::Context::new();
        ctx.insert("company_to_type_to_block_to_vehicles", &company_to_type_to_block_to_vehicles);
        ctx.insert("max_ride_count", &max_ride_count);

        ("bimcoverage.html.tera", ctx)
    } else {
        // list riders
        let riders_res = db_conn.query("SELECT DISTINCT rider_username FROM bim.rides", &[]).await;
        let rider_rows = match riders_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query riders: {}", e);
                return return_500();
            },
        };
        let mut riders: BTreeSet<String> = BTreeSet::new();
        for rider_row in rider_rows {
            let rider: String = rider_row.get(0);
            riders.insert(rider);
        }

        let mut ctx = tera::Context::new();
        ctx.insert("riders", &riders);

        ("bimcoverage-pickrider.html.tera", ctx)
    };

    render(template, &query_pairs, ctx).await
}

pub(crate) async fn handle_bim_detail(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let company = match query_pairs.get("company") {
        Some(c) => c.to_owned().into_owned(),
        None => return return_400("missing parameter \"company\"").await,
    };
    let vehicle_number_str = match query_pairs.get("vehicle") {
        Some(v) => v,
        None => return return_400("missing parameter \"vehicle\"").await,
    };
    let vehicle_number: u32 = match vehicle_number_str.parse() {
        Ok(vn) => vn,
        Err(_) => return return_400("invalid parameter value for \"vehicle\"").await,
    };

    let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<u32, serde_json::Value>> = BTreeMap::new();
    for (company, bim_database_opt) in company_to_bim_database_opts.into_iter() {
        company_to_bim_database.insert(company, bim_database_opt.unwrap_or_else(|| BTreeMap::new()));
    }

    let company_bim_database = match company_to_bim_database.get(&company) {
        Some(bd) => bd,
        None => return return_400("unknown company").await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let vehicle_json = company_bim_database.get(&vehicle_number)
        .map(|v| v.clone())
        .unwrap_or(serde_json::Value::Null);

    // query rides
    let vehicle_number_i64: i64 = vehicle_number.into();
    let ride_rows_res = db_conn.query(
        "
            SELECT
                rav.id, rav.rider_username, rav.\"timestamp\", rav.line, rav.spec_position,
                rav.as_part_of_fixed_coupling, rav.fixed_coupling_position
            FROM bim.rides_and_vehicles rav
            WHERE rav.company = $1
            AND rav.vehicle_number = $2
            ORDER BY rav.\"timestamp\" DESC, rav.id
        ",
        &[&company, &vehicle_number_i64],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut rides_json = Vec::new();
    for ride_row in ride_rows {
        let ride_id: i64 = ride_row.get(0);
        let rider_username: String = ride_row.get(1);
        let timestamp: DateTime<Local> = ride_row.get(2);
        let line: Option<String> = ride_row.get(3);
        let spec_position: i64 = ride_row.get(4);
        let as_part_of_fixed_coupling: bool = ride_row.get(5);
        let fixed_coupling_position: i64 = ride_row.get(6);

        rides_json.push(serde_json::json!({
            "id": ride_id,
            "rider_username": rider_username,
            "timestamp": timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            "line": line,
            "spec_position": spec_position,
            "as_part_of_fixed_coupling": as_part_of_fixed_coupling,
            "fixed_coupling_position": fixed_coupling_position,
        }));
    }

    let mut ctx = tera::Context::new();
    ctx.insert("vehicle", &vehicle_json);
    ctx.insert("rides", &rides_json);

    render("bimdetails.html.tera", &query_pairs, ctx).await
}
