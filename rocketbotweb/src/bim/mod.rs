pub(crate) mod achievements;
pub(crate) mod charts;
pub(crate) mod coverage;
pub(crate) mod details;
pub(crate) mod drilldown;
pub(crate) mod query;
pub(crate) mod tables;
pub(crate) mod top;


use std::collections::{BTreeMap, HashMap};
use std::fs::File;

use form_urlencoded;
use rocketbot_bim_common::{VehicleInfo, VehicleNumber};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

use crate::{connect_to_db, get_bot_config};


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct VehicleDatabaseExtract {
    pub company_to_vehicle_to_fixed_coupling: HashMap<String, HashMap<VehicleNumber, Vec<VehicleNumber>>>,
    pub company_to_vehicle_to_type: HashMap<String, HashMap<VehicleNumber, String>>,
}
impl VehicleDatabaseExtract {
    pub fn new(
        company_to_vehicle_to_fixed_coupling: HashMap<String, HashMap<VehicleNumber, Vec<VehicleNumber>>>,
        company_to_vehicle_to_type: HashMap<String, HashMap<VehicleNumber, String>>,
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


fn append_to_query(query_string: &mut String, key: &str, value: &str) {
    if query_string.len() > 0 {
        query_string.push('&');
    }
    for key_piece in form_urlencoded::byte_serialize(key.as_bytes()) {
        query_string.push_str(key_piece);
    }
    query_string.push('=');
    for value_piece in form_urlencoded::byte_serialize(value.as_bytes()) {
        query_string.push_str(value_piece);
    }
}


async fn obtain_bim_plugin_config() -> Option<serde_json::Value> {
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
        .filter(|p|
            p["enabled"].as_bool().unwrap_or(false)
            || p["web_enabled"].as_bool().unwrap_or(false)
        )
        .filter(|p| p["name"].as_str().map(|n| n == "bim").unwrap_or(false))
        .nth(0);
    let bim_plugin = match bim_plugin_opt {
        Some(bp) => bp,
        None => {
            warn!("no enabled bim plugin found in bot config");
            return None;
        },
    };
    Some(bim_plugin.clone())
}


async fn obtain_company_to_definition() -> Option<BTreeMap<String, serde_json::Value>> {
    let bim_plugin = obtain_bim_plugin_config().await?;

    let company_to_definition = match bim_plugin["config"]["company_to_definition"].as_object() {
        Some(ctd) => ctd,
        None => {
            warn!("no company_to_definition object found in bim plugin config");
            return None;
        },
    };
    let company_to_definition_set = company_to_definition
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Some(company_to_definition_set)
}


fn obtain_company_to_bim_database(company_to_definition: &BTreeMap<String, serde_json::Value>) -> Option<BTreeMap<String, Option<BTreeMap<VehicleNumber, VehicleInfo>>>> {
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
        let bim_database: Vec<VehicleInfo> = match ciborium::from_reader(file) {
            Ok(bd) => bd,
            Err(e) => {
                error!("failed to parse bim database file {:?}: {}", bim_database_path, e);
                continue;
            }
        };

        let mut bim_map: BTreeMap<VehicleNumber, VehicleInfo> = BTreeMap::new();
        for bim in bim_database {
            bim_map.insert(bim.number.clone(), bim.clone());
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

    let company_to_bim_database_opt = obtain_company_to_definition().await
        .as_ref()
        .and_then(|ctd| obtain_company_to_bim_database(ctd));
    let company_to_bim_database = match company_to_bim_database_opt {
        Some(ctbd) => ctbd,
        None => return VehicleDatabaseExtract::default(),
    };

    for (company, database_opt) in company_to_bim_database.iter() {
        let database = match database_opt {
            Some(db) => db,
            None => continue,
        };

        for (number, bim) in database {
            company_to_vehicle_to_type.entry(company.clone())
                .or_insert_with(|| HashMap::new())
                .insert(number.to_owned(), bim.type_code.to_owned());

            if bim.fixed_coupling.len() > 0 {
                let fixed_coupling_vns: Vec<VehicleNumber> = bim.fixed_coupling.iter()
                    .map(|fc| fc.clone())
                    .collect();
                company_to_vehicle_to_fixed_coupling.entry(company.clone())
                    .or_insert_with(|| HashMap::new())
                    .insert(number.to_owned(), fixed_coupling_vns);
            }
        }
    }

    VehicleDatabaseExtract::new(
        company_to_vehicle_to_fixed_coupling,
        company_to_vehicle_to_type,
    )
}
