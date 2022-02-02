use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::{Infallible, TryInto};
use std::fs::File;

use hyper::{Body, Method, Request, Response};
use log::error;
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeStruct;

use crate::{
    connect_to_db, get_bot_config, get_query_pairs, render_json, render_template, return_405,
    return_500,
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


async fn obtain_company_to_bim_database() -> Option<BTreeMap<String, Option<BTreeMap<u32, serde_json::Value>>>> {
    let bot_config = match get_bot_config().await {
        Some(bc) => bc,
        None => return None,
    };

    let plugins = match bot_config["plugins"].as_array() {
        Some(ps) => ps,
        None => return None,
    };
    let bim_plugin_opt = plugins.iter()
        .filter(|p| p["enabled"].as_bool().unwrap_or(false))
        .filter(|p| p["name"].as_str().map(|n| n == "bim").unwrap_or(false))
        .nth(0);
    let bim_plugin = match bim_plugin_opt {
        Some(bp) => bp,
        None => return None,
    };

    let company_to_definition = match bim_plugin["config"]["company_to_definition"].as_object() {
        Some(ctd) => ctd,
        None => return None,
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

    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let stats_json = ctx.into_json();
        match render_json(&stats_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        match render_template("bimtypes.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}
