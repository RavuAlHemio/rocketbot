use std::collections::{HashMap, HashSet};
use std::convert::{Infallible, TryInto};
use std::fs::File;

use hyper::{Body, Method, Request, Response};
use log::error;
use serde::{Deserialize, Serialize};

use crate::{
    connect_to_db, get_bot_config, get_query_pairs, render_json, render_template, return_405,
    return_500,
};


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct RideRow {
    company: String,
    vehicle_numbers: Vec<u32>,
    ride_count: i64,
    last_line: Option<String>,
}
impl RideRow {
    pub fn new(
        company: String,
        vehicle_numbers: Vec<u32>,
        ride_count: i64,
        last_line: Option<String>,
    ) -> Self {
        Self {
            company,
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


async fn assemble_fixed_couplings() -> HashMap<String, HashMap<u32, Vec<u32>>> {
    let mut ret = HashMap::new();

    let bot_config = match get_bot_config().await {
        Some(bc) => bc,
        None => return ret,
    };

    let plugins = match bot_config["plugins"].as_array() {
        Some(ps) => ps,
        None => return ret,
    };
    let bim_plugin_opt = plugins.iter()
        .filter(|p| p["enabled"].as_bool().unwrap_or(false))
        .filter(|p| p["name"].as_str().map(|n| n == "bim").unwrap_or(false))
        .nth(0);
    let bim_plugin = match bim_plugin_opt {
        Some(bp) => bp,
        None => return ret,
    };

    let company_to_bim_database_path = match bim_plugin["config"]["company_to_bim_database_path"].as_object() {
        Some(ctbdpo) => ctbdpo,
        None => return ret,
    };
    for (company, bim_database_path_object) in company_to_bim_database_path.iter() {
        if bim_database_path_object.is_null() {
            ret.insert(
                company.clone(),
                HashMap::new(),
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
        for bim in bim_array {
            let number_i64 = match bim["number"].as_i64() {
                Some(bn) => bn,
                None => {
                    error!("number in {} in bim database file {:?} not an i64", bim, bim_database_path);
                    continue;
                },
            };
            let number_u32: u32 = match number_i64.try_into() {
                Ok(n) => n,
                Err(_) => {
                    error!("number {} in bim database file {:?} not convertible to u32", number_i64, bim_database_path);
                    continue;
                },
            };
            let fixed_coupling = match bim["fixed_coupling"].as_array() {
                Some(fc) => fc,
                None => {
                    error!("fixed_coupling in {} in bim database file {:?} not an array", bim, bim_database_path);
                    continue;
                },
            };
            let mut fixed_coupling_u32s: Vec<u32> = Vec::new();
            for fixed_coupling_value in fixed_coupling {
                let fc_i64 = match fixed_coupling_value.as_i64() {
                    Some(n) => n,
                    None => {
                        error!("fixed coupling value {} in bim database file {:?} not an i64", fixed_coupling_value, bim_database_path);
                        continue;
                    },
                };
                let fc_u32: u32 = match fc_i64.try_into() {
                    Ok(n) => n,
                    Err(_) => {
                        error!("fixed coupling value {} in bim database file {:?} not a u32", fc_i64, bim_database_path);
                        continue;
                    },
                };
                fixed_coupling_u32s.push(fc_u32);
            }
            if fixed_coupling_u32s.len() > 0 {
                ret.entry(company.clone())
                    .or_insert_with(|| HashMap::new())
                    .insert(number_u32, fixed_coupling_u32s);
            }
        }
    }

    ret
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
    let company_to_vehicle_to_fixed_coupling = assemble_fixed_couplings()
        .await;

    let query_res = db_conn.query("
        SELECT lr.company, lr.vehicle_number, CAST(SUM(lr.ride_count) AS bigint), MAX(lr.last_line)
        FROM bim.last_rides lr
        GROUP BY lr.company, lr.vehicle_number
        ORDER BY lr.company, lr.vehicle_number
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
    for row in rows {
        let company: String = row.get(0);
        let vehicle_number_i64: i64 = row.get(1);
        let ride_count: i64 = row.get(2);
        let last_line: Option<String> = row.get(3);

        let vehicle_number_u32: u32 = vehicle_number_i64.try_into().unwrap();
        let fixed_coupling_opt = company_to_vehicle_to_fixed_coupling
            .get(&company)
            .map(|v2fc| v2fc.get(&vehicle_number_u32))
            .flatten();
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

            rides.push(RideRow::new(company, fixed_coupling.clone(), ride_count, last_line));
        } else {
            // not a fixed coupling; output 1:1
            rides.push(RideRow::new(company, vec![vehicle_number_u32], ride_count, last_line));
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
        ctx.insert("rides", &rides);
        match render_template("bimrides.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}
