use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Infallible;

use askama::Template;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use rocketbot_bim_common::{VehicleInfo, VehicleNumber};
use serde::Serialize;
use tokio_postgres::types::ToSql;
use tracing::error;

use crate::{get_query_pairs, render_response, return_400, return_405, return_500};
use crate::bim::{connect_to_db, obtain_company_to_bim_database, obtain_company_to_definition};
use crate::templating::filters;
use crate::util::sort_as_text;


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CoverageCompany {
    pub uncoupled_type_to_block_name_to_vehicles: BTreeMap<String, BTreeMap<String, Vec<CoverageVehiclePart>>>,
    pub coupled_sequences: Vec<Vec<CoverageVehiclePart>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct CoverageVehiclePart {
    pub block_str: String,
    pub number_str: String,
    pub type_code: String,
    pub full_number_str: String,
    pub is_active: bool,
    pub ride_count: i64,
    pub everybody_ride_count: i64,
}
impl CoverageVehiclePart {
    pub fn has_ride(&self) -> bool {
        self.ride_count > 0
    }

    pub fn has_everybody_ride(&self) -> bool {
        self.everybody_ride_count > 0
    }

    pub fn from_vehicle_info(
        vehicle: &VehicleInfo,
        ridden_vehicles: &HashMap<VehicleNumber, i64>,
        all_riders_ridden_vehicles: &HashMap<VehicleNumber, i64>,
        use_number_blocks: bool,
    ) -> Self {
        let full_number_str = vehicle.number.to_string();
        let (block_str, number_str) = if use_number_blocks && full_number_str.len() >= 6 {
            full_number_str.split_at(4)
        } else {
            ("", full_number_str.as_str())
        };

        let from_known = vehicle.in_service_since.is_some();
        let to_known = vehicle.out_of_service_since.is_some();
        let is_active = from_known && !to_known;
        let ride_count = ridden_vehicles.get(&vehicle.number)
            .map(|c| *c)
            .unwrap_or(0);
        let everybody_ride_count = all_riders_ridden_vehicles.get(&vehicle.number)
            .map(|c| *c)
            .unwrap_or(0);

        Self {
            block_str: block_str.to_owned(),
            number_str: number_str.to_owned(),
            type_code: vehicle.type_code.clone(),
            full_number_str: full_number_str.clone(),
            is_active,
            ride_count,
            everybody_ride_count,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum MergeMode {
    SplitTypes,
    MergeTypes,
    MergeTypesGroupFixedCoupling,
}
impl MergeMode {
    #[inline]
    pub const fn merge_types(&self) -> bool {
        match self {
            Self::SplitTypes => false,
            Self::MergeTypes => true,
            Self::MergeTypesGroupFixedCoupling => true,
        }
    }

    pub fn try_from_str(s: &str) -> Option<MergeMode> {
        match s {
            "S" => Some(Self::SplitTypes),
            "M" => Some(Self::MergeTypes),
            "F" => Some(Self::MergeTypesGroupFixedCoupling),
            _ => None,
        }
    }
}
impl Default for MergeMode {
    fn default() -> Self { Self::SplitTypes }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimcoverage.html")]
struct BimCoverageTemplate {
    pub max_ride_count: i64,
    pub everybody_max_ride_count: i64,
    pub name_to_company: BTreeMap<String, CoverageCompany>,
    pub merge_mode: MergeMode,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimcoverage-pickrider.html")]
struct BimCoveragePickRiderTemplate {
    pub riders: Vec<String>,
    pub countries: Vec<String>,
}


#[inline]
fn cow_empty_to_none<'a, 'b>(val: Option<&'a Cow<'b, str>>) -> Option<&'a Cow<'b, str>> {
    match val {
        None => None,
        Some(x) => if x.len() > 0 { Some(x) } else { None },
    }
}

async fn get_company_to_vehicles_ridden(
    db_conn: &tokio_postgres::Client,
    to_date_opt: Option<DateTime<Local>>,
    rider_username_opt: Option<&str>,
    ridden_only: bool,
) -> Option<(HashMap<String, HashMap<VehicleNumber, i64>>, i64)> {
    let mut conditions: Vec<String> = Vec::with_capacity(3);
    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(2);

    if let Some(to_date) = to_date_opt.as_ref() {
        conditions.push(format!("\"timestamp\" <= ${}", conditions.len() + 1));
        params.push(to_date);
    }

    if let Some(rider_username) = rider_username_opt.as_ref() {
        conditions.push(format!("rider_username = ${}", conditions.len() + 1));
        params.push(rider_username);
    }

    if ridden_only {
        conditions.push("coupling_mode = 'R'".to_owned());
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
            return None;
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
    Some((company_to_vehicles_ridden, max_ride_count))
}

async fn get_company_to_vehicles_is_last_rider(
    db_conn: &tokio_postgres::Client,
    to_date_opt: Option<DateTime<Local>>,
    rider_username: &str,
    ridden_only: bool,
    query_everyone: bool,
) -> Option<(HashMap<String, HashMap<VehicleNumber, i64>>, i64)> {
    let mut inner_conditions: Vec<String> = Vec::with_capacity(1);
    let mut conditions: Vec<String> = Vec::with_capacity(3);
    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(3);

    if let Some(to_date) = to_date_opt.as_ref() {
        inner_conditions.push(format!("AND rav2.\"timestamp\" <= ${}", params.len() + 1));
        conditions.push(format!("AND rav.\"timestamp\" <= ${}", params.len() + 1));
        params.push(to_date);
    }

    if ridden_only {
        inner_conditions.push("AND rav2.coupling_mode = 'R'".to_owned());
        conditions.push("AND rav.coupling_mode = 'R'".to_owned());
    }

    let inner_conditions_string = inner_conditions.join(" ");
    let conditions_string = conditions.join(" ");

    let query = if query_everyone {
        let query = format!(
            "
                SELECT
                    rav.company,
                    rav.vehicle_number,
                    CAST(CASE WHEN EXISTS (
                        SELECT 1
                        FROM bim.rides_and_vehicles rav2
                        WHERE rav2.company = rav.company
                        AND rav2.vehicle_number = rav.vehicle_number
                        AND rav2.rider_username = ${}
                        {}
                    ) THEN 1 ELSE 3 END AS bigint) count_value
                FROM bim.rides_and_vehicles rav
                WHERE 1=1
                {}
            ",
            params.len() + 1,
            inner_conditions_string,
            conditions_string,
        );
        params.push(&rider_username);
        query
    } else {
        let query = format!(
            "
                SELECT rav.company, rav.vehicle_number, CAST(1 AS bigint) count_value
                FROM bim.rides_and_vehicles rav
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM bim.rides_and_vehicles rav2
                    WHERE rav2.company = rav.company
                    AND rav2.vehicle_number = rav.vehicle_number
                    AND rav2.\"timestamp\" > rav.\"timestamp\"
                    {}
                )
                AND rav.rider_username = ${}
                {}
            ",
            inner_conditions_string,
            params.len() + 1,
            conditions_string,
        );
        params.push(&rider_username);
        query
    };

    // get ridden vehicles for rider
    let vehicles_res = db_conn.query(&query, &params).await;
    let vehicle_rows = match vehicles_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query vehicles: {}", e);
            return None;
        },
    };
    let mut company_to_vehicles_ridden: HashMap<String, HashMap<VehicleNumber, i64>> = HashMap::new();
    for vehicle_row in vehicle_rows {
        let company: String = vehicle_row.get(0);
        let vehicle_number = VehicleNumber::from_string(vehicle_row.get(1));
        let count_value: i64 = vehicle_row.get(2);
        company_to_vehicles_ridden
            .entry(company)
            .or_insert_with(|| HashMap::new())
            .insert(vehicle_number, count_value);
    }

    Some((company_to_vehicles_ridden, 4))
}


pub(crate) async fn handle_bim_coverage(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let merge_mode = query_pairs.get("merge-mode")
        .map(|qp| MergeMode::try_from_str(qp))
        .flatten()
        .unwrap_or(MergeMode::SplitTypes);
    let hide_inactive = query_pairs.get("hide-inactive")
        .map(|qp| qp == "1")
        .unwrap_or(false);
    let compare_mode = query_pairs.get("compare")
        .map(|qp| qp == "1")
        .unwrap_or(false);
    let ridden_only = query_pairs.get("ridden-only")
        .map(|qp| qp == "1")
        .unwrap_or(false);
    let last_rider = query_pairs.get("last-rider")
        .map(|qp| qp == "1")
        .unwrap_or(false);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    if let Some(rider_name) = query_pairs.get("rider") {
        let rider_username_opt = if rider_name == "!ALL" {
            None
        } else {
            Some(rider_name.as_ref())
        };
        let country_code_opt = query_pairs.get("country");

        if last_rider && rider_username_opt.is_none() {
            return return_400("last-rider mode requires a specific rider to be chosen", &query_pairs).await;
        }

        let mut to_date_opt: Option<DateTime<Local>> = None;
        if let Some(date_str) = cow_empty_to_none(query_pairs.get("to-date")) {
            let input_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => return return_400("invalid date format, expected yyyy-mm-dd", &query_pairs).await,
            };

            // end of that day is actually next day at 04:00
            let naive_timestamp = input_date
                .succ_opt().unwrap()
                .and_hms_opt(4, 0, 0).unwrap();
            to_date_opt = match Local.from_local_datetime(&naive_timestamp).earliest() {
                Some(lts) => Some(lts),
                None => return return_400("failed to convert timestamp to local time", &query_pairs).await,
            };
        }
        let query_res = if last_rider {
            get_company_to_vehicles_is_last_rider(
                &db_conn,
                to_date_opt,
                rider_username_opt.unwrap(),
                ridden_only,
                false,
            ).await
        } else {
            get_company_to_vehicles_ridden(
                &db_conn,
                to_date_opt,
                rider_username_opt,
                ridden_only,
            ).await
        };
        let (company_to_vehicles_ridden, max_ride_count) = match query_res {
            Some(val) => val,
            None => return return_500(),
        };

        let (all_riders_company_to_vehicles_ridden, everybody_max_ride_count) = if compare_mode {
            // get ridden vehicles for all riders
            let query_res = if last_rider {
                get_company_to_vehicles_is_last_rider(
                    &db_conn,
                    to_date_opt,
                    rider_username_opt.unwrap(),
                    ridden_only,
                    true,
                ).await
            } else {
                get_company_to_vehicles_ridden(
                    &db_conn,
                    to_date_opt,
                    None,
                    ridden_only,
                ).await
            };
            match query_res {
                Some(val) => val,
                None => return return_500(),
            }
        } else {
            (HashMap::new(), 0)
        };

        // get company definitions
        let mut company_to_definition = match obtain_company_to_definition().await {
            Some(ctd) => ctd,
            None => return return_500(),
        };

        // drop those that don't match the country
        if let Some(country_code) = country_code_opt {
            company_to_definition.retain(|_name, definition|
                definition["country"]
                    .as_str()
                    .map(|def_country| def_country == country_code)
                    .unwrap_or(true) // keep companies where no country is set
            );
        }

        let company_to_bim_database_opts = match obtain_company_to_bim_database(&company_to_definition) {
            Some(ctbdb) => ctbdb,
            None => return return_500(),
        };
        let company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, VehicleInfo>> = company_to_bim_database_opts.iter()
            .filter_map(|(comp, db_opt)| {
                if let Some(db) = db_opt.as_ref() {
                    Some((comp.clone(), db.clone()))
                } else {
                    None
                }
            })
            .collect();

        // run through vehicles
        let mut name_to_company: BTreeMap<String, CoverageCompany> = BTreeMap::new();
        let no_ridden_vehicles = HashMap::new();
        for (company, number_to_vehicle) in &company_to_bim_database {
            let ridden_vehicles = company_to_vehicles_ridden.get(company)
                .unwrap_or(&no_ridden_vehicles);
            let all_riders_ridden_vehicles = all_riders_company_to_vehicles_ridden.get(company)
                .unwrap_or(&no_ridden_vehicles);

            let mut uncoupled_type_to_block_name_to_vehicles: BTreeMap<String, BTreeMap<String, Vec<CoverageVehiclePart>>> = BTreeMap::new();
            for vehicle in number_to_vehicle.values() {
                if merge_mode == MergeMode::MergeTypesGroupFixedCoupling {
                    if vehicle.fixed_coupling.len() > 0 {
                        // we handle vehicles with fixed couplings later
                        continue;
                    }
                }

                let vehicle_data = CoverageVehiclePart::from_vehicle_info(
                    vehicle,
                    ridden_vehicles,
                    all_riders_ridden_vehicles,
                    true,
                );

                if hide_inactive && !vehicle_data.is_active && vehicle_data.ride_count == 0 {
                    continue;
                }

                let type_code_key = if merge_mode == MergeMode::SplitTypes {
                    vehicle.type_code.clone()
                } else {
                    String::new()
                };

                uncoupled_type_to_block_name_to_vehicles
                    .entry(type_code_key)
                    .or_insert_with(|| BTreeMap::new())
                    .entry(vehicle_data.block_str.clone())
                    .or_insert_with(|| Vec::new())
                    .push(vehicle_data);
            }

            let coupled_sequences: Vec<Vec<CoverageVehiclePart>> = if merge_mode == MergeMode::MergeTypesGroupFixedCoupling {
                // now, handle all the fixed couplings
                let mut fixed_coupling_to_vehicles = BTreeMap::new();
                for vehicle in number_to_vehicle.values() {
                    if vehicle.fixed_coupling.len() == 0 {
                        // vehicles without fixed couplings were already handled
                        continue;
                    }

                    let fixed_coupling: Vec<VehicleNumber> = vehicle.fixed_coupling.iter()
                        .map(|nss| nss.clone())
                        .collect();
                    if fixed_coupling_to_vehicles.contains_key(&fixed_coupling) {
                        // we've already done this one
                        continue;
                    }

                    let coupling_vehicles: Vec<VehicleInfo> = fixed_coupling.iter()
                        .filter_map(|vn| number_to_vehicle.get(vn))
                        .map(|v| v.clone())
                        .collect();

                    fixed_coupling_to_vehicles.insert(fixed_coupling, coupling_vehicles);
                }

                let mut sequences: Vec<Vec<_>> = fixed_coupling_to_vehicles.values()
                    .map(|vehicles|
                        vehicles.into_iter()
                            .map(|vehicle| CoverageVehiclePart::from_vehicle_info(
                                vehicle,
                                ridden_vehicles,
                                all_riders_ridden_vehicles,
                                false,
                            ))
                            .collect()
                    )
                    .collect();

                if hide_inactive {
                    sequences.retain(|vehicles|
                        vehicles.iter()
                            .any(|veh| veh.is_active || veh.ride_count > 0)
                    );
                }

                sequences
            } else {
                Vec::with_capacity(0)
            };

            name_to_company.insert(
                company.clone(),
                CoverageCompany {
                    uncoupled_type_to_block_name_to_vehicles,
                    coupled_sequences,
                },
            );
        }

        let template = BimCoverageTemplate {
            max_ride_count,
            everybody_max_ride_count,
            name_to_company,
            merge_mode,
        };
        match render_response(&template, &query_pairs, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        // obtain countries
        let company_to_definition = match obtain_company_to_definition().await {
            Some(ctd) => ctd,
            None => return return_500(),
        };
        let mut countries_set = HashSet::new();
        for company_definition in company_to_definition.values() {
            let country = match company_definition["country"].as_str() {
                Some(c) => c,
                None => continue,
            };
            countries_set.insert(country.to_owned());
        }
        let mut countries: Vec<String> = countries_set.into_iter().collect();
        sort_as_text(&mut countries);

        // list riders
        let riders_res = db_conn.query("SELECT DISTINCT rider_username FROM bim.rides", &[]).await;
        let rider_rows = match riders_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query riders: {}", e);
                return return_500();
            },
        };
        let mut riders_set: HashSet<String> = HashSet::new();
        for rider_row in rider_rows {
            let rider: String = rider_row.get(0);
            riders_set.insert(rider);
        }
        let mut riders: Vec<String> = riders_set.into_iter().collect();
        sort_as_text(&mut riders);

        let template = BimCoveragePickRiderTemplate {
            riders,
            countries,
        };
        match render_response(&template, &query_pairs, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}

pub(crate) async fn handle_bim_coverage_field(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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

    let company_to_bim_database_opt = obtain_company_to_definition().await
        .as_ref()
        .and_then(|ctd| obtain_company_to_bim_database(ctd));
    let company_to_bim_database_opts = match company_to_bim_database_opt {
        Some(ctbdb) => ctbdb,
        None => return return_500(),
    };
    let mut company_to_bim_database: BTreeMap<String, BTreeMap<VehicleNumber, VehicleInfo>> = BTreeMap::new();
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
        pixels.push(vehicles.contains(&vehicle.number));
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

    let body = Full::new(Bytes::from(png_bytes));
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
