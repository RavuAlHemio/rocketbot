use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::Infallible;

use askama::Template;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rocketbot_bim_common::{VehicleInfo, VehicleNumber};
use rocketbot_date_time::DateTimeLocalWithWeekday;
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeStruct;
use tracing::error;

use crate::{get_config, get_query_pairs, render_response, return_405, return_500};
use crate::bim::{
    connect_to_db, obtain_company_to_bim_database, obtain_company_to_definition,
};
use crate::templating::filters;


static PLACEHOLDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "\\{([oc]|[0-9]+)\\}"
).expect("failed to compile placeholder regex"));


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


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct RideInfo {
    pub rider: String,
    pub timestamp: DateTimeLocalWithWeekday,
    pub line: Option<String>,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct RideRow {
    company: String,
    vehicle_type_opt: Option<String>,
    vehicle_number: VehicleNumber,
    ride_count: i64,
    last_line: Option<String>,
}
impl RideRow {
    pub fn new(
        company: String,
        vehicle_type_opt: Option<String>,
        vehicle_number: VehicleNumber,
        ride_count: i64,
        last_line: Option<String>,
    ) -> Self {
        Self {
            company,
            vehicle_type_opt,
            vehicle_number,
            ride_count,
            last_line,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct OddsEndsTable {
    pub title: String,
    pub description: Option<String>,
    pub column_titles: Vec<String>,
    pub rows: Vec<Vec<OddsEndsCell>>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct OddsEndsCell {
    pub value: String,
    pub link: Option<String>,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimrides.html")]
struct BimRidesTemplate {
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimoddsends.html")]
struct BimOddsEndsTemplate {
    pub tables: Vec<OddsEndsTable>,
}


pub(crate) async fn handle_bim_rides(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let query_res = db_conn.query("
        SELECT r.company, rv.vehicle_number, rv.vehicle_type, CAST(COUNT(*) AS bigint), MAX(r.line)
        FROM bim.rides r
        INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
        WHERE rv.coupling_mode = 'R'
        GROUP BY r.company, rv.vehicle_number, rv.vehicle_type
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut rides: Vec<RideRow> = Vec::new();
    for row in rows {
        let company: String = row.get(0);
        let vehicle_number = VehicleNumber::from_string(row.get(1));
        let vehicle_type_opt: Option<String> = row.get(2);
        let ride_count: i64 = row.get(3);
        let last_line: Option<String> = row.get(4);
        // output 1:1
        rides.push(RideRow::new(company, vehicle_type_opt, vehicle_number, ride_count, last_line));
    }

    rides.sort_unstable_by(|left, right| {
        left.company.cmp(&right.company)
            .then_with(|| left.vehicle_number.cmp(&right.vehicle_number))
            .then_with(|| left.ride_count.cmp(&right.ride_count))
            .then_with(|| left.last_line.cmp(&right.last_line))
    });

    let template = BimRidesTemplate {
        rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_types(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let company_to_bim_database_opt = obtain_company_to_definition().await
        .as_ref()
        .and_then(|ctd| obtain_company_to_bim_database(ctd));
    let company_to_bim_database = match company_to_bim_database_opt {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };

    let query_res = db_conn.query("
        SELECT DISTINCT r.rider_username, r.company, rv.vehicle_number
        FROM bim.rides r
        INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
        WHERE rv.coupling_mode = 'R'
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
                let is_active =
                    bim_data.in_service_since.is_some()
                    && bim_data.out_of_service_since.is_none()
                ;

                let riders = vehicle_to_riders
                    .remove(bim_number)
                    .unwrap_or_else(|| BTreeSet::new());

                let type_stats = stats.type_to_stats
                    .entry(bim_data.type_code.clone())
                    .or_insert_with(|| TypeStats::new(bim_data.type_code.clone(), all_riders.iter()));

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


pub(crate) async fn handle_bim_vehicles(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let per_rider = query_pairs.get("per-rider").map(|pr| pr == "1").unwrap_or(false);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let company_to_bim_database_opt = obtain_company_to_definition().await
        .as_ref()
        .and_then(|ctd| obtain_company_to_bim_database(ctd));
    let company_to_bim_database_opts = match company_to_bim_database_opt {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, VehicleInfo>> = BTreeMap::new();
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
                type_code: Some(bim_value.type_code.clone()),
                manufacturer: bim_value.manufacturer.clone(),
                active_from: bim_value.in_service_since.clone(),
                active_to: bim_value.out_of_service_since.clone(),
                add_info: bim_value.other_data.clone(),
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


pub(crate) async fn handle_bim_odds_ends(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let config_guard = match get_config().await {
        Some(cg) => cg,
        None => return return_500(),
    };
    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut tables = Vec::with_capacity(config_guard.bim_odds_ends.len());
    for odd_end in &config_guard.bim_odds_ends {
        let title = odd_end.title.clone();
        let description = odd_end.description.clone();
        let column_titles = odd_end.column_titles.clone();
        let db_rows = match db_conn.query(&odd_end.query, &[]).await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to execute query {:?} (skipping table): {}", odd_end.query, e);
                continue;
            },
        };
        let mut rows = Vec::with_capacity(db_rows.len());
        for db_row in db_rows {
            let mut row = Vec::with_capacity(db_row.len());
            for i in 0..db_row.len() {
                let value: String = db_row.get(i);
                row.push(OddsEndsCell {
                    value,
                    link: None,
                });
            }

            // enrich with links
            for i in 0..row.len() {
                let link_format = odd_end.column_link_formats
                    .get(i)
                    .map(|lf| lf.as_str())
                    .unwrap_or("");
                if link_format.len() == 0 {
                    continue;
                }

                let link = PLACEHOLDER_REGEX.replace_all(link_format, |caps: &Captures| {
                    let placeholder_name = caps.get(1).expect("placeholder name not captured").as_str();
                    if placeholder_name == "o" {
                        // opening curly brace
                        "{"
                    } else if placeholder_name == "c" {
                        // closing curly brace
                        "}"
                    } else {
                        // column index
                        let column_index: usize = placeholder_name.parse()
                            .expect("placeholder index not parsable as usize");
                        row[column_index].value.as_str()
                    }
                });
                row[i].link = Some(link.into_owned());
            }

            rows.push(row);
        }
        tables.push(OddsEndsTable {
            title,
            description,
            column_titles,
            rows,
        });
    }

    let template = BimOddsEndsTemplate {
        tables,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
