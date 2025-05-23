use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;

use askama::Template;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use rocketbot_bim_common::VehicleNumber;
use rocketbot_interface::clown::ClownExt;
use serde::Serialize;
use tokio_postgres::types::ToSql;
use tracing::{debug, error};

use crate::{
    get_query_pairs, get_query_pairs_multiset, render_response, return_400, return_405, return_500,
};
use crate::bim::{
    append_to_query, connect_to_db, obtain_bim_plugin_config, obtain_company_to_bim_database,
    obtain_company_to_definition,
};
use crate::templating::filters;
use crate::util::sort_as_text;


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct QueryFiltersPart {
    pub timestamp: Option<NaiveDate>,
    pub rider_usernames: Vec<String>,
    pub companies: Vec<String>,
    pub lines: Vec<String>,
    pub vehicle_numbers: Vec<String>,
    pub vehicle_types: Vec<String>,
    pub missing_vehicle_type: Option<bool>,
    pub vehicle_ridden_only: bool,
}
impl QueryFiltersPart {
    pub fn filtering_on_vehicle_type(&self) -> bool {
        self.vehicle_types.len() > 0 || self.missing_vehicle_type.is_some()
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct QueriedRidePart {
    pub id: i64,
    pub timestamp: DateTime<Local>,
    pub rider_username: String,
    pub company: String,
    pub line: Option<String>,
    pub vehicles: Vec<QueriedRideVehiclePart>,
}
impl QueriedRidePart {
    pub fn at_least_one_vehicle_has_type(&self) -> bool {
        self.vehicles
            .iter()
            .any(|veh| veh.vehicle_type.is_some())
    }

    pub fn at_least_one_vehicle_ridden(&self) -> bool {
        self.vehicles
            .iter()
            .any(|veh| veh.coupling_mode.is_some())
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct QueriedRideVehiclePart {
    pub vehicle_number: String,
    pub vehicle_type: Option<String>,
    pub spec_position: i64,
    pub coupling_mode: Option<char>,
    pub fixed_coupling_position: i64,
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimquery.html")]
struct QueryTemplate {
    pub filters: QueryFiltersPart,
    pub all_riders: Vec<String>,
    pub all_companies: Vec<String>,
    pub all_vehicle_types: Vec<String>,
    pub rides: Vec<QueriedRidePart>,

    pub prev_page: Option<i64>,
    pub next_page: Option<i64>,
    pub filter_query_and: String,
    pub total_ride_count: i64,
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimvehiclestatus-setup.html")]
struct VehicleStatusSetupTemplate {
    pub companies: Vec<String>,
    pub default_company: Option<String>,
    pub riders: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimvehiclestatus.html")]
struct VehicleStatusTemplate {
    pub vehicles: BTreeMap<VehicleNumber, VehicleStatusEntry>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct VehicleStatusEntry {
    pub state: LastRideState,
    pub my_last_ride_opt: Option<RiderAndUtcTime>,
    pub other_last_ride_opt: Option<RiderAndUtcTime>,
    pub fixed_coupling: Vec<VehicleNumber>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct RiderAndUtcTime {
    pub rider: String,
    pub time: DateTime<Utc>,
    pub line: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum LastRideState {
    Unridden,
    OtherOnly,
    OtherLast,
    YouOnly,
    YouLast,
    YouOnlyRecently,
    YouLastRecently,
}


fn cows_to_owned_skip_empty<'a, 'b>(vals: Option<&'a Vec<Cow<'b, str>>>) -> Vec<String> {
    if let Some(some_vals) = vals {
        let mut ret = Vec::with_capacity(some_vals.len());
        for val in some_vals {
            if val.len() == 0 {
                continue;
            }
            ret.push(val.clowned());
        }
        ret
    } else {
        Vec::with_capacity(0)
    }
}


pub(crate) async fn handle_bim_query(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let query_pairs_multiset = get_query_pairs_multiset(request);
    let filters = {
        let last_timestamp = query_pairs_multiset
            .get("timestamp")
            .and_then(|tss| tss.last());
        let timestamp = match last_timestamp {
            Some(ts) => if ts.len() == 0 {
                None
            } else {
                match NaiveDate::parse_from_str(ts.as_ref(), "%Y-%m-%d") {
                    Ok(nd) => Some(nd),
                    Err(_) => return return_400("failed to parse date; expected format \"YYYY-MM-DD\"", &query_pairs).await,
                }
            },
            None => None,
        };
        let rider_usernames = cows_to_owned_skip_empty(query_pairs_multiset.get("rider"));
        let companies = cows_to_owned_skip_empty(query_pairs_multiset.get("company"));
        let lines = cows_to_owned_skip_empty(query_pairs_multiset.get("line"));
        let vehicle_numbers = cows_to_owned_skip_empty(query_pairs_multiset.get("vehicle-number"));
        let mut vehicle_types = cows_to_owned_skip_empty(query_pairs_multiset.get("vehicle-type"));
        let vehicle_ridden_only = query_pairs.get("vehicle-ridden-only")
            .map(|qp| qp == "1")
            .unwrap_or(false);

        let missing_vehicle_type = if vehicle_types.iter().any(|vt| vt == "\u{18}") {
            // we are filtering on vehicle types and want vehicles with missing type
            vehicle_types.retain(|vt| vt != "\u{18}");
            Some(true)
        } else if vehicle_types.len() > 0 {
            // we are filtering on vehicle types but don't want vehicles with missing type
            Some(false)
        } else {
            // we aren't filtering on vehicle types
            None
        };

        QueryFiltersPart {
            timestamp,
            rider_usernames,
            companies,
            lines,
            vehicle_numbers,
            vehicle_types,
            missing_vehicle_type,
            vehicle_ridden_only,
        }
    };
    let page: i64 = match query_pairs.get("page") {
        Some(page_str) => match page_str.parse() {
            Ok(p) => if p < 1 {
                return return_400("page numbers start at 1", &query_pairs).await
            } else {
                p
            },
            Err(_) => return return_400("invalid page number", &query_pairs).await,
        },
        None => 1,
    };

    // assemble query
    let mut next_filter_index = 1;
    let mut filter_pieces = Vec::new();
    let mut filter_values: Vec<&(dyn ToSql + Sync)> = Vec::new();
    let mut filter_query_and = String::new();

    if let Some(timestamp) = &filters.timestamp {
        filter_pieces.push(format!("CAST(rav.timestamp AS date) = ${}", next_filter_index));
        next_filter_index += 1;
        filter_values.push(timestamp);
        append_to_query(&mut filter_query_and, "timestamp", &timestamp.format("%Y-%m-%d").to_string());
    }
    if filters.rider_usernames.len() > 0 {
        filter_pieces.push(format!("rav.rider_username = ANY(${})", next_filter_index));
        next_filter_index += 1;
        filter_values.push(&filters.rider_usernames);
        for value in &filters.rider_usernames {
            append_to_query(&mut filter_query_and, "rider", value);
        }
    }
    if filters.companies.len() > 0 {
        filter_pieces.push(format!("rav.company = ANY(${})", next_filter_index));
        next_filter_index += 1;
        filter_values.push(&filters.companies);
        for value in &filters.companies {
            append_to_query(&mut filter_query_and, "company", value);
        }
    }
    if filters.lines.len() > 0 {
        filter_pieces.push(format!("rav.line = ANY(${})", next_filter_index));
        next_filter_index += 1;
        filter_values.push(&filters.lines);
        for value in &filters.lines {
            append_to_query(&mut filter_query_and, "line", value);
        }
    }
    if filters.vehicle_numbers.len() > 0 {
        // filtering on vehicle_number directly would limit output to only the filtered vehicle number
        // instead, check if the ride generally contains the vehicle number
        let mut num_piece = format!("EXISTS (SELECT 1 FROM bim.rides_and_vehicles rav_veh WHERE rav_veh.id = rav.id AND rav_veh.vehicle_number = ANY(${})", next_filter_index);
        if filters.vehicle_ridden_only {
            num_piece.push_str(" AND rav_veh.coupling_mode = 'R'");
        }
        num_piece.push(')');
        filter_pieces.push(num_piece);
        next_filter_index += 1;
        filter_values.push(&filters.vehicle_numbers);
        for value in &filters.vehicle_numbers {
            append_to_query(&mut filter_query_and, "vehicle-number", value);
        }
    }
    if filters.filtering_on_vehicle_type() {
        // same caveat as with vehicle number, also handle missing types
        let final_condition = if filters.missing_vehicle_type == Some(true) {
            if filters.vehicle_types.len() > 0 {
                let fc = format!("(rav_vehtp.vehicle_type IS NULL OR rav_vehtp.vehicle_type = ANY(${}))", next_filter_index);
                next_filter_index += 1;
                filter_values.push(&filters.vehicle_types);
                fc
            } else {
                "rav_vehtp.vehicle_type IS NULL".to_owned()
            }
        } else {
            assert!(filters.vehicle_types.len() > 0);
            let fc = format!("rav_vehtp.vehicle_type = ANY(${})", next_filter_index);
            next_filter_index += 1;
            filter_values.push(&filters.vehicle_types);
            fc
        };
        filter_pieces.push(format!("EXISTS (SELECT 1 FROM bim.rides_and_vehicles rav_vehtp WHERE rav_vehtp.id = rav.id AND {})", final_condition));
        if filters.missing_vehicle_type == Some(true) {
            append_to_query(&mut filter_query_and, "vehicle-type", "\u{18}");
        }
        for value in &filters.vehicle_types {
            append_to_query(&mut filter_query_and, "vehicle-type", value);
        }
    }

    let filter_string = filter_pieces.join(" AND ");
    if filter_query_and.len() > 0 {
        filter_query_and.push('&');
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let count_query = format!(
        "
            SELECT
                CAST(COUNT(*) AS bigint)
            FROM
                bim.rides rav
            {} {}
        ",
        if filter_string.len() > 0 { "WHERE" } else { "" },
        filter_string,
    );
    let count_res = db_conn.query_one(&count_query, &filter_values).await;
    let count_row = match count_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides for count: {}", e);
            debug!("failed query is {:?}", count_query);
            return return_500();
        },
    };
    let total_ride_count: i64 = count_row.get(0);

    const COUNT_PER_PAGE: i64 = 20;
    let offset = (page - 1) * COUNT_PER_PAGE;
    filter_values.push(&COUNT_PER_PAGE);
    filter_values.push(&offset);

    let query = format!(
        "
            SELECT
                rav.id, rav.company, rav.rider_username, rav.timestamp, rav.line,
                jsonb_agg(
                    row(rav.vehicle_number, rav.vehicle_type, rav.spec_position, rav.coupling_mode, rav.fixed_coupling_position)
                    ORDER BY rav.spec_position, rav.fixed_coupling_position
                ) vehicles_json
            FROM
                bim.rides_and_vehicles rav
            {} {}
            GROUP BY
                rav.id, rav.company, rav.rider_username, rav.timestamp, rav.line
            ORDER BY
                rav.timestamp DESC,
                rav.id DESC
            LIMIT ${} OFFSET ${}
        ",
        if filter_string.len() > 0 { "WHERE" } else { "" },
        filter_string,
        next_filter_index,
        next_filter_index + 1,
    );
    let riders_res = db_conn.query(&query, &filter_values).await;
    let rider_rows = match riders_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };

    let mut rides: Vec<QueriedRidePart> = Vec::with_capacity(rider_rows.len());
    for row in &rider_rows {
        let id: i64 = row.get(0);
        let company: String = row.get(1);
        let rider_username: String = row.get(2);
        let timestamp: DateTime<Local> = row.get(3);
        let line: Option<String> = row.get(4);
        let vehicles_json: serde_json::Value = row.get(5);

        let vehicles: Vec<QueriedRideVehiclePart> = vehicles_json
            .as_array().expect("vehicles_json not an array")
            .into_iter()
            .map(|veh| {
                let vehicle_number = veh["f1"].as_str().expect("vehicle.f1 (vehicle number) is not a string").to_owned();
                let vehicle_type = veh["f2"].as_str().map(|v| v.to_owned());
                let spec_position = veh["f3"].as_i64().expect("vehicle.f3 (spec position) is not an i64");
                let coupling_mode = veh["f4"].as_str().expect("vehicle.f4 (coupling mode) is not a string")
                    .chars().nth(0);
                let fixed_coupling_position = veh["f5"].as_i64().expect("vehicle.f5 (fixed coupling position) is not an i64");

                QueriedRideVehiclePart {
                    vehicle_number,
                    vehicle_type,
                    spec_position,
                    coupling_mode,
                    fixed_coupling_position,
                }
            })
            .collect();

        rides.push(QueriedRidePart {
            id,
            timestamp,
            rider_username,
            company,
            line,
            vehicles,
        });
    }

    let all_rider_rows_res = db_conn.query(
        "SELECT DISTINCT rider_username FROM bim.rides",
        &[],
    ).await;
    let all_rider_rows = match all_rider_rows_res {
        Ok(arr) => arr,
        Err(e) => {
            error!("failed to query riders: {}", e);
            return return_500();
        },
    };
    let mut all_riders = Vec::new();
    for rider_row in all_rider_rows {
        let rider_username: String = rider_row.get(0);
        all_riders.push(rider_username);
    }
    sort_as_text(&mut all_riders);

    let all_company_rows_res = db_conn.query(
        "SELECT DISTINCT company FROM bim.rides",
        &[],
    ).await;
    let all_company_rows = match all_company_rows_res {
        Ok(acr) => acr,
        Err(e) => {
            error!("failed to query companies: {}", e);
            return return_500();
        },
    };
    let mut all_companies = Vec::new();
    for company_row in all_company_rows {
        let company: String = company_row.get(0);
        all_companies.push(company);
    }
    sort_as_text(&mut all_companies);

    let all_type_rows_res = db_conn.query(
        "SELECT DISTINCT vehicle_type FROM bim.ride_vehicles WHERE vehicle_type IS NOT NULL",
        &[],
    ).await;
    let all_type_rows = match all_type_rows_res {
        Ok(acr) => acr,
        Err(e) => {
            error!("failed to query vehicle types: {}", e);
            return return_500();
        },
    };
    let mut all_vehicle_types = Vec::new();
    for type_row in all_type_rows {
        let vehicle_type: String = type_row.get(0);
        all_vehicle_types.push(vehicle_type);
    }
    sort_as_text(&mut all_vehicle_types);

    let prev_page = if page > 1 { Some(page - 1) } else { None };
    let next_page = if rides.len() > 0 { Some(page + 1) } else { None };
    let template = QueryTemplate {
        filters,
        rides,
        all_riders,
        all_companies,
        all_vehicle_types,
        prev_page,
        next_page,
        filter_query_and,
        total_ride_count,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_vehicle_status(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let company_opt = query_pairs.get("company");
    let rider_opt = query_pairs.get("rider");

    let company_to_definition = match obtain_company_to_definition().await {
        Some(ctd) => ctd,
        None => return return_500(),
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    match (company_opt, rider_opt) {
        (Some(company), Some(rider)) => {
            let company_to_bim_database_opts = match obtain_company_to_bim_database(&company_to_definition) {
                Some(ctbdb) => ctbdb,
                None => return return_500(),
            };
            let empty_database = BTreeMap::new();
            let bim_database = match company_to_bim_database_opts.get(company.as_ref()) {
                Some(Some(bd)) => bd,
                _ => &empty_database,
            };

            let rows_res = db_conn.query(
                "
                    SELECT
                        rav.vehicle_number, rav.\"timestamp\", rav.rider_username, rav.line
                    FROM bim.rides_and_vehicles rav
                    WHERE rav.company = $1
                    AND rav.coupling_mode = 'R'
                    AND rav.rider_username = $2
                    AND NOT EXISTS (
                        SELECT 1
                        FROM bim.rides_and_vehicles rav2
                        WHERE rav2.company = rav.company
                        AND rav2.vehicle_number = rav.vehicle_number
                        AND rav2.coupling_mode = rav.coupling_mode
                        AND rav2.rider_username = $2
                        AND rav2.\"timestamp\" > rav.\"timestamp\"
                    )

                    UNION ALL

                    SELECT
                        rav3.vehicle_number, rav3.\"timestamp\", rav3.rider_username, rav3.line
                    FROM bim.rides_and_vehicles rav3
                    WHERE rav3.company = $1
                    AND rav3.coupling_mode = 'R'
                    AND rav3.rider_username <> $2
                    AND NOT EXISTS (
                        SELECT 1
                        FROM bim.rides_and_vehicles rav4
                        WHERE rav4.company = rav3.company
                        AND rav4.vehicle_number = rav3.vehicle_number
                        AND rav4.coupling_mode = rav3.coupling_mode
                        AND rav4.rider_username <> $2
                        AND rav4.\"timestamp\" > rav3.\"timestamp\"
                    )
                ",
                &[&company.as_ref(), &rider.as_ref()],
            ).await;
            let timestamp = Utc::now();
            let rows = match rows_res {
                Ok(r) => r,
                Err(e) => {
                    error!("failed to read rows: {}", e);
                    return return_500();
                },
            };

            let mut vehicle_to_last_rides: BTreeMap<VehicleNumber, Vec<RiderAndUtcTime>> = BTreeMap::new();
            for row in rows {
                let vehicle_number_raw: String = row.get(0);
                let time: DateTime<Utc> = row.get(1);
                let rider_username: String = row.get(2);
                let line: Option<String> = row.get(3);

                let vehicle_number = VehicleNumber::from_string(vehicle_number_raw);
                let last = RiderAndUtcTime {
                    rider: rider_username,
                    time,
                    line,
                };
                vehicle_to_last_rides
                    .entry(vehicle_number)
                    .or_insert_with(|| Vec::with_capacity(2))
                    .push(last);
            }

            let mut vehicles = BTreeMap::new();
            for (vehicle_number, last_rides) in vehicle_to_last_rides {
                let my_last_ride_opt = last_rides.iter()
                    .filter(|r| &r.rider == rider)
                    .nth(0)
                    .map(|rat| rat.clone());
                let other_last_ride_opt = last_rides.iter()
                    .filter(|r| &r.rider != rider)
                    .nth(0)
                    .map(|rat| rat.clone());

                let state = match (&my_last_ride_opt, &other_last_ride_opt) {
                    (None, None) => LastRideState::Unridden,
                    (Some(my_last_ride), None) => {
                        if my_last_ride.time <= timestamp && my_last_ride.time - timestamp < Duration::hours(24) {
                            LastRideState::YouOnlyRecently
                        } else {
                            LastRideState::YouOnly
                        }
                    },
                    (None, Some(_other_last_ride)) => LastRideState::OtherOnly,
                    (Some(my_last_ride), Some(other_last_ride)) => {
                        if my_last_ride.time >= other_last_ride.time {
                            if my_last_ride.time <= timestamp && my_last_ride.time - timestamp < Duration::hours(24) {
                                LastRideState::YouLastRecently
                            } else {
                                LastRideState::YouLast
                            }
                        } else {
                            LastRideState::OtherLast
                        }
                    },
                };
                let fixed_coupling: Vec<VehicleNumber> = bim_database.get(&vehicle_number)
                    .map(|fc| fc.fixed_coupling.iter().map(|v| v.clone()).collect())
                    .unwrap_or_else(|| Vec::with_capacity(0));

                vehicles.insert(
                    vehicle_number,
                    VehicleStatusEntry {
                        state,
                        my_last_ride_opt,
                        other_last_ride_opt,
                        fixed_coupling,
                    },
                );
            }

            for (vehicle_number, vehicle_entry) in bim_database.iter() {
                if vehicles.contains_key(vehicle_number) {
                    continue;
                }
                let fixed_coupling: Vec<VehicleNumber> = vehicle_entry.fixed_coupling.iter()
                    .map(|v| v.clone())
                    .collect();
                vehicles.insert(
                    vehicle_number.clone(),
                    VehicleStatusEntry {
                        state: LastRideState::Unridden,
                        my_last_ride_opt: None,
                        other_last_ride_opt: None,
                        fixed_coupling,
                    },
                );
            }

            let template = VehicleStatusTemplate {
                vehicles,
                timestamp,
            };
            match render_response(&template, &query_pairs, 200, vec![]).await {
                Some(r) => Ok(r),
                None => return_500(),
            }
        },
        _ => {
            // show setup page
            let plugin_config = match obtain_bim_plugin_config().await {
                Some(p) => p,
                None => return return_500(),
            };
            let default_company = match &plugin_config["config"]["default_company"] {
                serde_json::Value::Null => None,
                serde_json::Value::String(s) => Some(s.clone()),
                other => {
                    error!("default company has unexpected value {:?}", other);
                    return return_500();
                },
            };

            let mut riders_set = HashSet::new();
            let rows = match db_conn.query("SELECT DISTINCT rider_username FROM bim.rides", &[]).await {
                Ok(r) => r,
                Err(e) => {
                    error!("error querying riders: {}", e);
                    return return_500();
                },
            };
            for row in rows {
                let rider: String = row.get(0);
                riders_set.insert(rider);
            }
            let mut riders: Vec<String> = riders_set.into_iter().collect();
            sort_as_text(&mut riders);

            let companies_set: HashSet<String> = company_to_definition.keys()
                .map(|k| k.clone())
                .collect();
            let mut companies: Vec<String> = companies_set.into_iter().collect();
            sort_as_text(&mut companies);

            let template = VehicleStatusSetupTemplate {
                companies,
                default_company,
                riders,
            };
            match render_response(&template, &query_pairs, 200, vec![]).await {
                Some(r) => Ok(r),
                None => return_500(),
            }
        },
    }
}
