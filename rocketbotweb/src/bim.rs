use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::Infallible;
use std::fs::File;
use std::str::FromStr;

use askama::Template;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use hyper::{Body, Method, Request, Response};
use indexmap::IndexSet;
use log::{error, warn};
use png;
use rocketbot_bim_achievements::{AchievementDef, ACHIEVEMENT_DEFINITIONS};
use rocketbot_date_time::DateTimeLocalWithWeekday;
use rocketbot_string::NatSortedString;
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeStruct;
use tokio_postgres::types::ToSql;

use crate::{
    connect_to_db, get_bot_config, get_query_pairs, render_response, return_400, return_405,
    return_500,
};
use crate::templating::filters;


type VehicleNumber = NatSortedString;


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct RideRow {
    company: String,
    vehicle_type_opt: Option<String>,
    vehicle_numbers: Vec<VehicleNumber>,
    ride_count: i64,
    last_line: Option<String>,
}
impl RideRow {
    pub fn new(
        company: String,
        vehicle_type_opt: Option<String>,
        vehicle_numbers: Vec<VehicleNumber>,
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

    pub fn active_per_total(&self) -> f64 { (self.active_count as f64) / (self.total_count as f64) }
    pub fn ridden_per_total(&self) -> f64 { (self.ridden_count as f64) / (self.total_count as f64) }
    pub fn ridden_per_active(&self) -> Option<f64> {
        if self.active_count > 0 {
            Some((self.ridden_count as f64) / (self.active_count as f64))
        } else {
            None
        }
    }
    pub fn rider_ridden_per_total(&self) -> BTreeMap<String, f64> {
        let mut ret = BTreeMap::new();
        for (rider, &rider_ridden_count) in &self.rider_ridden_counts {
            let rpt = (rider_ridden_count as f64) / (self.total_count as f64);
            ret.insert(rider.clone(), rpt);
        }
        ret
    }
    pub fn rider_ridden_per_active(&self) -> BTreeMap<String, Option<f64>> {
        let mut ret = BTreeMap::new();
        for (rider, &rider_ridden_count) in &self.rider_ridden_counts {
            let rpa = if self.active_count > 0 {
                Some((rider_ridden_count as f64) / (self.active_count as f64))
            } else {
                None
            };
            ret.insert(rider.clone(), rpa);
        }
        ret
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
        state.serialize_field("active_per_total", &self.active_per_total())?;
        state.serialize_field("ridden_per_total", &self.ridden_per_total())?;
        state.serialize_field("ridden_per_active", &self.ridden_per_active())?;
        state.serialize_field("rider_ridden_per_total", &self.rider_ridden_per_total())?;
        state.serialize_field("rider_ridden_per_active", &self.rider_ridden_per_active())?;
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


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct RideInfo {
    pub rider: String,
    pub timestamp: DateTimeLocalWithWeekday,
    pub line: Option<String>,
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
impl VehicleProfile {
    pub fn ride_count_text_for_rider(&self, rider: &str) -> String {
        if let Some(r) = self.rider_to_ride_count.get(rider) {
            r.to_string()
        } else {
            String::new()
        }
    }
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimrides.html")]
struct BimRidesTemplate {
    pub has_any_vehicle_type: bool,
    pub rides: Vec<RideRow>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimtypes.html")]
struct BimTypesTemplate {
    pub company_to_stats: BTreeMap<String, CompanyTypeStats>,
    pub all_riders: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimvehicles.html")]
struct BimVehicleTemplate {
    pub per_rider: bool,
    pub all_riders: BTreeSet<String>,
    pub company_to_vehicle_to_profile: BTreeMap<String, BTreeMap<VehicleNumber, VehicleProfile>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimcoverage.html")]
struct BimCoverageTemplate {
    pub max_ride_count: i64,
    pub company_to_type_to_block_to_vehicles: BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<CoverageVehiclePart>>>>,
    pub merge_types: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CoverageVehiclePart {
    pub block_str: String,
    pub number_str: String,
    pub type_code: String,
    pub full_number_str: String,
    pub is_active: bool,
    pub ride_count: i64,
}
impl CoverageVehiclePart {
    pub fn has_ride(&self) -> bool {
        self.ride_count > 0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimcoverage-pickrider.html")]
struct BimCoveragePickRiderTemplate {
    pub riders: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimdetails.html")]
struct BimDetailsTemplate {
    pub company: String,
    pub vehicle: Option<VehicleDetailsPart>,
    pub rides: Vec<BimDetailsRidePart>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimachievements.html")]
struct BimAchievementsTemplate {
    pub achievement_to_rider_to_timestamp: HashMap<i64, HashMap<String, DateTimeLocalWithWeekday>>,
    pub all_achievements: Vec<AchievementDef>,
    pub all_riders: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct VehicleDetailsPart {
    pub number: VehicleNumber,
    pub type_code: String,
    pub vehicle_class: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub other_data: BTreeMap<String, String>,
    pub fixed_coupling: IndexSet<VehicleNumber>,
}
impl VehicleDetailsPart {
    pub fn try_from_json(vehicle: &serde_json::Value) -> Option<Self> {
        let number: VehicleNumber = vehicle["number"].as_str()?.to_owned().into();
        let type_code = vehicle["type_code"].as_str()?.to_owned();
        let vehicle_class = vehicle["vehicle_class"].as_str()?.to_owned();
        let in_service_since = vehicle["in_service_since"]
            .as_str().map(|s| s.to_owned());
        let out_of_service_since = vehicle["out_of_service_since"]
            .as_str().map(|s| s.to_owned());
        let manufacturer = vehicle["manufacturer"]
            .as_str().map(|s| s.to_owned());

        let other_data_map = vehicle["other_data"]
            .as_object()?;
        let mut other_data = BTreeMap::new();
        for (key, val_val) in other_data_map {
            let val = val_val.as_str()?;
            other_data.insert(key.clone(), val.to_owned());
        }

        let fixed_coupling_array = vehicle["fixed_coupling"]
            .as_array()?;
        let mut fixed_coupling = IndexSet::new();
        for fc_value in fixed_coupling_array {
            let fc_number: VehicleNumber = fc_value.as_str()?.to_owned().into();
            fixed_coupling.insert(fc_number);
        }

        Some(Self {
            number,
            type_code,
            vehicle_class,
            in_service_since,
            out_of_service_since,
            manufacturer,
            other_data,
            fixed_coupling,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimlinedetails.html")]
struct BimLineDetailsTemplate {
    pub company: String,
    pub line: String,
    pub rides: Vec<BimDetailsRidePart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct BimDetailsRidePart {
    pub id: i64,
    pub rider_username: String,
    pub timestamp: String,
    pub line: Option<String>,
    pub vehicle_number: VehicleNumber,
    pub spec_position: i64,
    pub as_part_of_fixed_coupling: bool,
    pub fixed_coupling_position: i64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "widebims.html")]
struct WideBimsTemplate {
    pub rider_count: i64,
    pub rider_groups: Vec<RiderGroupPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct RiderGroupPart {
    pub riders: BTreeSet<String>,
    pub vehicles: BTreeSet<VehiclePart>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct VehiclePart {
    pub company: String,
    pub number: VehicleNumber,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "topbims.html")]
struct TopBimsTemplate {
    pub counts_vehicles: Vec<CountVehiclesPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CountVehiclesPart {
    pub ride_count: i64,
    pub vehicles: BTreeSet<VehiclePart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "topbimlines.html")]
struct TopBimLinesTemplate {
    pub counts_lines: Vec<CountLinesPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CountLinesPart {
    pub ride_count: i64,
    pub lines: BTreeSet<LinePart>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct LinePart {
    pub company: String,
    pub line: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimridebyid.html")]
struct BimRideByIdTemplate {
    pub id_param: String,
    pub ride_state: RideInfoState,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum RideInfoState {
    NotGiven,
    Invalid,
    NotFound,
    Found(RidePart),
}
impl Default for RideInfoState {
    fn default() -> Self { Self::NotGiven }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct RidePart {
    pub id: i64,
    pub rider_username: String,
    pub timestamp: String,
    pub company: String,
    pub line: Option<String>,
    pub vehicles: Vec<RideVehiclePart>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct RideVehiclePart {
    pub vehicle_number: VehicleNumber,
    pub vehicle_type: Option<String>,
    pub spec_position: i64,
    pub as_part_of_fixed_coupling: bool,
    pub fixed_coupling_position: i64,
}

#[inline]
fn cow_empty_to_none<'a, 'b>(val: Option<&'a Cow<'b, str>>) -> Option<&'a Cow<'b, str>> {
    match val {
        None => None,
        Some(x) => if x.len() > 0 { Some(x) } else { None },
    }
}


async fn obtain_company_to_bim_database() -> Option<BTreeMap<String, Option<BTreeMap<VehicleNumber, serde_json::Value>>>> {
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

        let mut bim_map: BTreeMap<VehicleNumber, serde_json::Value> = BTreeMap::new();
        for bim in bim_array {
            let number: VehicleNumber = match bim["number"].as_str() {
                Some(bn) => bn.to_owned().into(),
                None => {
                    error!("number in {} in {:?} bim database file {:?} not a string", bim, company, bim_database_path);
                    continue;
                },
            };
            bim_map.insert(number, bim.clone());
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

        for (number, bim) in database {
            if let Some(type_code) = bim["type_code"].as_str() {
                company_to_vehicle_to_type.entry(company.clone())
                    .or_insert_with(|| HashMap::new())
                    .insert(number.to_owned(), type_code.to_owned());
            };

            let fixed_coupling = match bim["fixed_coupling"].as_array() {
                Some(fc) => fc,
                None => {
                    error!("fixed_coupling in {} in {:?} bim database file not an array", bim, company);
                    continue;
                },
            };
            let mut fixed_coupling_vns: Vec<VehicleNumber> = Vec::new();
            for fixed_coupling_value in fixed_coupling {
                let fc: VehicleNumber = match fixed_coupling_value.as_str() {
                    Some(n) => n.to_owned().into(),
                    None => {
                        error!("fixed coupling value {} in {:?} bim database file not a string", fixed_coupling_value, company);
                        continue;
                    },
                };
                fixed_coupling_vns.push(fc);
            }
            if fixed_coupling_vns.len() > 0 {
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


pub(crate) async fn handle_bim_rides(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

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
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut company_to_known_fixed_couplings: HashMap<String, HashSet<Vec<VehicleNumber>>> = HashMap::new();
    let mut rides: Vec<RideRow> = Vec::new();
    let mut has_any_vehicle_type = false;
    for row in rows {
        let company: String = row.get(0);
        let vehicle_number = VehicleNumber::from_string(row.get(1));
        let ride_count: i64 = row.get(2);
        let last_line: Option<String> = row.get(3);

        let vehicle_type_opt = vehicle_extract
            .company_to_vehicle_to_type
            .get(&company)
            .and_then(|v2t| v2t.get(&vehicle_number))
            .map(|t| t.clone());
        if !has_any_vehicle_type && vehicle_type_opt.is_some() {
            has_any_vehicle_type = true;
        }

        let fixed_coupling_opt = vehicle_extract
            .company_to_vehicle_to_fixed_coupling
            .get(&company)
            .and_then(|v2fc| v2fc.get(&vehicle_number));
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
            rides.push(RideRow::new(company, vehicle_type_opt, vec![vehicle_number], ride_count, last_line));
        }
    }

    rides.sort_unstable_by(|left, right| {
        left.company.cmp(&right.company)
            .then_with(|| {
                let mut left_sorted_vehicle_numbers = left.vehicle_numbers.clone();
                let mut right_sorted_vehicle_numbers = right.vehicle_numbers.clone();
                left_sorted_vehicle_numbers.sort_unstable();
                right_sorted_vehicle_numbers.sort_unstable();
                left_sorted_vehicle_numbers.cmp(&right_sorted_vehicle_numbers)
            })
            .then_with(|| left.ride_count.cmp(&right.ride_count))
            .then_with(|| left.last_line.cmp(&right.last_line))
    });

    let template = BimRidesTemplate {
        has_any_vehicle_type,
        rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_types(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

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
    let mut company_to_vehicle_to_riders: HashMap<String, HashMap<VehicleNumber, BTreeSet<String>>> = HashMap::new();
    let mut all_riders: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        let rider_username: String = row.get(0);
        let company: String = row.get(1);
        let vehicle_number = VehicleNumber::from_string(row.get(2));

        company_to_vehicle_to_riders
            .entry(company)
            .or_insert_with(|| HashMap::new())
            .entry(vehicle_number)
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
            for (bim_number, bim_data) in bims {
                let type_code = match bim_data["type_code"].as_str() {
                    Some(tc) => tc,
                    None => continue,
                };
                let is_active =
                    !bim_data["in_service_since"].is_null()
                    && bim_data["out_of_service_since"].is_null()
                ;

                let riders = vehicle_to_riders
                    .remove(bim_number)
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

    let template = BimTypesTemplate {
        company_to_stats,
        all_riders,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_vehicles(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let per_rider = query_pairs.get("per-rider").map(|pr| pr == "1").unwrap_or(false);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, serde_json::Value>> = BTreeMap::new();
    for (company, bim_database_opt) in company_to_bim_database_opts.into_iter() {
        company_to_bim_database.insert(company, bim_database_opt.unwrap_or_else(|| BTreeMap::new()));
    }

    let mut company_to_vehicle_to_ride_info: BTreeMap<String, BTreeMap<VehicleNumber, (i64, BTreeMap<String, i64>, RideInfo, RideInfo)>> = BTreeMap::new();

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
        let vehicle_number = VehicleNumber::from_string(vehicle_row.get(1));
        let ride_count: i64 = vehicle_row.get(2);
        let first_ride = RideInfo {
            rider: vehicle_row.get(3),
            timestamp: DateTimeLocalWithWeekday(vehicle_row.get(4)),
            line: vehicle_row.get(5),
        };
        let latest_ride = RideInfo {
            rider: vehicle_row.get(6),
            timestamp: DateTimeLocalWithWeekday(vehicle_row.get(7)),
            line: vehicle_row.get(8),
        };

        company_to_vehicle_to_ride_info
            .entry(company)
            .or_insert_with(|| BTreeMap::new())
            .insert(vehicle_number, (ride_count, BTreeMap::new(), first_ride, latest_ride));
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
        let vehicle_number = VehicleNumber::from_string(rider_vehicle_row.get(1));
        let rider_username: String = rider_vehicle_row.get(2);
        let ride_count: i64 = rider_vehicle_row.get(3);

        all_riders.insert(rider_username.clone());
        company_to_vehicle_to_ride_info
            .get_mut(&company).expect("company not found")
            .get_mut(&vehicle_number).expect("vehicle not found")
            .1
            .insert(rider_username, ride_count);
    }

    let mut company_to_vehicle_to_profile: BTreeMap<String, BTreeMap<VehicleNumber, VehicleProfile>> = BTreeMap::new();
    for (company, bim_database) in &company_to_bim_database {
        let vehicle_to_profile = company_to_vehicle_to_profile
            .entry(company.clone())
            .or_insert_with(|| BTreeMap::new());

        for (vn, bim_value) in bim_database {
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
                .map(|vtri| vtri.get(vn))
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
            vehicle_to_profile.insert(vn.clone(), profile);
        }

        // add those that are missing in the bim database
        if let Some(vtri) = company_to_vehicle_to_ride_info.get(company) {
            for (vn, (ride_count, rider_to_ride_count, first_ride, last_ride)) in vtri {
                let rtrc_usize = rider_to_ride_count.iter()
                    .map(|(r, rrc)| (r.clone(), *rrc as usize))
                    .collect();
                vehicle_to_profile
                    .entry(vn.clone())
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

    let template = BimVehicleTemplate {
        per_rider,
        all_riders,
        company_to_vehicle_to_profile,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_coverage(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let merge_types = query_pairs.get("merge-types")
        .map(|qp| qp == "1")
        .unwrap_or(false);
    let hide_inactive = query_pairs.get("hide-inactive")
        .map(|qp| qp == "1")
        .unwrap_or(false);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    if let Some(rider_name) = query_pairs.get("rider") {
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
                Err(_) => return return_400("invalid date format, expected yyyy-mm-dd", &query_pairs).await,
            };

            // end of that day is actually next day at 04:00
            let naive_timestamp = input_date
                .succ_opt().unwrap()
                .and_hms_opt(4, 0, 0).unwrap();
            local_timestamp = match Local.from_local_datetime(&naive_timestamp).earliest() {
                Some(lts) => lts,
                None => return return_400("failed to convert timestamp to local time", &query_pairs).await,
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
        let mut company_to_vehicles_ridden: HashMap<String, HashMap<VehicleNumber, i64>> = HashMap::new();
        let mut max_ride_count: i64 = 0;
        for vehicle_row in vehicle_rows {
            let company: String = vehicle_row.get(0);
            let vehicle_number = VehicleNumber::from_string(vehicle_row.get(1));
            let ride_count: i64 = vehicle_row.get(2);
            if max_ride_count < ride_count {
                max_ride_count = ride_count;
            }
            company_to_vehicles_ridden
                .entry(company)
                .or_insert_with(|| HashMap::new())
                .insert(vehicle_number, ride_count);
        }

        // get vehicle database
        let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
            Some(ctbdb) => ctbdb,
            None => return return_500(),
        };
        let company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, serde_json::Value>> = company_to_bim_database_opts.iter()
            .filter_map(|(comp, db_opt)| {
                if let Some(db) = db_opt.as_ref() {
                    Some((comp.clone(), db.clone()))
                } else {
                    None
                }
            })
            .collect();

        // run through vehicles
        let mut company_to_type_to_block_to_vehicles: BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<CoverageVehiclePart>>>> = BTreeMap::new();
        let no_ridden_vehicles = HashMap::new();
        for (company, number_to_vehicle) in &company_to_bim_database {
            let ridden_vehicles = company_to_vehicles_ridden.get(company)
                .unwrap_or(&no_ridden_vehicles);

            let mut type_to_block_to_vehicles = BTreeMap::new();
            for (number, vehicle) in number_to_vehicle {
                let full_number_str = number.to_string();
                let (block_str, number_str) = if full_number_str.len() >= 6 {
                    // assume first four digits are block
                    full_number_str.split_at(4)
                } else {
                    ("", full_number_str.as_str())
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
                let ride_count = ridden_vehicles.get(number).map(|c| *c).unwrap_or(0);

                if hide_inactive && !is_active && ride_count == 0 {
                    continue;
                }

                let vehicle_data = CoverageVehiclePart {
                    block_str: block_str.to_owned(),
                    number_str: number_str.to_owned(),
                    type_code,
                    full_number_str: full_number_str.clone(),
                    is_active,
                    ride_count,
                };
                type_to_block_to_vehicles
                    .entry(type_code_key)
                    .or_insert_with(|| BTreeMap::new())
                    .entry(block_str.to_owned())
                    .or_insert_with(|| Vec::new())
                    .push(vehicle_data);
            }

            company_to_type_to_block_to_vehicles.insert(company.clone(), type_to_block_to_vehicles);
        }

        let template = BimCoverageTemplate {
            max_ride_count,
            company_to_type_to_block_to_vehicles,
            merge_types,
        };
        match render_response(&template, &query_pairs, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
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

        let template = BimCoveragePickRiderTemplate {
            riders,
        };
        match render_response(&template, &query_pairs, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}

pub(crate) async fn handle_bim_detail(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let company = match query_pairs.get("company") {
        Some(c) => c.to_owned().into_owned(),
        None => return return_400("missing parameter \"company\"", &query_pairs).await,
    };
    let vehicle_number_str = match query_pairs.get("vehicle") {
        Some(v) => v,
        None => return return_400("missing parameter \"vehicle\"", &query_pairs).await,
    };
    let vehicle_number: VehicleNumber = match vehicle_number_str.parse() {
        Ok(vn) => VehicleNumber::from_string(vn),
        Err(_) => return return_400("invalid parameter value for \"vehicle\"", &query_pairs).await,
    };

    let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, serde_json::Value>> = BTreeMap::new();
    for (company, bim_database_opt) in company_to_bim_database_opts.into_iter() {
        company_to_bim_database.insert(company, bim_database_opt.unwrap_or_else(|| BTreeMap::new()));
    }

    let company_bim_database = match company_to_bim_database.get(&company) {
        Some(bd) => bd,
        None => return return_400("unknown company", &query_pairs).await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let vehicle = company_bim_database.get(&vehicle_number)
        .map(|v| VehicleDetailsPart::try_from_json(v))
        .flatten();

    // query rides
    let ride_rows_res = db_conn.query(
        "
            SELECT
                rav.id, rav.rider_username, rav.\"timestamp\", rav.line, rav.vehicle_number,
                rav.spec_position, rav.as_part_of_fixed_coupling, rav.fixed_coupling_position
            FROM bim.rides_and_vehicles rav
            WHERE rav.company = $1
            AND rav.vehicle_number = $2
            ORDER BY rav.\"timestamp\" DESC, rav.id
        ",
        &[&company, &vehicle_number.as_str()],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut rides = Vec::new();
    for ride_row in ride_rows {
        let ride_id: i64 = ride_row.get(0);
        let rider_username: String = ride_row.get(1);
        let timestamp: DateTime<Local> = ride_row.get(2);
        let line: Option<String> = ride_row.get(3);
        let vehicle_number = VehicleNumber::from_string(ride_row.get(4));
        let spec_position: i64 = ride_row.get(5);
        let as_part_of_fixed_coupling: bool = ride_row.get(6);
        let fixed_coupling_position: i64 = ride_row.get(7);

        rides.push(BimDetailsRidePart {
            id: ride_id,
            rider_username,
            timestamp: timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            line,
            vehicle_number,
            spec_position,
            as_part_of_fixed_coupling,
            fixed_coupling_position,
        });
    }

    let template = BimDetailsTemplate {
        company,
        vehicle,
        rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_line_detail(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let company = match query_pairs.get("company") {
        Some(c) => c.to_owned().into_owned(),
        None => return return_400("missing parameter \"company\"", &query_pairs).await,
    };
    let line = match query_pairs.get("line") {
        Some(l) => l,
        None => return return_400("missing parameter \"line\"", &query_pairs).await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // query rides
    let ride_rows_res = db_conn.query(
        "
            SELECT
                rav.id, rav.rider_username, rav.\"timestamp\", rav.line, rav.vehicle_number,
                rav.spec_position, rav.as_part_of_fixed_coupling, rav.fixed_coupling_position
            FROM bim.rides_and_vehicles rav
            WHERE rav.company = $1
            AND rav.line = $2
            ORDER BY rav.\"timestamp\" DESC, rav.id, rav.spec_position, rav.fixed_coupling_position
        ",
        &[&company, &line],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut rides = Vec::new();
    for ride_row in ride_rows {
        let ride_id: i64 = ride_row.get(0);
        let rider_username: String = ride_row.get(1);
        let timestamp: DateTime<Local> = ride_row.get(2);
        let line: Option<String> = ride_row.get(3);
        let vehicle_number = VehicleNumber::from_string(ride_row.get(4));
        let spec_position: i64 = ride_row.get(5);
        let as_part_of_fixed_coupling: bool = ride_row.get(6);
        let fixed_coupling_position: i64 = ride_row.get(7);

        rides.push(BimDetailsRidePart {
            id: ride_id,
            rider_username,
            timestamp: timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            line,
            vehicle_number,
            spec_position,
            as_part_of_fixed_coupling,
            fixed_coupling_position,
        });
    }

    let template = BimLineDetailsTemplate {
        company,
        line: line.clone().into_owned(),
        rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_wide_bims(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let count_opt: Option<i64> = query_pairs.get("count")
        .map(|c_str| c_str.parse().ok())
        .flatten();

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let rider_count = if let Some(c) = count_opt {
        c
    } else {
        // query for most riders per vehicle
        let most_riders_row_opt_res = db_conn.query_opt(
            "
                WITH vehicle_and_distinct_rider_count(company, vehicle_number, rider_count) AS (
                    SELECT rav.company, rav.vehicle_number, COUNT(DISTINCT rav.rider_username)
                    FROM bim.rides_and_vehicles rav
                    WHERE rav.fixed_coupling_position = 0
                    GROUP BY rav.company, rav.vehicle_number
                )
                SELECT CAST(COALESCE(MAX(rider_count), 0) AS bigint) FROM vehicle_and_distinct_rider_count
            ",
            &[],
        ).await;
        match most_riders_row_opt_res {
            Ok(Some(r)) => r.get(0),
            Ok(None) => 0,
            Err(e) => {
                error!("error querying maximum distinct rider count: {}", e);
                return return_500();
            },
        }
    };

    // query rides
    let ride_rows_res = db_conn.query(
        "
            WITH vehicle_and_distinct_rider_count(company, vehicle_number, rider_count) AS (
                SELECT rav.company, rav.vehicle_number, COUNT(DISTINCT rav.rider_username)
                FROM bim.rides_and_vehicles rav
                WHERE rav.fixed_coupling_position = 0
                GROUP BY rav.company, rav.vehicle_number
            )
            SELECT DISTINCT rav.company, rav.vehicle_number, rav.rider_username rc
            FROM bim.rides_and_vehicles rav
            INNER JOIN vehicle_and_distinct_rider_count vadrc
                ON vadrc.company = rav.company
                AND vadrc.vehicle_number = rav.vehicle_number
            WHERE
                vadrc.rider_count = $1
        ",
        &[&rider_count],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut vehicle_to_riders: HashMap<(String, VehicleNumber), BTreeSet<String>> = HashMap::new();
    for ride_row in ride_rows {
        let company: String = ride_row.get(0);
        let vehicle_number = VehicleNumber::from_string(ride_row.get(1));
        let rider_username: String = ride_row.get(2);

        vehicle_to_riders
            .entry((company, vehicle_number))
            .or_insert_with(|| BTreeSet::new())
            .insert(rider_username);
    }

    let mut rider_groups_to_rides: BTreeMap<BTreeSet<String>, BTreeSet<VehiclePart>> = BTreeMap::new();
    for ((company, vehicle_number), riders) in vehicle_to_riders.drain() {
        rider_groups_to_rides
            .entry(riders)
            .or_insert_with(|| BTreeSet::new())
            .insert(VehiclePart {
                company,
                number: vehicle_number,
            });
    }

    let rider_groups: Vec<RiderGroupPart> = rider_groups_to_rides.iter()
        .map(|(riders, rides)| RiderGroupPart {
            riders: riders.clone(),
            vehicles: rides.clone(),
        })
        .collect();

    let template = WideBimsTemplate {
        rider_count,
        rider_groups,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_top_bims(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let top_count: i64 = query_pairs.get("count")
        .map(|c_str| c_str.parse().ok())
        .flatten()
        .filter(|tc| *tc > 0)
        .unwrap_or(10);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // query rides
    let ride_rows_res = db_conn.query(
        "
            WITH ride_counts(company, vehicle_number, ride_count) AS (
                SELECT rav.company, rav.vehicle_number, COUNT(*)
                FROM bim.rides_and_vehicles rav
                WHERE rav.fixed_coupling_position = 0
                GROUP BY rav.company, rav.vehicle_number
            ),
            top_ride_counts(ride_count) AS (
                SELECT DISTINCT ride_count
                FROM ride_counts
                ORDER BY ride_count DESC
                LIMIT $1
            )
            SELECT rc.company, rc.vehicle_number, CAST(rc.ride_count AS bigint)
            FROM ride_counts rc
            WHERE EXISTS (
                SELECT 1
                FROM top_ride_counts trc
                WHERE trc.ride_count = rc.ride_count
            )
        ",
        &[&top_count],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut count_to_vehicles: BTreeMap<i64, BTreeSet<(String, VehicleNumber)>> = BTreeMap::new();
    for ride_row in ride_rows {
        let company: String = ride_row.get(0);
        let vehicle_number = VehicleNumber::from_string(ride_row.get(1));
        let ride_count: i64 = ride_row.get(2);

        count_to_vehicles
            .entry(ride_count)
            .or_insert_with(|| BTreeSet::new())
            .insert((company, vehicle_number));
    }

    let counts_vehicles: Vec<CountVehiclesPart> = count_to_vehicles.iter()
        .rev()
        .map(|(count, vehicles)| {
            let vehicle_parts = vehicles.iter()
                .map(|(c, vn)| VehiclePart {
                    company: c.clone(),
                    number: vn.clone(),
                })
                .collect();
            CountVehiclesPart {
                ride_count: *count,
                vehicles: vehicle_parts,
            }
        })
        .collect();

    let template = TopBimsTemplate {
        counts_vehicles,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_coverage_field(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let rider_opt = query_pairs.get("rider");

    let company_opt = query_pairs.get("company");
    let company = match company_opt {
        Some(c) => c,
        _ => return return_400("GET parameter \"company\" is required", &query_pairs).await,
    };

    let company_to_bim_database_opts = match obtain_company_to_bim_database().await {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, serde_json::Value>> = BTreeMap::new();
    for (company, bim_database_opt) in company_to_bim_database_opts.into_iter() {
        if let Some(bd) = bim_database_opt {
            company_to_bim_database.insert(company, bd);
        }
    }

    let bim_database = match company_to_bim_database.get(company.as_ref()) {
        Some(bd) => bd,
        None => return return_400("company does not exist or does not have a vehicle database", &query_pairs).await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let vehicle_rows_res = if let Some(rider) = rider_opt {
        db_conn.query(
            "
                SELECT DISTINCT rav.vehicle_number
                FROM bim.rides_and_vehicles rav
                WHERE rav.company = $1
                AND rav.rider_username = $2
            ",
            &[&company, &rider],
        ).await
    } else {
        db_conn.query(
            "
                SELECT DISTINCT rav.vehicle_number
                FROM bim.rides_and_vehicles rav
                WHERE rav.company = $1
            ",
            &[&company],
        ).await
    };
    let vehicle_rows = match vehicle_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicles: {}", e);
            return return_500();
        },
    };

    let mut vehicles = HashSet::new();
    for vehicle_row in vehicle_rows {
        let vehicle_number = VehicleNumber::from_string(vehicle_row.get(0));
        vehicles.insert(vehicle_number);
    }

    let mut pixels = Vec::with_capacity(bim_database.len());
    for vehicle in bim_database.values() {
        let number = vehicle["number"].as_str().unwrap().to_owned().into();
        pixels.push(vehicles.contains(&number));
    }

    let image_side = (pixels.len() as f64).sqrt() as usize;
    let image_height = image_side;
    let width_correction = if pixels.len() % image_height as usize != 0 { 1 } else { 0 };
    let image_width = pixels.len() / image_height + width_correction;

    let scanline_width_correction = if image_width % 8 != 0 { 1 } else { 0 };
    let scanline_width = image_width / 8 + scanline_width_correction;

    let mut pixel_bytes = vec![0u8; scanline_width * image_height];
    for (i, pixel) in pixels.iter().enumerate() {
        if !*pixel {
            continue;
        }

        let row_index = i / image_width;
        let column_index = i % image_width;

        let column_byte_index = column_index / 8;
        let column_bit_index = 7 - (column_index % 8);

        let byte_index = row_index * scanline_width + column_byte_index;

        pixel_bytes[byte_index] |= 1 << column_bit_index;
    }

    // make a PNG!
    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut png = png::Encoder::new(&mut png_bytes, image_width as u32, image_height as u32);
        png.set_color(png::ColorType::Indexed);
        png.set_depth(png::BitDepth::One);
        png.set_palette(vec![
            0x00, 0x00, 0x00, // index 0: black (transparent)
            0x00, 0xFF, 0x00, // index 1: green
        ]);
        png.set_trns(vec![
            0x00, // index 0: transparent
            0xFF, // index 1: opaque
        ]);
        let mut writer = match png.write_header() {
            Ok(w) => w,
            Err(e) => {
                error!("error writing PNG header: {}", e);
                return return_500();
            },
        };
        if let Err(e) =  writer.write_image_data(&pixel_bytes) {
            error!("error writing PNG data: {}", e);
            return return_500();
        }
    }

    let body = Body::from(png_bytes);
    let resp_res = Response::builder()
        .header("Content-Type", "image/png")
        .body(body);
    match resp_res {
        Ok(resp) => Ok(resp),
        Err(e) => {
            error!("error generating PNG response: {}", e);
            return return_500();
        },
    }
}

pub(crate) async fn handle_top_bim_lines(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let top_count: i64 = query_pairs.get("count")
        .map(|c_str| c_str.parse().ok())
        .flatten()
        .filter(|tc| *tc > 0)
        .unwrap_or(10);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // query rides
    let ride_rows_res = db_conn.query(
        "
            WITH ride_counts(company, line, ride_count) AS (
                SELECT r.company, r.line, COUNT(*)
                FROM bim.rides r
                WHERE r.line IS NOT NULL
                GROUP BY r.company, r.line
            ),
            top_ride_counts(ride_count) AS (
                SELECT DISTINCT ride_count
                FROM ride_counts
                ORDER BY ride_count DESC
                LIMIT $1
            )
            SELECT rc.company, rc.line, CAST(rc.ride_count AS bigint)
            FROM ride_counts rc
            WHERE EXISTS (
                SELECT 1
                FROM top_ride_counts trc
                WHERE trc.ride_count = rc.ride_count
            )
        ",
        &[&top_count],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut count_to_lines: BTreeMap<i64, BTreeSet<(String, String)>> = BTreeMap::new();
    for ride_row in ride_rows {
        let company: String = ride_row.get(0);
        let line: String = ride_row.get(1);
        let ride_count: i64 = ride_row.get(2);

        count_to_lines
            .entry(ride_count)
            .or_insert_with(|| BTreeSet::new())
            .insert((company, line));
    }

    let counts_lines: Vec<CountLinesPart> = count_to_lines.iter()
        .rev()
        .map(|(count, vehicles)| {
            let line_parts: BTreeSet<LinePart> = vehicles.iter()
                .map(|(c, l)| LinePart {
                    company: c.clone(),
                    line: l.clone(),
                })
                .collect();
            CountLinesPart {
                ride_count: *count,
                lines: line_parts,
            }
        })
        .collect();

    let template = TopBimLinesTemplate {
        counts_lines,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_achievements(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // query achievements
    let ach_rows_res = db_conn.query(
        "
            SELECT ra.rider_username, ra.achievement_id, ra.achieved_on
            FROM
                bim.rider_achievements ra
            ORDER BY
                ra.rider_username, ra.achievement_id
        ",
        &[],
    ).await;
    let ach_rows = match ach_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let all_achievements = ACHIEVEMENT_DEFINITIONS.iter()
        .map(|ad| ad.clone())
        .collect();

    let mut all_riders = BTreeSet::new();
    let mut achievement_to_rider_to_timestamp = HashMap::new();
    for ach in ach_rows {
        let rider: String = ach.get(0);
        let achievement_id: i64 = ach.get(1);
        let achieved_on_odtl: Option<DateTime<Local>> = ach.get(2);

        let achieved_on = match achieved_on_odtl {
            Some(dtl) => DateTimeLocalWithWeekday(dtl),
            None => continue,
        };

        all_riders.insert(rider.clone());
        achievement_to_rider_to_timestamp
            .entry(achievement_id)
            .or_insert_with(|| HashMap::new())
            .insert(rider, achieved_on);
    }

    let template = BimAchievementsTemplate {
        achievement_to_rider_to_timestamp,
        all_riders,
        all_achievements,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_ride_by_id(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let ride_id_str = query_pairs.get("id");
    let ride_id = ride_id_str.map(|ris| i64::from_str(&ris).ok());
    let ride_state = match ride_id {
        None => {
            // no ride ID given
            RideInfoState::NotGiven
        },
        Some(None) => {
            // ride ID invalid
            RideInfoState::Invalid
        },
        Some(Some(rid)) => {
            let db_conn = match connect_to_db().await {
                Some(c) => c,
                None => return return_500(),
            };

            let ride_res = db_conn.query(
                "
                    SELECT
                        rav.company, rav.rider_username, rav.\"timestamp\", rav.line,
                        rav.vehicle_number, rav.vehicle_type, rav.spec_position,
                        rav.as_part_of_fixed_coupling, rav.fixed_coupling_position
                    FROM bim.rides_and_vehicles rav
                    WHERE
                        rav.id = $1
                    ORDER BY
                        rav.spec_position, rav.fixed_coupling_position
                ",
                &[&rid],
            ).await;
            let ride_rows = match ride_res {
                Ok(r) => r,
                Err(e) => {
                    error!("failed to query ride: {}", e);
                    return return_500();
                },
            };
            if ride_rows.len() == 0 {
                RideInfoState::NotFound
            } else {
                let mut company: Option<String> = None;
                let mut rider_username: Option<String> = None;
                let mut timestamp: Option<DateTime<Local>> = None;
                let mut line: Option<Option<String>> = None;
                let mut vehicles: Vec<RideVehiclePart> = Vec::new();

                for ride_row in ride_rows {
                    if company.is_none() {
                        company = Some(ride_row.get(0));
                    }
                    if rider_username.is_none() {
                        rider_username = Some(ride_row.get(1));
                    }
                    if timestamp.is_none() {
                        timestamp = Some(ride_row.get(2));
                    }
                    if line.is_none() {
                        line = Some(ride_row.get(3));
                    }

                    let vehicle_number = VehicleNumber::from_string(ride_row.get(4));
                    let vehicle_type: Option<String> = ride_row.get(5);
                    let spec_position: i64 = ride_row.get(6);
                    let as_part_of_fixed_coupling: bool = ride_row.get(7);
                    let fixed_coupling_position: i64 = ride_row.get(8);

                    let vehicle = RideVehiclePart {
                        vehicle_number,
                        vehicle_type,
                        spec_position,
                        as_part_of_fixed_coupling,
                        fixed_coupling_position,
                    };
                    vehicles.push(vehicle);
                }

                RideInfoState::Found(RidePart {
                    id: rid,
                    rider_username: rider_username.unwrap(),
                    timestamp: DateTimeLocalWithWeekday(timestamp.unwrap()).to_string(),
                    company: company.unwrap(),
                    line: line.unwrap(),
                    vehicles,
                })
            }
        },
    };

    let template = BimRideByIdTemplate {
        id_param: ride_id_str.map(|s| s.clone().into_owned()).unwrap_or_else(|| "".to_owned()),
        ride_state,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
