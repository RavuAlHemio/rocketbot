use std::collections::BTreeMap;
use std::convert::Infallible;
use std::str::FromStr;

use askama::Template;
use chrono::{DateTime, Local};
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use rocketbot_bim_common::{CouplingMode, VehicleInfo, VehicleNumber};
use rocketbot_date_time::DateTimeLocalWithWeekday;
use serde::Serialize;
use tracing::error;

use crate::{get_query_pairs, render_response, return_400, return_405, return_500};
use crate::bim::{connect_to_db, obtain_company_to_bim_database, obtain_company_to_definition};
use crate::templating::filters;


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct BimDetailsRidePart {
    pub id: i64,
    pub rider_username: String,
    pub timestamp: String,
    pub line: Option<String>,
    pub vehicle_number: VehicleNumber,
    pub spec_position: i64,
    pub coupling_mode: CouplingMode,
    pub fixed_coupling_position: i64,
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
    pub coupling_mode: CouplingMode,
    pub fixed_coupling_position: i64,
}


#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimdetails.html")]
struct BimDetailsTemplate {
    pub company: String,
    pub vehicle: Option<VehicleInfo>,
    pub rides: Vec<BimDetailsRidePart>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimlinedetails.html")]
struct BimLineDetailsTemplate {
    pub company: String,
    pub line: String,
    pub rides: Vec<BimDetailsRidePart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimridebyid.html")]
struct BimRideByIdTemplate {
    pub id_param: String,
    pub ride_state: RideInfoState,
}


pub(crate) async fn handle_bim_detail(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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

    let company_bim_database = match company_to_bim_database.get(&company) {
        Some(bd) => bd,
        None => return return_400("unknown company", &query_pairs).await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let vehicle = company_bim_database.get(&vehicle_number)
        .map(|v| v.clone());

    // query rides
    let ride_rows_res = db_conn.query(
        "
            SELECT
                rav.id, rav.rider_username, rav.\"timestamp\", rav.line, rav.vehicle_number,
                rav.spec_position, rav.coupling_mode, rav.fixed_coupling_position
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
        let coupling_mode_string: String = ride_row.get(6);
        let fixed_coupling_position: i64 = ride_row.get(7);

        let coupling_mode = match CouplingMode::try_from_db_str(&coupling_mode_string) {
            Some(cm) => cm,
            None => {
                error!(
                    "error decoding coupling mode string {:?} on ride ID {} from database; skipping row",
                    coupling_mode_string, ride_id,
                );
                continue;
            }
        };

        rides.push(BimDetailsRidePart {
            id: ride_id,
            rider_username,
            timestamp: timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            line,
            vehicle_number,
            spec_position,
            coupling_mode,
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

pub(crate) async fn handle_bim_line_detail(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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
                rav.spec_position, rav.coupling_mode, rav.fixed_coupling_position
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
        let coupling_mode_string: String = ride_row.get(6);
        let fixed_coupling_position: i64 = ride_row.get(7);

        let coupling_mode = match CouplingMode::try_from_db_str(&coupling_mode_string) {
            Some(cm) => cm,
            None => {
                error!(
                    "error decoding coupling mode string {:?} on ride ID {} from database; skipping row",
                    coupling_mode_string, ride_id,
                );
                continue;
            }
        };

        rides.push(BimDetailsRidePart {
            id: ride_id,
            rider_username,
            timestamp: timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            line,
            vehicle_number,
            spec_position,
            coupling_mode,
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


pub(crate) async fn handle_bim_ride_by_id(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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
                        rav.coupling_mode, rav.fixed_coupling_position
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
                    let coupling_mode_string: String = ride_row.get(7);
                    let fixed_coupling_position: i64 = ride_row.get(8);

                    let coupling_mode = match CouplingMode::try_from_db_str(&coupling_mode_string) {
                        Some(cm) => cm,
                        None => {
                            error!(
                                "error decoding coupling mode string {:?} on ride ID {} from database; skipping row",
                                coupling_mode_string, rid,
                            );
                            continue;
                        }
                    };

                    let vehicle = RideVehiclePart {
                        vehicle_number,
                        vehicle_type,
                        spec_position,
                        coupling_mode,
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
