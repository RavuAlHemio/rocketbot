use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::Write;

use askama::Template;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate};
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use rocketbot_bim_common::{CouplingMode, VehicleNumber};
use rocketbot_interface::clown::ClownExt;
use serde::Serialize;
use tokio_postgres::types::ToSql;
use tracing::error;

use crate::{get_query_pairs, render_response, return_400, return_405, return_500};
use crate::bim::{connect_to_db, obtain_company_to_bim_database, obtain_vehicle_extract};
use crate::line_graph_drawing::{LineGraph, GRAPH_COLORS};
use crate::templating::filters;

use super::obtain_company_to_definition;


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct GraphRiderPart {
    pub name: String,
    pub color: [u8; 3],
}
impl GraphRiderPart {
    pub fn color_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.color[0], self.color[1], self.color[2])
    }
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimlatestridercount.html")]
struct BimLatestRiderCountTemplate {
    pub company: Option<String>,
    pub riders: Vec<GraphRiderPart>,
    pub from_to_count: BTreeMap<(String, String), u64>,
}
impl BimLatestRiderCountTemplate {
    fn sankey_json_data(&self) -> String {
        let json_object: Vec<serde_json::Value> = self.from_to_count.iter()
            .map(|((f, t), count)| serde_json::json!({
                "from": format!("\u{238B}{}", f),
                "to": format!("\u{2386}{}", t),
                "flow": count,
            }))
            .collect();
        serde_json::to_string(&json_object)
            .expect("failed to serialize Sankey JSON?!")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimhistogramdow.html")]
struct HistogramByDayOfWeekTemplate {
    pub rider_to_weekday_counts: BTreeMap<String, [i64; 7]>,
}
impl HistogramByDayOfWeekTemplate {
    pub fn json_data(&self) -> String {
        let riders: Vec<&String> = self.rider_to_weekday_counts
            .keys()
            .collect();
        let value = serde_json::json!({
            "riders": riders,
            "riderToWeekdayToCount": self.rider_to_weekday_counts,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimhistogramridecountgroup.html")]
struct HistogramByRideCountGroupTemplate {
    pub what: String,
    pub ride_count_group_names: Vec<String>,
    pub rider_to_group_counts: BTreeMap<String, Vec<i64>>,
}
impl HistogramByRideCountGroupTemplate {
    pub fn json_data(&self) -> String {
        let riders: Vec<&String> = self.rider_to_group_counts
            .keys()
            .collect();
        let value = serde_json::json!({
            "riders": riders,
            "rideCountGroupNames": self.ride_count_group_names,
            "riderToGroupToCount": self.rider_to_group_counts,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimlastriderpie.html")]
struct LastRiderPieTemplate {
    pub company_to_type_to_rider_to_last_count: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>>,
    pub company_to_type_to_rider_to_last_count_ridden: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>>,
}
impl LastRiderPieTemplate {
    pub fn json_data(&self) -> String {
        let value = serde_json::json!({
            "companyToTypeToLastRiderToCount": self.company_to_type_to_rider_to_last_count,
            "companyToTypeToLastRiderToCountRidden": self.company_to_type_to_rider_to_last_count_ridden,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimhistogramfixedcoupling.html")]
struct HistogramFixedCouplingTemplate {
    front_vehicle_type_to_rider_to_counts: BTreeMap<String, BTreeMap<String, Vec<i64>>>,
}
impl HistogramFixedCouplingTemplate {
    pub fn json_data(&self) -> String {
        serde_json::to_string(&self.front_vehicle_type_to_rider_to_counts)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimglobalstats.html")]
struct GlobalStatsTemplate {
    pub total_rides: i64,
    pub company_to_total_rides: BTreeMap<String, i64>,
}
impl GlobalStatsTemplate {
    pub fn json_data(&self) -> String {
        let value = serde_json::json!({
            "totalRides": self.total_rides,
            "companyToTotalRides": self.company_to_total_rides,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimfirstriderpie.html")]
struct FirstRiderPieTemplate {
    pub company_to_rider_to_first_rides: BTreeMap<String, BTreeMap<String, i64>>,
    pub rider_to_total_first_rides: BTreeMap<String, i64>,
}
impl FirstRiderPieTemplate {
    pub fn json_data(&self) -> String {
        let value = serde_json::json!({
            "companyToRiderToFirstRides": self.company_to_rider_to_first_rides,
            "riderToTotalFirstRides": self.rider_to_total_first_rides,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimtypehistogram.html")]
struct TypeHistogramTemplate {
    pub company_to_vehicle_type_to_rider_to_count: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>>,
    pub company_to_vehicle_type_to_count: BTreeMap<String, BTreeMap<String, i64>>,
}
impl TypeHistogramTemplate {
    pub fn json_data(&self) -> serde_json::Value {
        serde_json::json!({
            "companyToVehicleTypeToRiderToCount": self.company_to_vehicle_type_to_rider_to_count,
            "companyToVehicleTypeToCount": self.company_to_vehicle_type_to_count,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct MonopolyRide {
    pub id: i64,
    pub company: String,
    pub rider_username: String,
    pub timestamp: DateTime<Local>,
    pub vehicles: Vec<MonopolyVehicle>,
}
impl MonopolyRide {
    pub fn monopoly_rider_username<'c>(&self, company_to_vehicle_number_to_last_rider: &'c BTreeMap<String, BTreeMap<String, String>>) -> Option<&'c str> {
        if self.vehicles.len() < 2 {
            return None;
        }
        let Some(vehicle_number_to_last_rider) = company_to_vehicle_number_to_last_rider.get(&self.company)
            else { return None };
        let Some(first_vehicle_last_rider) = vehicle_number_to_last_rider.get(&self.vehicles[0].vehicle_number)
            else { return None };
        for vehicle in self.vehicles.iter().skip(1) {
            let Some(this_vehicle_last_rider) = vehicle_number_to_last_rider.get(&vehicle.vehicle_number)
                else { return None };
            if first_vehicle_last_rider != this_vehicle_last_rider {
                return None;
            }
        }
        Some(&first_vehicle_last_rider)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct MonopolyVehicle {
    pub vehicle_number: String,
    pub coupling_mode: CouplingMode,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimfixedmonopoliesovertime.html")]
struct FixedMonopoliesOverTime {
    pub rider_to_timestamp_to_monopolies: BTreeMap<String, BTreeMap<String, MonopolyEntry>>,
}
impl FixedMonopoliesOverTime {
    pub fn json_data(&self) -> serde_json::Value {
        serde_json::json!({
            "riderToTimestampToMonopolies": self.rider_to_timestamp_to_monopolies,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct MonopolyEntry {
    pub count: usize,
    pub points: usize,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimlastriderhistfixedpos.html")]
struct LastRiderHistogramByFixedPosTemplate {
    pub leading_type_to_rider_to_counts: BTreeMap<String, BTreeMap<String, Vec<i64>>>,
}
impl LastRiderHistogramByFixedPosTemplate {
    pub fn json_data(&self) -> String {
        let value = serde_json::json!({
            "leadingTypeToRiderToCounts": self.leading_type_to_rider_to_counts,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bim-depot-lastrider-pie.html")]
struct BimDepotLastRiderPieTemplate {
    pub company_to_depot_to_rider_to_last_rides: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>>,
}
impl BimDepotLastRiderPieTemplate {
    pub fn json_data(&self) -> String {
        let value = serde_json::json!({
            "companyToDepotToRiderToLastRides": self.company_to_depot_to_rider_to_last_rides,
        });
        serde_json::to_string(&value)
            .expect("failed to JSON-encode graph data")
    }
}


pub(crate) async fn handle_bim_latest_rider_count_over_time_image(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let mut thicken = 1;
    if let Some(thicken_str) = query_pairs.get("thicken") {
        if let Ok(thicken_val) = thicken_str.parse() {
            thicken = thicken_val;
        }
    }

    let mut over_time = false;
    if let Some(over_time_str) = query_pairs.get("over-time") {
        if over_time_str == "1" {
            over_time = true;
        }
    }

    let company = query_pairs
        .get("company");

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let query = format!(
        "
            SELECT {}, rvto.old_rider, rvto.new_rider, rvto.\"timestamp\"
            FROM bim.ridden_vehicles_between_riders(FALSE) rvto
            {}
            ORDER BY rvto.\"timestamp\", rvto.id
        ",
        if over_time { "CAST(bim.to_transport_date(rvto.\"timestamp\") - CAST('2000-01-01' AS date) AS bigint)" } else { "rvto.id" },
        if company.is_some() { "WHERE rvto.company = $1" } else { "" },
    );
    const TRANSPORT_EPOCH: NaiveDate = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();

    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(1);
    if company.is_some() {
        params.push(&company);
    }

    let ride_res = db_conn.query(&query, &params).await;
    let ride_rows = match ride_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };

    let mut all_riders = HashSet::new();
    let mut ride_id_and_rider_to_latest_vehicle_count: Vec<(i64, HashMap<String, usize>)> = Vec::new();
    let mut prev_year = None;
    let mut graph_x_to_new_year = BTreeMap::new();
    for row in &ride_rows {
        let ride_id: i64 = row.get(0);
        let old_rider: Option<String> = row.get(1);
        let new_rider: String = row.get(2);
        let timestamp: DateTime<Local> = row.get(3);

        if let Some(or) = old_rider.as_ref() {
            all_riders.insert(or.clone());
        }
        all_riders.insert(new_rider.clone());

        let last_ride_id = ride_id_and_rider_to_latest_vehicle_count.last()
            .map(|(ride_id, _rtlvc)| *ride_id);
        if last_ride_id != Some(ride_id) {
            // different ride
            // clone to new entry (or create a completely new map)
            let new_rider_to_latest_vehicle_count = ride_id_and_rider_to_latest_vehicle_count.last()
                .map(|(_ride_id, rtlvc)| rtlvc.clone())
                .unwrap_or_else(|| HashMap::new());
            if over_time {
                if let Some(lrid) = last_ride_id {
                    // ensure ride IDs (actually day ordinals) are increasing
                    assert!(lrid < ride_id);
                    let mut walker_id = last_ride_id.unwrap() + 1;
                    while walker_id < ride_id {
                        ride_id_and_rider_to_latest_vehicle_count.push((walker_id, new_rider_to_latest_vehicle_count.clone()));
                        walker_id += 1;
                    }
                }
            }
            ride_id_and_rider_to_latest_vehicle_count.push((ride_id, new_rider_to_latest_vehicle_count));
        }
        let rider_to_latest_vehicle_count = &mut ride_id_and_rider_to_latest_vehicle_count
            .last_mut().unwrap()
            .1;

        if let Some(or) = &old_rider {
            let old_rider_count = rider_to_latest_vehicle_count
                .entry(or.clone())
                .or_insert(0);
            *old_rider_count -= 1;
        }
        let new_rider_count = rider_to_latest_vehicle_count
            .entry(new_rider.clone())
            .or_insert(0);
        *new_rider_count += 1;

        if !over_time {
            let cur_year = timestamp.year();
            if let Some(py) = prev_year {
                if py != cur_year {
                    // year changed!
                    graph_x_to_new_year.insert(ride_id_and_rider_to_latest_vehicle_count.len(), cur_year);
                }
            }
            prev_year = Some(cur_year);
        }
    }

    if over_time {
        let mut last_year = None;
        for (i, (date_ordinal, _)) in ride_id_and_rider_to_latest_vehicle_count.iter().enumerate(){
            let date_ordinal_u64: u64 = (*date_ordinal).try_into().unwrap();
            let date = TRANSPORT_EPOCH.checked_add_days(Days::new(date_ordinal_u64))
                .unwrap();
            let year = date.year();
            if last_year != Some(year) {
                graph_x_to_new_year.insert(i, year);
                last_year = Some(year);
            }
        }
    }

    let mut rider_names: Vec<String> = all_riders
        .iter()
        .map(|rn| rn.clone())
        .collect();
    rider_names.sort_unstable_by_key(|r| (r.to_lowercase(), r.clone()));

    if query_pairs.get("format").map(|f| f == "tsv").unwrap_or(false) {
        let mut tsv_output = String::new();
        let mut first_rider = true;

        for rider in &rider_names {
            if first_rider {
                first_rider = false;
            } else {
                tsv_output.push('\t');
            }
            tsv_output.push_str(rider);
        }

        for (_ride_id, rider_to_latest_vehicle_count) in &ride_id_and_rider_to_latest_vehicle_count {
            tsv_output.push('\n');

            let mut first_rider = true;
            for rider in &rider_names {
                if first_rider {
                    first_rider = false;
                } else {
                    tsv_output.push('\t');
                }
                let vehicle_count = rider_to_latest_vehicle_count
                    .get(rider)
                    .map(|vc| *vc)
                    .unwrap_or(0);
                write!(&mut tsv_output, "{}", vehicle_count).unwrap();
            }
        }

        let response_res = Response::builder()
            .header("Content-Type", "text/tab-separated-values; charset=utf-8")
            .body(Full::new(Bytes::from(tsv_output)));
        match response_res {
            Ok(r) => return Ok(r),
            Err(e) => {
                error!("failed to construct latest-rider-count-over-time-image TSV response: {}", e);
                return return_500();
            }
        }
    }

    // prepare the graph
    let ride_count = ride_id_and_rider_to_latest_vehicle_count.len();
    let max_count = ride_id_and_rider_to_latest_vehicle_count
        .iter()
        .flat_map(|(_ride_id, rtlvc)| rtlvc.values())
        .map(|val| *val)
        .max()
        .unwrap_or(0);
    let mut graph = LineGraph::new_for_ranges(ride_count, max_count, thicken);

    // draw the "start of new year" bars
    const YEAR_NUMBER_LEVELS: usize = 3;
    const YEAR_NUMBER_LEVEL_OFFSET: usize = 8;
    const YEAR_NUMBER_X_OFFSET: usize = 4;
    const YEAR_NUMBER_Y_OFFSET: usize = 4;
    let mut current_year_number_level = 0;
    for (graph_x, new_year) in &graph_x_to_new_year {
        // line
        graph.draw_time_subdivision(*graph_x);

        // year number text
        let new_year_string = new_year.to_string();
        graph.draw_string(
            *graph_x + YEAR_NUMBER_X_OFFSET,
            YEAR_NUMBER_Y_OFFSET + YEAR_NUMBER_LEVEL_OFFSET * current_year_number_level,
            &new_year_string,
        );
        current_year_number_level = (current_year_number_level + 1) % YEAR_NUMBER_LEVELS;
    }

    // draw the data
    for (graph_x, (_ride_id, rider_to_latest_vehicle_count)) in ride_id_and_rider_to_latest_vehicle_count.iter().enumerate() {
        for (i, rider) in rider_names.iter().enumerate() {
            let vehicle_count = rider_to_latest_vehicle_count
                .get(rider)
                .map(|vc| *vc)
                .unwrap_or(0);

            graph.draw_data_point(graph_x, vehicle_count, (i % GRAPH_COLORS.len()).try_into().unwrap());
        }
    }

    // gimme PNG
    let png_bytes = graph.to_png();

    let response_res = Response::builder()
        .header("Content-Type", "image/png")
        .body(Full::new(Bytes::from(png_bytes)));
    match response_res {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("failed to construct latest-rider-count-over-time-image response: {}", e);
            return return_500();
        }
    }
}


pub(crate) async fn handle_bim_latest_rider_count_over_time(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let company = query_pairs
        .get("company")
        .clowned();

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let riders_query = format!(
        "SELECT DISTINCT rider_username FROM bim.rides{}",
        if company.is_some() { " WHERE company = $1" } else { "" },
    );
    let mut riders_params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(1);
    if company.is_some() {
        riders_params.push(&company);
    }

    let riders_res = db_conn.query(&riders_query, &riders_params).await;
    let rider_rows = match riders_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query riders: {}", e);
            return return_500();
        },
    };

    let mut all_riders = HashSet::new();
    for row in &rider_rows {
        let rider_username: String = row.get(0);
        all_riders.insert(rider_username);
    }

    let mut rider_names: Vec<String> = all_riders
        .iter()
        .map(|rn| rn.clone())
        .collect();
    rider_names.sort_unstable_by_key(|r| (r.to_lowercase(), r.clone()));

    let mut riders: Vec<GraphRiderPart> = Vec::with_capacity(rider_names.len());
    for (i, rider_name) in rider_names.iter().enumerate() {
        riders.push(GraphRiderPart {
            name: rider_name.clone(),
            color: GRAPH_COLORS[i % GRAPH_COLORS.len()],
        });
    }

    let query = format!(
        "
            SELECT rvto.old_rider, rvto.new_rider, CAST(COUNT(*) AS bigint) pair_count
            FROM bim.ridden_vehicles_between_riders(FALSE) rvto
            WHERE rvto.old_rider IS NOT NULL {}
            GROUP BY rvto.old_rider, rvto.new_rider
        ",
        if company.is_some() { "AND rvto.company = $1" } else { "" },
    );

    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(1);
    if company.is_some() {
        params.push(&company);
    }

    let rides_res = db_conn.query(&query, &params).await;
    let ride_rows = match rides_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query riders: {}", e);
            return return_500();
        },
    };

    let mut from_to_count: BTreeMap<(String, String), u64> = BTreeMap::new();
    for ride_row in ride_rows {
        let from_rider: String = ride_row.get(0);
        let to_rider: String = ride_row.get(1);
        let pair_count: i64 = ride_row.get(2);

        let pair_count_u64: u64 = pair_count.try_into().unwrap();

        from_to_count.insert((from_rider, to_rider), pair_count_u64);
    }

    let template = BimLatestRiderCountTemplate {
        company,
        riders,
        from_to_count,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_fixed_monopolies_over_time(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // collect rides with vehicles in fixed couplings
    let rides_res = db_conn.query(
        "
            SELECT
                rav.id, rav.company, rav.rider_username, rav.\"timestamp\",
                rav.vehicle_number, rav.coupling_mode
            FROM
                bim.rides_and_vehicles rav
            WHERE
                rav.coupling_mode IN ('R', 'F')
                AND EXISTS (
                    SELECT 1
                    FROM bim.ride_vehicles rv
                    WHERE rv.ride_id = rav.id
                    AND rv.coupling_mode = 'F'
                )
            ORDER BY
                rav.\"timestamp\",
                rav.id,
                rav.spec_position,
                rav.fixed_coupling_position
        ",
        &[],
    ).await;
    let ride_rows = match rides_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };

    let mut known_rides = Vec::new();
    let mut current_ride: Option<MonopolyRide> = None;
    for row in &ride_rows {
        let ride_id: i64 = row.get(0);
        let company: String = row.get(1);
        let rider_username: String = row.get(2);
        let timestamp: DateTime<Local> = row.get(3);
        let vehicle_number: String = row.get(4);
        let coupling_mode_str: String = row.get(5);

        let coupling_mode = match CouplingMode::try_from_db_str(&coupling_mode_str) {
            Some(ct) => ct,
            None => {
                error!("invalid coupling mode {:?}; skipping row", coupling_mode_str);
                continue;
            },
        };

        let same_ride = current_ride.as_ref().map(|r| r.id == ride_id).unwrap_or(false);
        if !same_ride {
            let new_ride = MonopolyRide {
                id: ride_id,
                company,
                rider_username,
                timestamp,
                vehicles: Vec::new(),
            };
            let prev_current_ride = std::mem::replace(&mut current_ride, Some(new_ride));
            if let Some(pcr) = prev_current_ride {
                known_rides.push(pcr);
            }
        }

        current_ride.as_mut().unwrap().vehicles.push(MonopolyVehicle {
            vehicle_number,
            coupling_mode,
        });
    }

    if let Some(cr) = current_ride {
        known_rides.push(cr);
    }

    // run through the rides
    let mut company_to_vehicle_number_to_last_rider: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current_rider_to_monopolies = BTreeMap::new();
    let mut chrono_timestamp_to_rider_to_monopolies = BTreeMap::new();
    for ride in &known_rides {
        let previous_monopolist = ride.monopoly_rider_username(&company_to_vehicle_number_to_last_rider)
            .map(|m| m.to_owned());

        let vehicle_number_to_last_rider = company_to_vehicle_number_to_last_rider
            .entry(ride.company.clone())
            .or_insert_with(|| BTreeMap::new());
        for vehicle in &ride.vehicles {
            if vehicle.coupling_mode == CouplingMode::Ridden {
                vehicle_number_to_last_rider.insert(vehicle.vehicle_number.clone(), ride.rider_username.clone());
            }
        }

        let current_monopolist = ride.monopoly_rider_username(&company_to_vehicle_number_to_last_rider)
            .map(|m| m.to_owned());

        if previous_monopolist != current_monopolist {
            // monopoly change!
            if let Some(pm) = previous_monopolist {
                let prev_entry = current_rider_to_monopolies
                    .entry(pm)
                    .or_insert(MonopolyEntry {
                        count: 0,
                        points: 0,
                    });
                prev_entry.count -= 1;
                prev_entry.points -= ride.vehicles.len();
            }
            if let Some(nm) = current_monopolist {
                let new_entry = current_rider_to_monopolies
                    .entry(nm)
                    .or_insert(MonopolyEntry {
                        count: 0,
                        points: 0,
                    });
                new_entry.count += 1;
                new_entry.points += ride.vehicles.len();
            }

            chrono_timestamp_to_rider_to_monopolies.insert(ride.timestamp.clone(), current_rider_to_monopolies.clone());
        }
    }

    // collect all riders
    let mut all_riders = HashSet::new();
    for rider_to_monopolies in chrono_timestamp_to_rider_to_monopolies.values() {
        for rider in rider_to_monopolies.keys() {
            all_riders.insert(rider.clone());
        }
    }

    // fill missing riders
    for rider_to_monopolies in chrono_timestamp_to_rider_to_monopolies.values_mut() {
        for rider in &all_riders {
            rider_to_monopolies
                .entry(rider.clone())
                .or_insert(MonopolyEntry::default());
        }
    }

    let mut rider_to_timestamp_to_monopolies = BTreeMap::new();
    for (timestamp, rider_to_monopolies) in chrono_timestamp_to_rider_to_monopolies.into_iter() {
        let timestamp_string = timestamp.format("%Y-%m-%d %H:%M").to_string();
        for (rider, monopolies) in rider_to_monopolies.into_iter() {
            rider_to_timestamp_to_monopolies
                .entry(rider)
                .or_insert_with(|| BTreeMap::new())
                .insert(timestamp_string.clone(), monopolies);
        }
    }

    let template = FixedMonopoliesOverTime {
        rider_to_timestamp_to_monopolies,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_histogram_by_day_of_week(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let riders_res = db_conn.query(
        "
            SELECT
                rider_username,
                CAST(EXTRACT(DOW FROM bim.to_transport_date(\"timestamp\")) AS bigint) day_of_week,
                CAST(COUNT(*) AS bigint) count
            FROM
                bim.rides
            GROUP BY
                rider_username,
                day_of_week
        ",
        &[],
    ).await;
    let rider_rows = match riders_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query riders: {}", e);
            return return_500();
        },
    };
    let mut rider_to_weekday_counts: BTreeMap<String, [i64; 7]> = BTreeMap::new();
    for row in &rider_rows {
        let rider_username: String = row.get(0);
        let day_of_week_postgres: i64 = row.get(1);
        let ride_count: i64 = row.get(2);

        let day_of_week_graph: usize = if day_of_week_postgres == 0 {
            // Sunday
            6
        } else {
            (day_of_week_postgres - 1).try_into().expect("very unexpected weekday number")
        };

        let weekday_values = rider_to_weekday_counts
            .entry(rider_username)
            .or_insert_with(|| [0; 7]);
        weekday_values[day_of_week_graph] += ride_count;
    }

    let template = HistogramByDayOfWeekTemplate {
        rider_to_weekday_counts,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_histogram_by_vehicle_ride_count_group(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let mut bin_size: i64 = 10;
    if let Some(bin_size_str) = query_pairs.get("group-size") {
        match bin_size_str.parse() {
            Ok(bs) => {
                if bs <= 0 {
                    return return_400(
                        "group-size must be at least 1", &query_pairs
                    ).await
                }
                bin_size = bs;
            },
            Err(_) => return return_400(
                "group-size is not a valid 64-bit integer", &query_pairs
            ).await,
        }
    }
    let bin_size_usize: usize = match bin_size.try_into() {
        Ok(bs) => bs,
        Err(_) => return return_400(
            "group-size is not a valid unsigned native-sized integer", &query_pairs
        ).await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let riders_res = db_conn.query(
        "
            SELECT
                rider_username,
                company,
                vehicle_number,
                CAST(COUNT(*) AS bigint) count
            FROM
                bim.rides_and_vehicles
            GROUP BY
                rider_username,
                company,
                vehicle_number
        ",
        &[],
    ).await;
    let rider_rows = match riders_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };

    let mut rider_to_vehicle_to_ride_count: BTreeMap<String, BTreeMap<(String, String), i64>> = BTreeMap::new();
    for row in &rider_rows {
        let rider_username: String = row.get(0);
        let company: String = row.get(1);
        let vehicle_number: String = row.get(2);
        let ride_count: i64 = row.get(3);

        rider_to_vehicle_to_ride_count
            .entry(rider_username)
            .or_insert_with(|| BTreeMap::new())
            .insert((company, vehicle_number), ride_count);
    }

    let mut rider_to_bin_to_vehicle_count: BTreeMap<String, BTreeMap<usize, i64>> = BTreeMap::new();
    for (rider, vehicle_to_ride_count) in &rider_to_vehicle_to_ride_count {
        let bin_to_vehicle_count = rider_to_bin_to_vehicle_count
            .entry(rider.clone())
            .or_insert_with(|| BTreeMap::new());
        for ride_count in vehicle_to_ride_count.values() {
            let bin_index_i64 = *ride_count / bin_size;
            if bin_index_i64 < 0 {
                continue;
            }
            let bin_index: usize = bin_index_i64.try_into().unwrap();

            *bin_to_vehicle_count.entry(bin_index).or_insert(0) += 1;
        }
    }

    let max_bin_index = rider_to_bin_to_vehicle_count
        .values()
        .flat_map(|bin_to_count| bin_to_count.keys())
        .map(|count| *count)
        .max()
        .unwrap_or(0);

    let mut bin_names = Vec::with_capacity(max_bin_index + 1);
    for i in 0..(max_bin_index+1) {
        bin_names.push(format!("{}-{}", i*bin_size_usize, ((i+1)*bin_size_usize)-1));
    }

    let mut rider_to_group_counts: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for (rider, bin_to_count) in rider_to_bin_to_vehicle_count.iter() {
        let group_counts = rider_to_group_counts
            .entry(rider.clone())
            .or_insert_with(|| vec![0; max_bin_index+1]);
        for (bin, count) in bin_to_count.iter() {
            group_counts[*bin] += *count;
        }
    }

    let template = HistogramByRideCountGroupTemplate {
        what: "Vehicle".to_owned(),
        ride_count_group_names: bin_names,
        rider_to_group_counts,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_histogram_by_line_ride_count_group(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let mut bin_size: i64 = 10;
    if let Some(bin_size_str) = query_pairs.get("group-size") {
        match bin_size_str.parse() {
            Ok(bs) => {
                if bs <= 0 {
                    return return_400(
                        "group-size must be at least 1", &query_pairs
                    ).await
                }
                bin_size = bs;
            },
            Err(_) => return return_400(
                "group-size is not a valid 64-bit integer", &query_pairs
            ).await,
        }
    }
    let bin_size_usize: usize = match bin_size.try_into() {
        Ok(bs) => bs,
        Err(_) => return return_400(
            "group-size is not a valid unsigned native-sized integer", &query_pairs
        ).await,
    };

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let riders_res = db_conn.query(
        "
            SELECT
                rider_username,
                company,
                line,
                CAST(COUNT(*) AS bigint) count
            FROM
                bim.rides
            WHERE
                line IS NOT NULL
            GROUP BY
                rider_username,
                company,
                line
        ",
        &[],
    ).await;
    let rider_rows = match riders_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };

    let mut rider_to_line_to_ride_count: BTreeMap<String, BTreeMap<(String, String), i64>> = BTreeMap::new();
    for row in &rider_rows {
        let rider_username: String = row.get(0);
        let company: String = row.get(1);
        let line: String = row.get(2);
        let ride_count: i64 = row.get(3);

        rider_to_line_to_ride_count
            .entry(rider_username)
            .or_insert_with(|| BTreeMap::new())
            .insert((company, line), ride_count);
    }

    let mut rider_to_bin_to_line_count: BTreeMap<String, BTreeMap<usize, i64>> = BTreeMap::new();
    for (rider, line_to_ride_count) in &rider_to_line_to_ride_count {
        let bin_to_line_count = rider_to_bin_to_line_count
            .entry(rider.clone())
            .or_insert_with(|| BTreeMap::new());
        for ride_count in line_to_ride_count.values() {
            let bin_index_i64 = *ride_count / bin_size;
            if bin_index_i64 < 0 {
                continue;
            }
            let bin_index: usize = bin_index_i64.try_into().unwrap();

            *bin_to_line_count.entry(bin_index).or_insert(0) += 1;
        }
    }

    let max_bin_index = rider_to_bin_to_line_count
        .values()
        .flat_map(|bin_to_count| bin_to_count.keys())
        .map(|count| *count)
        .max()
        .unwrap_or(0);

    let mut bin_names = Vec::with_capacity(max_bin_index + 1);
    for i in 0..(max_bin_index+1) {
        bin_names.push(format!("{}-{}", i*bin_size_usize, ((i+1)*bin_size_usize)-1));
    }

    let mut rider_to_group_counts: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for (rider, bin_to_count) in rider_to_bin_to_line_count.iter() {
        let group_counts = rider_to_group_counts
            .entry(rider.clone())
            .or_insert_with(|| vec![0; max_bin_index+1]);
        for (bin, count) in bin_to_count.iter() {
            group_counts[*bin] += *count;
        }
    }

    let template = HistogramByRideCountGroupTemplate {
        what: "Line".to_owned(),
        ride_count_group_names: bin_names,
        rider_to_group_counts,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_last_rider_pie(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut company_to_type_to_rider_to_last_count: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>> = BTreeMap::new();
    let mut company_to_type_to_rider_to_last_count_ridden: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>> = BTreeMap::new();

    let conditions_maps = [
        ("", "", &mut company_to_type_to_rider_to_last_count),
        ("AND rav.coupling_mode = 'R'", "AND rav2.coupling_mode = 'R'", &mut company_to_type_to_rider_to_last_count_ridden),
    ];
    for (condition_rav, condition_rav2, map) in conditions_maps {
        let query_string = format!(
            "
                WITH last_riders(company, vehicle_number, vehicle_type, rider_username) AS (
                    SELECT
                        rav.company,
                        rav.vehicle_number,
                        rav.vehicle_type,
                        rav.rider_username
                    FROM
                        bim.rides_and_vehicles rav
                    WHERE
                        NOT EXISTS (
                            SELECT 1
                            FROM bim.rides_and_vehicles rav2
                            WHERE
                                rav2.company = rav.company
                                AND rav2.vehicle_number = rav.vehicle_number
                                {}
                                AND rav2.\"timestamp\" > rav.\"timestamp\"
                        )
                        {}
                        AND rav.vehicle_type IS NOT NULL
                )
                SELECT
                    lr.company,
                    lr.vehicle_type,
                    lr.rider_username,
                    CAST(COUNT(*) AS bigint)
                FROM
                    last_riders lr
                GROUP BY
                    lr.company,
                    lr.vehicle_type,
                    lr.rider_username
            ",
            condition_rav2,
            condition_rav,
        );
        let rider_rows = match db_conn.query(&query_string, &[]).await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query rides: {}", e);
                return return_500();
            },
        };

        for row in &rider_rows {
            let company: String = row.get(0);
            let vehicle_type: String = row.get(1);
            let rider_username: String = row.get(2);
            let ride_count: i64 = row.get(3);

            let count_per_rider = map
                .entry(company)
                .or_insert_with(|| BTreeMap::new())
                .entry(vehicle_type)
                .or_insert_with(|| BTreeMap::new())
                .entry(rider_username)
                .or_insert(0);
            *count_per_rider += ride_count;
        }
    }

    let template = LastRiderPieTemplate {
        company_to_type_to_rider_to_last_count,
        company_to_type_to_rider_to_last_count_ridden,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_histogram_fixed_coupling(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // obtain database extract
    let database_extract = obtain_vehicle_extract().await;
    if database_extract.company_to_vehicle_to_fixed_coupling.len() == 0 {
        return return_500();
    }

    let mut company_to_vehnum_to_rider_to_count: BTreeMap<String, BTreeMap<VehicleNumber, BTreeMap<String, i64>>> = BTreeMap::new();
    let mut company_to_vehnum_to_total_count: BTreeMap<String, BTreeMap<VehicleNumber, i64>> = BTreeMap::new();
    let rider_rows_res = db_conn.query(
        "
            SELECT
                rav.company,
                rav.vehicle_number,
                rav.rider_username,
                CAST(COUNT(*) AS bigint)
            FROM
                bim.rides_and_vehicles rav
            WHERE
                rav.coupling_mode = 'R'
            GROUP BY
                rav.company,
                rav.vehicle_number,
                rav.rider_username
        ",
        &[],
    ).await;
    let rider_rows = match rider_rows_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    for row in &rider_rows {
        let company: String = row.get(0);
        let vehicle_number_string: String = row.get(1);
        let rider_username: String = row.get(2);
        let ride_count: i64 = row.get(3);

        let vehicle_number = VehicleNumber::from(vehicle_number_string);

        let count_per_rider = company_to_vehnum_to_rider_to_count
            .entry(company.clone())
            .or_insert_with(|| BTreeMap::new())
            .entry(vehicle_number.clone())
            .or_insert_with(|| BTreeMap::new())
            .entry(rider_username)
            .or_insert(0);
        *count_per_rider += ride_count;

        let total_count = company_to_vehnum_to_total_count
            .entry(company.clone())
            .or_insert_with(|| BTreeMap::new())
            .entry(vehicle_number.clone())
            .or_insert(0);
        *total_count += ride_count;
    }

    let mut front_vehicle_type_to_rider_to_counts = BTreeMap::new();
    let empty_map = BTreeMap::new();
    for (company, vehicle_to_fixed_coupling) in &database_extract.company_to_vehicle_to_fixed_coupling {
        let Some(vehicle_to_type) = database_extract.company_to_vehicle_to_type.get(company) else { continue };
        let Some(vehicle_to_rider_to_count) = company_to_vehnum_to_rider_to_count.get(company) else { continue };
        let Some(vehicle_to_total_count) = company_to_vehnum_to_total_count.get(company) else { continue };
        for (look_vehicle, fixed_coupling) in vehicle_to_fixed_coupling {
            if fixed_coupling.len() == 0 {
                continue;
            }
            if look_vehicle != &fixed_coupling[0] {
                // only pass each fixed coupling once
                // (when we are looking at the front vehicle)
                continue;
            }
            let Some(front_vehicle_type) = vehicle_to_type.get(&fixed_coupling[0]) else { continue };

            let full_front_vehicle_type = format!("{}/{}", company, front_vehicle_type);
            let rider_to_counts = front_vehicle_type_to_rider_to_counts
                .entry(full_front_vehicle_type)
                .or_insert_with(|| BTreeMap::new());
            for (i, vehicle) in fixed_coupling.iter().enumerate() {
                let rider_to_count = vehicle_to_rider_to_count
                    .get(vehicle).unwrap_or(&empty_map);
                let total_count = vehicle_to_total_count
                    .get(vehicle).map(|tc| *tc).unwrap_or(0);

                let all_counts = rider_to_counts.entry("\u{18}".to_owned())
                    .or_insert_with(|| Vec::with_capacity(fixed_coupling.len()));
                while i >= all_counts.len() {
                    all_counts.push(0);
                }
                all_counts[i] += total_count;

                for (rider, count) in rider_to_count {
                    let this_count = rider_to_counts.entry(rider.clone())
                        .or_insert_with(|| Vec::with_capacity(fixed_coupling.len()));
                    while i >= this_count.len() {
                        this_count.push(0);
                    }
                    this_count[i] += *count;
                }
            }
        }
    }

    let template = HistogramFixedCouplingTemplate {
        front_vehicle_type_to_rider_to_counts,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_global_stats(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut company_to_total_rides: BTreeMap<String, i64> = BTreeMap::new();
    let company_rows_res = db_conn.query(
        "
            SELECT
                r.company,
                CAST(COUNT(*) AS bigint)
            FROM
                bim.rides r
            GROUP BY
                r.company
        ",
        &[],
    ).await;
    let company_rows = match company_rows_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut total_rides = 0;
    for row in &company_rows {
        let company: String = row.get(0);
        let ride_count: i64 = row.get(1);

        company_to_total_rides.insert(company, ride_count);
        total_rides += ride_count;
    }

    let template = GlobalStatsTemplate {
        total_rides,
        company_to_total_rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_first_rider_pie(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let company_rows_res = db_conn.query(
        "
            SELECT
                rrv.company,
                rrv.rider_username,
                COUNT(*) count
            FROM
                bim.rides_and_ridden_vehicles rrv
            WHERE
                NOT EXISTS (
                    SELECT 1
                    FROM bim.rides_and_ridden_vehicles rrv2
                    WHERE rrv2.company = rrv.company
                    AND rrv2.vehicle_number = rrv.vehicle_number
                    AND rrv2.\"timestamp\" < rrv.\"timestamp\"
                )
            GROUP BY
                rrv.company,
                rrv.rider_username
        ",
        &[],
    ).await;
    let company_rows = match company_rows_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut company_to_rider_to_first_rides: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut rider_to_total_first_rides: BTreeMap<String, i64> = BTreeMap::new();
    for row in &company_rows {
        let company: String = row.get(0);
        let rider_username: String = row.get(1);
        let ride_count: i64 = row.get(2);

        company_to_rider_to_first_rides
            .entry(company)
            .or_insert_with(|| BTreeMap::new())
            .insert(rider_username.clone(), ride_count);
        let rider_total_first_rides = rider_to_total_first_rides
            .entry(rider_username)
            .or_insert(0);
        *rider_total_first_rides += ride_count;
    }

    let template = FirstRiderPieTemplate {
        company_to_rider_to_first_rides,
        rider_to_total_first_rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_type_histogram(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let company_rows_res = db_conn.query(
        "
            SELECT
                rrv.company,
                rrv.vehicle_type,
                rrv.rider_username,
                COUNT(*) count
            FROM
                bim.rides_and_ridden_vehicles rrv
            WHERE
                rrv.vehicle_type IS NOT NULL
            GROUP BY
                rrv.company,
                rrv.vehicle_type,
                rrv.rider_username
        ",
        &[],
    ).await;
    let company_rows = match company_rows_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut company_to_vehicle_type_to_rider_to_count: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>> = BTreeMap::new();
    let mut company_to_vehicle_type_to_count: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for row in &company_rows {
        let company: String = row.get(0);
        let vehicle_type: String = row.get(1);
        let rider_username: String = row.get(2);
        let ride_count: i64 = row.get(3);

        company_to_vehicle_type_to_rider_to_count
            .entry(company.clone()).or_insert_with(|| BTreeMap::new())
            .entry(vehicle_type.clone()).or_insert_with(|| BTreeMap::new())
            .insert(rider_username, ride_count);
        let type_rides = company_to_vehicle_type_to_count
            .entry(company).or_insert_with(|| BTreeMap::new())
            .entry(vehicle_type).or_insert(0);
        *type_rides += ride_count;
    }

    let template = TypeHistogramTemplate {
        company_to_vehicle_type_to_rider_to_count,
        company_to_vehicle_type_to_count,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_last_rider_histogram_by_fixed_pos(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let company_rows_res = db_conn.query(
        "
            WITH company_typed_vehicles(ride_id, \"timestamp\", company, company_type, rider_username, vehicle_number, coupling_mode, fixed_coupling_position) AS (
                SELECT
                    rav.id,
                    rav.\"timestamp\",
                    rav.company,
                    rav.company || '/' || rav.vehicle_type,
                    rav.rider_username,
                    rav.vehicle_number,
                    rav.coupling_mode,
                    rav.fixed_coupling_position
                FROM
                    bim.rides_and_vehicles rav
            )
            SELECT
                lv.company_type,
                ctv.rider_username,
                ctv.fixed_coupling_position,
                CAST(COUNT(*) AS bigint) last_rider_in_vehicle_count
            FROM
                company_typed_vehicles ctv
                INNER JOIN company_typed_vehicles lv -- leading vehicle
                    ON lv.ride_id = ctv.ride_id
                    AND lv.fixed_coupling_position = 0
            WHERE
                ctv.coupling_mode = 'R'
                AND EXISTS (
                    -- this is a fixed coupling
                    SELECT 1
                    FROM company_typed_vehicles ctv2
                    WHERE ctv2.ride_id = ctv.ride_id
                    AND ctv2.fixed_coupling_position = 1
                )
                AND NOT EXISTS (
                    -- this is the last ride in this vehicle
                    SELECT 1
                    FROM company_typed_vehicles ctv3
                    WHERE ctv3.company = ctv.company
                    AND ctv3.vehicle_number = ctv.vehicle_number
                    AND ctv3.\"timestamp\" > ctv.\"timestamp\"
                    AND ctv3.coupling_mode = 'R'
                )
            GROUP BY
                lv.company_type,
                ctv.rider_username,
                ctv.fixed_coupling_position
        ",
        &[],
    ).await;
    let company_rows = match company_rows_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut leading_type_to_rider_to_counts: BTreeMap<String, BTreeMap<String, Vec<i64>>> = BTreeMap::new();
    for row in &company_rows {
        let leading_type: String = row.get(0);
        let rider_username: String = row.get(1);
        let coupling_position: i64 = row.get(2);
        let ride_count: i64 = row.get(3);

        let coupling_position_usize: usize = match coupling_position.try_into() {
            Ok(cpu) => cpu,
            Err(_) => continue,
        };

        let counts = leading_type_to_rider_to_counts
            .entry(leading_type.clone()).or_insert_with(|| BTreeMap::new())
            .entry(rider_username.clone()).or_insert_with(|| Vec::new());
        while counts.len() <= coupling_position_usize {
            counts.push(0);
        }
        counts[coupling_position_usize] = ride_count;
    }

    let template = LastRiderHistogramByFixedPosTemplate {
        leading_type_to_rider_to_counts,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

pub(crate) async fn handle_bim_depot_last_rider_pie(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    const NO_DEPOT_KEY: &str = "\u{18}";

    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let Some(db_conn) = connect_to_db().await else {
        return return_500()
    };
    let Some(mut company_to_definition) = obtain_company_to_definition().await else {
        return return_500()
    };
    company_to_definition.retain(|_company, definition| !definition["bim_database_path"].is_null());
    let Some(company_to_opt_database) = obtain_company_to_bim_database(&company_to_definition) else {
        return return_500()
    };

    // only take companies that have a vehicle database where at least one depot is known
    let company_to_database: BTreeMap<_, _> = company_to_opt_database.into_iter()
        .filter_map(|(company, database)| database.map(|d| (company, d)))
        .filter(|(_company, database)| database.values().any(|v| v.depot.is_some()))
        .collect();

    let last_rider_stmt_res = db_conn.prepare(
        "
            SELECT
                rrv.vehicle_number,
                rrv.rider_username
            FROM
                bim.rides_and_ridden_vehicles rrv
            WHERE
                rrv.company = $1
                AND NOT EXISTS (
                    SELECT 1
                    FROM bim.rides_and_ridden_vehicles rrv2
                    WHERE rrv2.company = rrv.company
                    AND rrv2.vehicle_number = rrv.vehicle_number
                    AND rrv2.\"timestamp\" < rrv.\"timestamp\"
                )
        ",
    ).await;
    let last_rider_stmt = match last_rider_stmt_res {
        Ok(lrs) => lrs,
        Err(e) => {
            error!("failed to prepare query statement: {}", e);
            return return_500();
        },
    };

    let mut company_to_depot_to_rider_to_last_rides: BTreeMap<String, BTreeMap<String, BTreeMap<String, i64>>> = BTreeMap::new();
    for (company, vehicle_database) in &company_to_database {
        let last_rider_rows_res = db_conn.query(&last_rider_stmt, &[company]).await;
        let last_rider_rows = match last_rider_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query rides: {}", e);
                return return_500();
            },
        };
        for row in &last_rider_rows {
            let vehicle_number_raw: String = row.get(0);
            let rider_username: String = row.get(1);

            let vehicle_number = VehicleNumber::from_string(vehicle_number_raw);

            let Some(vehicle_entry) = vehicle_database.get(&vehicle_number) else {
                continue
            };
            let depot = vehicle_entry.depot.clone().unwrap_or_else(|| NO_DEPOT_KEY.to_owned());
            let is_active = vehicle_entry.in_service_since.is_some() && vehicle_entry.out_of_service_since.is_none();
            if !is_active {
                continue;
            }

            let last_vehicle_count = company_to_depot_to_rider_to_last_rides
                .entry(company.clone())
                .or_insert_with(|| BTreeMap::new())
                .entry(depot)
                .or_insert_with(|| BTreeMap::new())
                .entry(rider_username)
                .or_insert(0);
            *last_vehicle_count += 1;
        }
    }

    let template = BimDepotLastRiderPieTemplate {
        company_to_depot_to_rider_to_last_rides,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
