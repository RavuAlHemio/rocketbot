use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::Infallible;

use askama::Template;
use chrono::NaiveDate;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use rocketbot_bim_common::VehicleNumber;
use rocketbot_string::NatSortedString;
use serde::Serialize;
use tokio_postgres::types::ToSql;
use tracing::error;

use crate::{get_query_pairs, render_response, return_405, return_500};
use crate::bim::connect_to_db;
use crate::templating::filters;


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CountVehiclesPart {
    pub ride_count: i64,
    pub vehicles: BTreeSet<VehiclePart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct RiderGroupPart {
    pub riders: BTreeSet<String>,
    pub vehicles: BTreeSet<VehiclePart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct LineGroupPart {
    pub lines: BTreeSet<LinePart>,
    pub vehicles: BTreeSet<VehiclePart>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct VehiclePart {
    pub company: String,
    pub number: VehicleNumber,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CountLinesPart {
    pub ride_count: i64,
    pub lines: BTreeSet<LinePart>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct LinePart {
    pub company: String,
    pub line: NatSortedString,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct DayCountPart {
    pub day: NaiveDate,
    pub total_count: i64,
    pub rider_to_count: BTreeMap<String, i64>,
}
impl DayCountPart {
    pub fn riders_and_counts_desc(&self) -> Vec<(&str, i64)> {
        let mut ret: Vec<(&str, i64)> = self.rider_to_count
            .iter()
            .map(|(r, c)| (r.as_str(), *c))
            .collect();
        ret.sort_unstable_by_key(|(_r, c)| -(*c));
        ret
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "topbims.html")]
struct TopBimsTemplate {
    pub counts_vehicles: Vec<CountVehiclesPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "widebims.html")]
struct WideBimsTemplate {
    pub rider_count: i64,
    pub rider_groups: Vec<RiderGroupPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "explorerbims.html")]
struct ExplorerBimsTemplate {
    pub line_count: i64,
    pub line_groups: Vec<LineGroupPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "topbimlines.html")]
struct TopBimLinesTemplate {
    pub counts_lines: Vec<CountLinesPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "topbimdays.html")]
struct TopDaysTemplate {
    pub counts_and_days: Vec<(i64, BTreeSet<DayCountPart>)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bim-last-riders.html")]
struct LastRidersTemplate {
    pub riders: Vec<RiderVehiclesPart>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct RiderVehiclesPart {
    pub rider: String,
    pub vehicles: BTreeSet<VehiclePart>,
}


pub(crate) async fn handle_top_bims(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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
                WHERE rav.coupling_mode = 'R'
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


pub(crate) async fn handle_wide_bims(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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
                    WHERE rav.coupling_mode = 'R'
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
                WHERE rav.coupling_mode = 'R'
                GROUP BY rav.company, rav.vehicle_number
            )
            SELECT DISTINCT rav.company, rav.vehicle_number, rav.rider_username rc
            FROM bim.rides_and_vehicles rav
            INNER JOIN vehicle_and_distinct_rider_count vadrc
                ON vadrc.company = rav.company
                AND vadrc.vehicle_number = rav.vehicle_number
            WHERE
                vadrc.rider_count = $1
                AND rav.coupling_mode = 'R'
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

pub(crate) async fn handle_explorer_bims(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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

    let line_count = if let Some(c) = count_opt {
        c
    } else {
        // query for most lines per vehicle
        let most_lines_row_opt_res = db_conn.query_opt(
            "
                WITH vehicle_and_distinct_line_count(company, vehicle_number, line_count) AS (
                    SELECT rav.company, rav.vehicle_number, COUNT(DISTINCT rav.line)
                    FROM bim.rides_and_vehicles rav
                    WHERE rav.coupling_mode = 'R'
                    AND rav.line IS NOT NULL
                    GROUP BY rav.company, rav.vehicle_number
                )
                SELECT CAST(COALESCE(MAX(line_count), 0) AS bigint) FROM vehicle_and_distinct_line_count
            ",
            &[],
        ).await;
        match most_lines_row_opt_res {
            Ok(Some(r)) => r.get(0),
            Ok(None) => 0,
            Err(e) => {
                error!("error querying maximum distinct line count: {}", e);
                return return_500();
            },
        }
    };

    // query rides
    let ride_rows_res = db_conn.query(
        "
            WITH vehicle_and_distinct_line_count(company, vehicle_number, line_count) AS (
                SELECT rav.company, rav.vehicle_number, COUNT(DISTINCT rav.line)
                FROM bim.rides_and_vehicles rav
                WHERE rav.coupling_mode = 'R'
                AND rav.line IS NOT NULL
                GROUP BY rav.company, rav.vehicle_number
            )
            SELECT DISTINCT rav.company, rav.vehicle_number, rav.line
            FROM bim.rides_and_vehicles rav
            INNER JOIN vehicle_and_distinct_line_count vadlc
                ON vadlc.company = rav.company
                AND vadlc.vehicle_number = rav.vehicle_number
            WHERE
                vadlc.line_count = $1
                AND rav.line IS NOT NULL
        ",
        &[&line_count],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut vehicle_to_lines: HashMap<(String, VehicleNumber), BTreeSet<String>> = HashMap::new();
    for ride_row in ride_rows {
        let company: String = ride_row.get(0);
        let vehicle_number = VehicleNumber::from_string(ride_row.get(1));
        let line: String = ride_row.get(2);

        vehicle_to_lines
            .entry((company, vehicle_number))
            .or_insert_with(|| BTreeSet::new())
            .insert(line);
    }

    let mut line_groups_to_rides: BTreeMap<BTreeSet<(String, String)>, BTreeSet<VehiclePart>> = BTreeMap::new();
    for ((company, vehicle_number), lines) in vehicle_to_lines.drain() {
        let lines_with_company: BTreeSet<(String, String)> = lines.into_iter()
            .map(|l| (company.clone(), l))
            .collect();
        line_groups_to_rides
            .entry(lines_with_company)
            .or_insert_with(|| BTreeSet::new())
            .insert(VehiclePart {
                company,
                number: vehicle_number,
            });
    }

    let line_groups: Vec<LineGroupPart> = line_groups_to_rides.iter()
        .map(|(lines, rides)| LineGroupPart {
            lines: lines.iter()
                .map(|(c, l)| LinePart { company: c.clone(), line: l.clone().into() })
                .collect(),
            vehicles: rides.clone(),
        })
        .collect();

    let template = ExplorerBimsTemplate {
        line_count,
        line_groups,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_top_bim_lines(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let top_count: i64 = query_pairs.get("count")
        .map(|c_str| c_str.parse().ok())
        .flatten()
        .filter(|tc| *tc > 0)
        .unwrap_or(10);
    let username_opt = query_pairs.get("username")
        .and_then(|u| if u.len() == 0 { None } else { Some(u) });

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut ride_counts_criteria = Vec::new();
    let mut query_params: Vec<&(dyn ToSql + Sync)> = Vec::new();
    query_params.push(&top_count);

    if let Some(username) = username_opt {
        ride_counts_criteria.push(format!("AND r.rider_username = ${}", query_params.len() + 1));
        query_params.push(username);
    }

    // query rides
    let query = format!(
        "
            WITH ride_counts(company, line, ride_count) AS (
                SELECT r.company, r.line, COUNT(*)
                FROM bim.rides r
                WHERE r.line IS NOT NULL
                {}
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
        ride_counts_criteria.join(" "),
    );
    let ride_rows_res = db_conn.query(&query, &query_params).await;
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
                    line: l.clone().into(),
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

pub(crate) async fn handle_top_bim_days(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let top_count: i64 = query_pairs.get("count")
        .map(|c_str| c_str.parse().ok())
        .flatten()
        .filter(|tc| *tc > 0)
        .unwrap_or(10);
    let username_opt = query_pairs.get("username")
        .and_then(|u| if u.len() == 0 { None } else { Some(u) });

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut ride_counts_criteria = Vec::new();
    let mut main_criteria = Vec::new();
    let mut query_params: Vec<&(dyn ToSql + Sync)> = Vec::new();
    query_params.push(&top_count);

    if let Some(username) = username_opt {
        ride_counts_criteria.push(format!("r.rider_username = ${}", query_params.len() + 1));
        query_params.push(username);

        main_criteria.push(format!("AND r.rider_username = ${}", query_params.len() + 1));
        query_params.push(username);
    }

    // query rides
    let query = format!(
        "
            WITH ride_counts(ride_date, ride_count) AS (
                SELECT bim.to_transport_date(r.\"timestamp\"), COUNT(*)
                FROM bim.rides r
                {} {}
                GROUP BY bim.to_transport_date(r.\"timestamp\")
            ),
            top_ride_counts(ride_count) AS (
                SELECT DISTINCT ride_count
                FROM ride_counts
                ORDER BY ride_count DESC
                LIMIT $1
            )
            SELECT rc.ride_date, r.rider_username, rc.ride_count, CAST(COUNT(*) AS bigint)
            FROM bim.rides r
            INNER JOIN ride_counts rc
                ON rc.ride_date = bim.to_transport_date(r.\"timestamp\")
            WHERE EXISTS (
                SELECT 1
                FROM top_ride_counts trc
                WHERE trc.ride_count = rc.ride_count
            )
            {}
            GROUP BY rc.ride_date, r.rider_username, rc.ride_count
        ",
        if ride_counts_criteria.len() > 0 { "WHERE" } else { "" },
        ride_counts_criteria.join(" AND "),
        main_criteria.join(" "),
    );
    let ride_rows_res = db_conn.query(&query, &query_params).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut day_to_day_count: BTreeMap<NaiveDate, DayCountPart> = BTreeMap::new();
    for ride_row in ride_rows {
        let ride_date: NaiveDate = ride_row.get(0);
        let rider_username: String = ride_row.get(1);
        let total_count: i64 = ride_row.get(2);
        let rider_count: i64 = ride_row.get(3);

        day_to_day_count
            .entry(ride_date)
            .or_insert_with(|| DayCountPart {
                day: ride_date,
                total_count,
                rider_to_count: BTreeMap::new(),
            })
            .rider_to_count.insert(rider_username, rider_count);
    }

    let mut count_to_days: BTreeMap<i64, BTreeSet<DayCountPart>> = BTreeMap::new();
    for day_count in day_to_day_count.values() {
        count_to_days
            .entry(day_count.total_count)
            .or_insert_with(|| BTreeSet::new())
            .insert(day_count.clone());
    }

    let mut counts_and_days: Vec<(i64, BTreeSet<DayCountPart>)> = count_to_days.iter()
        .map(|(c, d)| (*c, d.clone()))
        .collect();
    counts_and_days.sort_unstable_by_key(|(c, _d)| -(*c));

    let template = TopDaysTemplate {
        counts_and_days,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_last_riders(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // query rides
    let ride_rows_res = db_conn.query(
        "
            SELECT rarv.rider_username, rarv.company, rarv.vehicle_number
            FROM bim.rides_and_ridden_vehicles rarv
            WHERE NOT EXISTS (
                SELECT 1
                FROM bim.rides_and_ridden_vehicles rarv2
                WHERE rarv2.company = rarv.company
                AND rarv2.vehicle_number = rarv.vehicle_number
                AND (
                    rarv2.\"timestamp\" > rarv.\"timestamp\"
                    OR (
                        rarv2.\"timestamp\" = rarv.\"timestamp\"
                        AND rarv2.id > rarv.id
                    )
                )
            )
        ",
        &[],
    ).await;
    let ride_rows = match ride_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let mut rider_to_last_vehicles: BTreeMap<String, BTreeSet<VehiclePart>> = BTreeMap::new();
    for ride_row in ride_rows {
        let rider_username: String = ride_row.get(0);
        let company: String = ride_row.get(1);
        let vehicle_number = VehicleNumber::from_string(ride_row.get(2));

        rider_to_last_vehicles
            .entry(rider_username)
            .or_insert_with(|| BTreeSet::new())
            .insert(VehiclePart {
                company,
                number: vehicle_number,
            });
    }

    let riders: Vec<RiderVehiclesPart> = rider_to_last_vehicles.into_iter()
        .map(|(rider, vehicles)| {
            RiderVehiclesPart {
                rider,
                vehicles,
            }
        })
        .collect();

    let template = LastRidersTemplate {
        riders,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
