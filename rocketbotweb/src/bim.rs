use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::{self, Write};
use std::fs::File;
use std::str::FromStr;

use askama::Template;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use form_urlencoded;
use hyper::{Body, Method, Request, Response};
use log::{error, warn};
use png;
use rocketbot_bim_common::VehicleInfo;
use rocketbot_bim_common::achievements::{AchievementDef, ACHIEVEMENT_DEFINITIONS};
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


const CHART_COLORS: [[u8; 3]; 7] = [
    [54, 162, 235],
    [255, 99, 132],
    [255, 159, 64],
    [255, 205, 86],
    [75, 192, 192],
    [153, 102, 255],
    [201, 203, 207],
];
const CHART_BORDER_COLOR: [u8; 3] = [0, 0, 0];
const CHART_BACKGROUND_COLOR: [u8; 3] = [255, 255, 255];
const CHART_TICK_COLOR: [u8; 3] = [221, 221, 221];


type VehicleNumber = NatSortedString;


/// Specifies whether a vehicle has actually been ridden or was simply coupled to one that was ridden.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CouplingMode {
    /// Explicitly specified and actually ridden.
    Ridden,

    /// Explicitly specified but only coupled to the vehicle actually ridden.
    Explicit,

    /// Not specified, but fixed-coupled to the vehicle actually ridden.
    FixedCoupling,
}
impl CouplingMode {
    pub fn try_from_db_str(db_str: &str) -> Option<Self> {
        match db_str {
            "R" => Some(Self::Ridden),
            "E" => Some(Self::Explicit),
            "F" => Some(Self::FixedCoupling),
            _ => None,
        }
    }
}
impl fmt::Display for CouplingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ridden => write!(f, "ridden"),
            Self::Explicit => write!(f, "explicit"),
            Self::FixedCoupling => write!(f, "coupled"),
        }
    }
}


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
    pub everybody_max_ride_count: i64,
    pub name_to_company: BTreeMap<String, CoverageCompany>,
    pub merge_mode: MergeMode,
}

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

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bimcoverage-pickrider.html")]
struct BimCoveragePickRiderTemplate {
    pub riders: BTreeSet<String>,
    pub countries: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimdetails.html")]
struct BimDetailsTemplate {
    pub company: String,
    pub vehicle: Option<VehicleInfo>,
    pub rides: Vec<BimDetailsRidePart>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimachievements.html")]
struct BimAchievementsTemplate {
    pub achievement_to_rider_to_timestamp: HashMap<i64, HashMap<String, DateTimeLocalWithWeekday>>,
    pub all_achievements: Vec<AchievementDef>,
    pub all_riders: BTreeSet<String>,
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
    pub coupling_mode: CouplingMode,
    pub fixed_coupling_position: i64,
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
    pub line: NatSortedString,
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
    pub coupling_mode: CouplingMode,
    pub fixed_coupling_position: i64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ChartColor {
    Background,
    Border,
    Tick,
    Data(u8),
}
impl ChartColor {
    #[inline]
    pub fn palette_index(&self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Border => 1,
            Self::Tick => 2,
            Self::Data(d) => d.checked_add(3).unwrap(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Template)]
#[template(path = "bimlatestridercount.html")]
struct BimLatestRiderCountTemplate {
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

#[inline]
fn cow_empty_to_none<'a, 'b>(val: Option<&'a Cow<'b, str>>) -> Option<&'a Cow<'b, str>> {
    match val {
        None => None,
        Some(x) => if x.len() > 0 { Some(x) } else { None },
    }
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
    rider_username_opt: Option<&str>,
    ridden_only: bool,
) -> Option<(HashMap<String, HashMap<VehicleNumber, i64>>, i64)> {
    let mut inner_conditions: Vec<String> = Vec::with_capacity(1);
    let mut conditions: Vec<String> = Vec::with_capacity(3);
    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(3);

    if let Some(rider_username) = rider_username_opt.as_ref() {
        conditions.push(format!("AND rav.rider_username = ${}", params.len() + 1));
        params.push(rider_username);
    }

    if let Some(to_date) = to_date_opt.as_ref() {
        inner_conditions.push(format!("AND rav2.\"timestamp\" <= ${}", params.len() + 1));
        params.push(to_date);
        conditions.push(format!("AND rav.\"timestamp\" <= ${}", params.len() + 1));
        params.push(to_date);
    }

    if ridden_only {
        conditions.push("AND coupling_mode = 'R'".to_owned());
    }

    let inner_conditions_string = conditions.join(" ");
    let conditions_string = conditions.join(" ");

    let query = format!(
        "
            SELECT rav.company, rav.vehicle_number
            FROM bim.rides_and_vehicles rav
            WHERE NOT EXISTS (
                SELECT 1
                FROM bim.rides_and_vehicles rav2
                WHERE rav2.company = rav.company
                AND rav2.vehicle_number = rav.vehicle_number
                AND rav2.\"timestamp\" > rav.\"timestamp\"
                {}
            )
            {}
        ",
        inner_conditions_string,
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
    for vehicle_row in vehicle_rows {
        let company: String = vehicle_row.get(0);
        let vehicle_number = VehicleNumber::from_string(vehicle_row.get(1));
        company_to_vehicles_ridden
            .entry(company)
            .or_insert_with(|| HashMap::new())
            .insert(vehicle_number, 1);
    }

    // use 2 as the max value to lead to lighter colors in the web interface
    Some((company_to_vehicles_ridden, 2))
}

pub(crate) async fn handle_bim_coverage(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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
                rider_username_opt,
                ridden_only,
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
                    None,
                    ridden_only,
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

                fixed_coupling_to_vehicles.values()
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
                    .collect()
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
        let mut countries = BTreeSet::new();
        for company_definition in company_to_definition.values() {
            let country = match company_definition["country"].as_str() {
                Some(c) => c,
                None => continue,
            };
            countries.insert(country.to_owned());
        }

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
            countries,
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

pub(crate) async fn handle_explorer_bims(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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
                    WHERE rav.fixed_coupling_position = 0
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
                WHERE rav.fixed_coupling_position = 0
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


pub(crate) async fn handle_bim_latest_rider_count_over_time_image(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let ride_res = db_conn.query(
        "
            SELECT
                rav.id, rav.company, rav.vehicle_number, rav.rider_username
            FROM bim.rides_and_vehicles rav
            ORDER BY
                rav.\"timestamp\", rav.id
        ",
        &[],
    ).await;
    let ride_rows = match ride_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };

    let mut all_riders = HashSet::new();
    let mut vehicle_to_latest_rider: HashMap<(String, String), String> = HashMap::new();
    let mut ride_id_to_rider_to_latest_vehicle_count: HashMap<i64, HashMap<String, usize>> = HashMap::new();
    let mut ride_ids_in_order: Vec<i64> = Vec::new();
    for row in &ride_rows {
        let ride_id: i64 = row.get(0);
        let company: String = row.get(1);
        let vehicle_number: String = row.get(2);
        let rider_username: String = row.get(3);

        all_riders.insert(rider_username.clone());

        if ride_ids_in_order.last() != Some(&ride_id) {
            ride_ids_in_order.push(ride_id);
        }

        vehicle_to_latest_rider.insert((company, vehicle_number), rider_username);

        let rider_to_latest_vehicle_count = ride_id_to_rider_to_latest_vehicle_count
            .entry(ride_id)
            .or_insert_with(|| HashMap::new());

        // reset all numbers -- only keep the last entry per ride ID
        for rider_username in &all_riders {
            rider_to_latest_vehicle_count.insert(rider_username.clone(), 0);
        }
        for latest_rider in vehicle_to_latest_rider.values() {
            *rider_to_latest_vehicle_count.get_mut(latest_rider).unwrap() += 1;
        }
    }

    let ride_count = ride_ids_in_order.len();

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

        for ride_id in &ride_ids_in_order {
            tsv_output.push('\n');

            let rider_to_latest_vehicle_count = ride_id_to_rider_to_latest_vehicle_count
                .get(ride_id).unwrap();

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
            .body(Body::from(tsv_output));
        match response_res {
            Ok(r) => return Ok(r),
            Err(e) => {
                error!("failed to construct latest-rider-count-over-time-image TSV response: {}", e);
                return return_500();
            }
        }
    }

    let max_count = ride_id_to_rider_to_latest_vehicle_count
        .values()
        .flat_map(|rtlvc_row| rtlvc_row.values())
        .map(|val| *val)
        .max()
        .unwrap_or(0);
    let max_count_with_headroom = if max_count % 100 > 75 {
        // 80 -> 200
        ((max_count / 100) + 2) * 100
    } else {
        // 50 -> 100
        ((max_count / 100) + 1) * 100
    };

    // calculate image size
    // 2 = frame width on both edges
    let width = 2 + ride_count;
    let height = 2 + max_count_with_headroom;
    let width_u32: u32 = width.try_into().expect("width too large");
    let height_u32: u32 = height.try_into().expect("height too large");

    let mut pixels = vec![ChartColor::Background; usize::try_from(width * height).unwrap()];

    // draw ticks
    const HORIZONTAL_TICK_STEP: usize = 100;
    const VERTICAL_TICK_STEP: usize = 100;
    for graph_y in (0..max_count_with_headroom).step_by(VERTICAL_TICK_STEP) {
        let y = height - (1 + graph_y);
        for x in 1..(width-1) {
            pixels[y * width + x] = ChartColor::Tick;
        }
    }
    for graph_x in (0..ride_count).step_by(HORIZONTAL_TICK_STEP) {
        let x = 1 + graph_x;
        for y in 1..(height-1) {
            pixels[y * width + x] = ChartColor::Tick;
        }
    }

    // draw frame
    for y in 0..height {
        pixels[y * width + 0] = ChartColor::Border;
        pixels[y * width + (width - 1)] = ChartColor::Border;
    }
    for x in 0..width {
        pixels[0 * width + x] = ChartColor::Border;
        pixels[(height - 1) * width + x] = ChartColor::Border;
    }

    // now draw the data
    for (graph_x, ride_id) in ride_ids_in_order.iter().enumerate() {
        let rider_to_latest_vehicle_count = ride_id_to_rider_to_latest_vehicle_count
            .get(ride_id).unwrap();

        let x = 1 + graph_x;
        for (i, rider) in rider_names.iter().enumerate() {
            let vehicle_count = rider_to_latest_vehicle_count
                .get(rider)
                .map(|vc| *vc)
                .unwrap_or(0);

            let y = height - (1 + vehicle_count);
            let pixel_value = ChartColor::Data((i % CHART_COLORS.len()).try_into().unwrap());
            pixels[y * width + x] = pixel_value;

            for graph_thicker_y in 0..thicken {
                let thicker_y_down = y + 1 + graph_thicker_y;
                if thicker_y_down < height {
                    pixels[thicker_y_down * width + x] = pixel_value;
                }

                if let Some(thicker_y_up) = y.checked_sub(1 + graph_thicker_y) {
                    pixels[thicker_y_up * width + x] = pixel_value;
                }
            }
        }
    }

    // PNGify
    let palette: Vec<u8> = CHART_BACKGROUND_COLOR.into_iter()
        .chain(CHART_BORDER_COLOR.into_iter())
        .chain(CHART_TICK_COLOR.into_iter())
        .chain(CHART_COLORS.into_iter().flat_map(|cs| cs))
        .collect();
    let mut png_bytes: Vec<u8> = Vec::new();

    {
        let mut png_encoder = png::Encoder::new(&mut png_bytes, width_u32, height_u32);
        png_encoder.set_color(png::ColorType::Indexed);
        png_encoder.set_depth(png::BitDepth::Eight);
        png_encoder.set_palette(palette);
        let mut png_writer = png_encoder.write_header().expect("failed to write PNG header");
        let mut png_data = Vec::with_capacity(pixels.len());
        png_data.extend(pixels.iter().map(|p| p.palette_index()));
        png_writer.write_image_data(&png_data).expect("failed to write image data");
    }

    let response_res = Response::builder()
        .header("Content-Type", "image/png")
        .body(Body::from(png_bytes));
    match response_res {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("failed to construct latest-rider-count-over-time-image response: {}", e);
            return return_500();
        }
    }
}


pub(crate) async fn handle_bim_latest_rider_count_over_time(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);
    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };
    let riders_res = db_conn.query(
        "SELECT DISTINCT rider_username FROM bim.rides",
        &[],
    ).await;
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
            color: CHART_COLORS[i % CHART_COLORS.len()],
        });
    }

    let rides_res = db_conn.query(
        "
            SELECT
                rav.company, rav.vehicle_number, rav.rider_username
            FROM bim.rides_and_vehicles rav
            ORDER BY
                rav.\"timestamp\", rav.id
        ",
        &[],
    ).await;
    let ride_rows = match rides_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query riders: {}", e);
            return return_500();
        },
    };

    let mut comp_veh_to_last_rider: HashMap<(String, String), String> = HashMap::new();
    let mut from_to_count: BTreeMap<(String, String), u64> = BTreeMap::new();
    for ride_row in ride_rows {
        let company: String = ride_row.get(0);
        let vehicle_number: String = ride_row.get(1);
        let rider_username: String = ride_row.get(2);

        if let Some(previous_rider) = comp_veh_to_last_rider.get(&(company.clone(), vehicle_number.clone())) {
            if previous_rider != &rider_username {
                let count_ref = from_to_count
                    .entry((previous_rider.clone(), rider_username.clone()))
                    .or_insert(0);
                *count_ref += 1;
            }
        }

        comp_veh_to_last_rider.insert((company, vehicle_number), rider_username);
    }

    let template = BimLatestRiderCountTemplate {
        riders,
        from_to_count,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_bim_histogram_by_day_of_week(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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


pub(crate) async fn handle_bim_histogram_by_vehicle_ride_count_group(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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

pub(crate) async fn handle_bim_histogram_by_line_ride_count_group(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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

pub(crate) async fn handle_bim_last_rider_pie(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
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
        ("", &mut company_to_type_to_rider_to_last_count),
        ("AND rav.coupling_mode = 'R'", &mut company_to_type_to_rider_to_last_count_ridden),
    ];
    for (condition, map) in conditions_maps {
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
            condition,
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
