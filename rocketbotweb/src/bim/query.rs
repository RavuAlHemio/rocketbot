use std::borrow::Cow;
use std::collections::BTreeSet;
use std::convert::Infallible;

use askama::Template;
use chrono::{DateTime, Local, NaiveDate};
use hyper::{Body, Method, Request, Response};
use log::error;
use serde::Serialize;
use tokio_postgres::types::ToSql;

use crate::{get_query_pairs, render_response, return_400, return_405, return_500};
use crate::bim::{append_to_query, connect_to_db};


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct QueryFiltersPart {
    pub timestamp: Option<NaiveDate>,
    pub rider_username: Option<String>,
    pub company: Option<String>,
    pub line: Option<String>,
    pub vehicle_number: Option<String>,
    pub vehicle_type: Option<String>,
}
impl QueryFiltersPart {
    pub fn want_missing_vehicle_types(&self) -> bool {
        self.vehicle_type
            .as_ref()
            .map(|vt| vt == "\u{18}")
            .unwrap_or(false)
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
    pub all_riders: BTreeSet<String>,
    pub all_companies: BTreeSet<String>,
    pub all_vehicle_types: BTreeSet<String>,
    pub rides: Vec<QueriedRidePart>,

    pub prev_page: Option<i64>,
    pub next_page: Option<i64>,
    pub filter_query_and: String,
}


#[inline]
fn cow_to_owned_or_empty_to_none<'a, 'b>(val: Option<&'a Cow<'b, str>>) -> Option<String> {
    match val {
        None => None,
        Some(x) => if x.len() > 0 {
            Some(x.clone().into_owned())
        } else {
            None
        },
    }
}


pub(crate) async fn handle_bim_query(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let filters = {
        let timestamp = match query_pairs.get("timestamp") {
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
        let rider_username = cow_to_owned_or_empty_to_none(query_pairs.get("rider"));
        let company = cow_to_owned_or_empty_to_none(query_pairs.get("company"));
        let line = cow_to_owned_or_empty_to_none(query_pairs.get("line"));
        let vehicle_number = cow_to_owned_or_empty_to_none(query_pairs.get("vehicle-number"));
        let vehicle_type = cow_to_owned_or_empty_to_none(query_pairs.get("vehicle-type"));

        QueryFiltersPart {
            timestamp,
            rider_username,
            company,
            line,
            vehicle_number,
            vehicle_type,
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
    if let Some(rider_username) = &filters.rider_username {
        filter_pieces.push(format!("rav.rider_username = ${}", next_filter_index));
        next_filter_index += 1;
        filter_values.push(rider_username);
        append_to_query(&mut filter_query_and, "rider", rider_username);
    }
    if let Some(company) = &filters.company {
        filter_pieces.push(format!("rav.company = ${}", next_filter_index));
        next_filter_index += 1;
        filter_values.push(company);
        append_to_query(&mut filter_query_and, "company", company);
    }
    if let Some(line) = &filters.line {
        filter_pieces.push(format!("rav.line = ${}", next_filter_index));
        next_filter_index += 1;
        filter_values.push(line);
        append_to_query(&mut filter_query_and, "line", line);
    }
    if let Some(vehicle_number) = &filters.vehicle_number {
        // filtering on vehicle_number directly would limit output to only the filtered vehicle number
        // instead, check if the ride generally contains the vehicle number
        filter_pieces.push(format!("EXISTS (SELECT 1 FROM bim.rides_and_vehicles rav_veh WHERE rav_veh.id = rav.id AND rav_veh.vehicle_number = ${})", next_filter_index));
        next_filter_index += 1;
        filter_values.push(vehicle_number);
        append_to_query(&mut filter_query_and, "vehicle-number", vehicle_number);
    }
    if filters.want_missing_vehicle_types() {
        // same caveat as with vehicle number
        filter_pieces.push(format!("EXISTS (SELECT 1 FROM bim.rides_and_vehicles rav_vehtp WHERE rav_vehtp.id = rav.id AND rav_vehtp.vehicle_type IS NULL)"));
        // no value here
        append_to_query(&mut filter_query_and, "vehicle-type", "\u{18}");
    } else if let Some(vehicle_type) = &filters.vehicle_type {
        // same caveat as with vehicle number
        filter_pieces.push(format!("EXISTS (SELECT 1 FROM bim.rides_and_vehicles rav_vehtp WHERE rav_vehtp.id = rav.id AND rav_vehtp.vehicle_type = ${})", next_filter_index));
        next_filter_index += 1;
        filter_values.push(vehicle_type);
        append_to_query(&mut filter_query_and, "vehicle-type", vehicle_type);
    }

    let filter_string = filter_pieces.join(" AND ");
    if filter_query_and.len() > 0 {
        filter_query_and.push('&');
    }

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

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

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
    let mut all_riders = BTreeSet::new();
    for rider_row in all_rider_rows {
        let rider_username: String = rider_row.get(0);
        all_riders.insert(rider_username);
    }

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
    let mut all_companies = BTreeSet::new();
    for company_row in all_company_rows {
        let company: String = company_row.get(0);
        all_companies.insert(company);
    }

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
    let mut all_vehicle_types = BTreeSet::new();
    for type_row in all_type_rows {
        let vehicle_type: String = type_row.get(0);
        all_vehicle_types.insert(vehicle_type);
    }

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
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
