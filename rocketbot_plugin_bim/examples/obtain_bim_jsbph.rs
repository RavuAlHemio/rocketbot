//! Obtains vehicle databases from busphoto.eu/transphoto.org sites that have migrated to
//! client-side DOM construction.


use std::collections::{BTreeMap, BTreeSet};
use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Mutex;

use boa_engine::object::builtins::JsArray;
use indexmap::IndexSet;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use rocketbot_bim_common::{PowerSource, VehicleClass, VehicleInfo, VehicleNumber};
use boa_engine::{Context, JsString, Source};
use rocketbot_string::regex::EnjoyableRegex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};


static REGEX_CACHE: Mutex<BTreeMap<String, Regex>> = Mutex::new(BTreeMap::new());


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub output_path: PathBuf,
    pub user_agent: String,
    pub data_script_selector: String,
    pub data_script_regexes: Vec<EnjoyableRegex>,
    pub type_mapping: BTreeMap<String, VehicleTypeConfig>,
    pub column_indexes: ColumnIndexConfig,
    pub urls: Vec<String>,
    pub values_to_ignore: BTreeSet<String>,
    pub interesting_states: BTreeSet<i32>,
    #[serde(default)] pub pre_script: String,
    #[serde(default)] pub post_script: String,
    #[serde(default)] pub number_splitter: Option<String>,
    #[serde(default)] pub number_evaluators: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct VehicleTypeConfig {
    pub vehicle_type: String,
    pub vehicle_class: VehicleClass,
    pub manufacturer: Option<String>,
    #[serde(default)] pub power_sources: BTreeSet<PowerSource>,
    #[serde(default)] pub number_evaluator_key: Option<String>,
    #[serde(default)] pub air_conditioned: Option<bool>,
    #[serde(default)] pub common_other_data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct ColumnIndexConfig {
    pub state_column: i64,
    pub number_column: i64,
    pub type_column: i64,
    pub in_service_since_columns: Vec<i64>, // first column with a value wins
    pub out_of_service_since_column: Option<i64>,
    pub depot_column: Option<i64>,
    pub other_info_names_to_columns: BTreeMap<String, i64>,
}


fn string_matches_regex(string: &str, regex_str: &str) -> Result<bool, Box<rhai::EvalAltResult>> {
    // do we know this regex already?
    let mut regex_cache_guard = REGEX_CACHE.lock()
        .expect("REGEX_CACHE poisoned?!");

    if let Some(known_regex) = regex_cache_guard.get(regex_str) {
        Ok(known_regex.is_match(string))
    } else {
        let regex_obj = match Regex::new(regex_str) {
            Ok(ro) => ro,
            Err(e) => return Err(format!("failed to parse regex: {:?}", e).into()),
        };
        regex_cache_guard.insert(regex_str.to_owned(), regex_obj.clone());
        Ok(regex_obj.is_match(string))
    }
}


async fn obtain_vehicles(
    client: &reqwest::Client,
    url: &str,
    config: &Config,
    number_to_vehicle: &mut BTreeMap<VehicleNumber, VehicleInfo>,
) -> Option<String> {
    // compile a few selectors
    let data_script_selector = match Selector::parse(&config.data_script_selector) {
        Ok(ts) => ts,
        Err(e) => panic!("invalid data script selector {:?}: {}", config.data_script_selector, e),
    };

    // compile the number evaluator scripts
    let compiler = rhai::Engine::new();
    let mut name_to_evaluator: BTreeMap<String, rhai::AST> = BTreeMap::new();
    for (key, evaluator_string) in &config.number_evaluators {
        let compiled = match compiler.compile(evaluator_string) {
            Ok(ast) => ast,
            Err(e) => panic!("failed to compile number evaluator {:?}: {}", key, e),
        };
        name_to_evaluator.insert(key.clone(), compiled);
    }

    // download the page
    let response_string = if let Some(path) = url.strip_prefix("file://") {
        std::fs::read_to_string(path)
            .expect("failed to read file URL")
    } else {
        let response_res = client.get(url)
            .send().await.and_then(|r| r.error_for_status());
        let response = match response_res {
            Ok(r) => r,
            Err(e) => panic!("failed to download {:?}: {}", url, e),
        };
        let response_bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => panic!("failed to obtain bytes for {:?}: {}", url, e),
        };
        match String::from_utf8(response_bytes.to_vec()) {
            Ok(rs) => rs,
            Err(e) => panic!("failed to decode bytes for {:?} as UTF-8: {}", url, e),
        }
    };
    let html = Html::parse_document(&response_string);

    // find the data script blocks
    for data_script_block in html.select(&data_script_selector) {
        let mut script: String = data_script_block
            .text()
            .collect();
        script.insert_str(0, &config.pre_script);
        script.push_str(&config.post_script);

        // check that all regexes match
        let mut good_block = true;
        for regex in &config.data_script_regexes {
            if !regex.is_match(&script) {
                good_block = false;
                break;
            }
        }
        if !good_block {
            // we are not interested in this code block
            continue;
        }

        // evaluate the JS block
        let mut context = Context::default();
        let source = Source::from_bytes(&script);
        context.eval(source)
            .expect("failed to evaluate data script block");
        let global = context.global_object();
        let obtain_bim_data_obj = global.get(JsString::from("obtainBimData"), &mut context)
            .expect("script did not store global value obtainBimData")
            .as_object().expect("obtainBimData is not an object");
        let obtain_bim_data_array = JsArray::from_object(obtain_bim_data_obj)
            .expect("obtainBimData is not an array");
        let data_length: i64 = obtain_bim_data_array.length(&mut context)
            .expect("failed to obtain obtainBimData length")
            .try_into().unwrap();

        for vehicle_index in 0..data_length {
            let vehicle_obj = obtain_bim_data_array.at(vehicle_index, &mut context)
                .expect("failed to obtain obtainBimData element")
                .as_object().expect("obtainBimData element is not an object");
            let vehicle_array = JsArray::from_object(vehicle_obj)
                .expect("obtainBimData element is not an array");
            let vehicle_length: i64 = vehicle_array.length(&mut context)
                .expect("failed to obtain obtainBimData element length")
                .try_into().unwrap();

            // this might be a vehicle, yay
            let mut vehicle_is_interesting = true;
            let mut vehicle_number = None;
            let mut raw_type = None;
            let mut in_service_since_values = BTreeMap::new();
            let mut out_of_service_since = None;
            let mut depot = None;
            let mut other_data = BTreeMap::new();
            for field_index in 0..vehicle_length {
                let field_value = vehicle_array.at(field_index, &mut context)
                    .expect("failed to obtain obtainBimData grandchild element");
                if field_value.is_null_or_undefined() {
                    continue;
                }

                let cell_text = field_value.to_string(&mut context)
                    .expect("failed to convert field value to a string")
                    .to_std_string().expect("field value not a valid UTF-16 string");

                if cell_text.len() == 0 {
                    continue;
                }
                if config.values_to_ignore.contains(&cell_text) {
                    continue;
                }

                if config.column_indexes.state_column == field_index {
                    // check the state value
                    if let Some(state_value) = field_value.as_i32() {
                        if !config.interesting_states.contains(&state_value) {
                            vehicle_is_interesting = false;
                            break;
                        }
                    }
                }
                if config.column_indexes.number_column == field_index {
                    vehicle_number = Some(VehicleNumber::from_string(cell_text.clone()));
                }
                if config.column_indexes.type_column == field_index {
                    raw_type = Some(cell_text.clone());
                }
                if config.column_indexes.in_service_since_columns.contains(&field_index) {
                    in_service_since_values.insert(field_index, cell_text.clone());
                }
                if config.column_indexes.out_of_service_since_column == Some(field_index) {
                    out_of_service_since = Some(cell_text.clone());
                }
                if config.column_indexes.depot_column == Some(field_index) {
                    depot = Some(cell_text.clone());
                }
                let interesting_keys = config.column_indexes.other_info_names_to_columns
                    .iter()
                    .filter(|(_key, oi_col_idx)| **oi_col_idx == field_index)
                    .map(|(key, _oi_col_idx)| key);
                for key in interesting_keys {
                    other_data.insert(key.clone(), cell_text.clone());
                }
            }

            if !vehicle_is_interesting {
                // vehicle probably in the wrong state
                continue;
            }

            if vehicle_number.is_none() || raw_type.is_none() {
                eprintln!("skipping incomplete vehicle {:?}/{:?}", vehicle_number, raw_type);
                continue;
            }
            let Some(type_info) = config.type_mapping.get(raw_type.as_ref().unwrap()) else {
                eprintln!("skipping vehicle {:?} of unmapped type {:?}", vehicle_number, raw_type);
                continue;
            };

            // determine the best in-service-since value
            let mut in_service_since = None;
            for in_service_since_column in &config.column_indexes.in_service_since_columns {
                if let Some(value) = in_service_since_values.get(in_service_since_column) {
                    in_service_since = Some(value.clone());
                    break;
                }
            }

            for (k, v) in &type_info.common_other_data {
                // do not overwrite existing entries
                other_data.entry(k.clone()).or_insert_with(|| v.clone());
            }

            let mut overridden_fixed_coupling = IndexSet::new();
            let vehicle_numbers: IndexSet<VehicleNumber> = if let Some(evaluator_name) = type_info.number_evaluator_key.as_ref() {
                // okay, roll out the big guns
                let evaluator = match name_to_evaluator.get(evaluator_name) {
                    Some(e) => e,
                    None => panic!("failed to find evaluator {:?} of type {:?}", evaluator_name, raw_type.as_ref().unwrap()),
                };
                let other_data_rhai = rhai::serde::to_dynamic(&other_data)
                    .expect("failed to create dynamic value from other_data");

                let mut engine = rhai::Engine::new();
                engine.register_fn("string_matches_regex", string_matches_regex);
                let mut scope = rhai::Scope::new();
                scope.set_value("vehicle_number", vehicle_number.unwrap().as_str().to_owned());
                scope.set_value("overridden_fixed_coupling", rhai::Array::new());
                scope.set_value("other_data", other_data_rhai);
                let vehicles_raw: Vec<rhai::Dynamic> = engine.eval_ast_with_scope(&mut scope, evaluator)
                    .expect("failed to evaluate evaluator");

                // handle fixed-coupling override
                let overridden_fixed: rhai::Array = scope.get_value("overridden_fixed_coupling")
                    .expect("overridden_fixed_coupling gone missing?!");
                for overridden_fixed_number in overridden_fixed {
                    let number = VehicleNumber::from_string(overridden_fixed_number.into_string().unwrap());
                    overridden_fixed_coupling.insert(number);
                }

                // handle modifications of other_data
                let other_data_modified = scope.get_value("other_data")
                    .expect("other_data gone missing?!");
                other_data = rhai::serde::from_dynamic(&other_data_modified)
                    .expect("failed to obtain other_data from dynamic value");

                vehicles_raw
                    .into_iter()
                    .map(|v| VehicleNumber::from_string(v.into_string().unwrap()))
                    .collect()
            } else if let Some(splitter) = config.number_splitter.as_ref() {
                vehicle_number
                    .as_ref().unwrap()
                    .split(splitter)
                    .map(|vn| VehicleNumber::from_string(vn.to_owned()))
                    .collect()
            } else {
                let mut vns = IndexSet::new();
                vns.insert(vehicle_number.as_ref().unwrap().clone());
                vns
            };

            for individual_vehicle_number in &vehicle_numbers {
                if number_to_vehicle.contains_key(individual_vehicle_number) {
                    eprintln!("skipping duplicate vehicle {:?} of type {:?}", individual_vehicle_number, raw_type);
                    continue;
                }

                let fixed_coupling = if overridden_fixed_coupling.len() > 0 {
                    overridden_fixed_coupling.clone()
                } else if vehicle_numbers.len() > 1 {
                    vehicle_numbers.clone()
                } else {
                    IndexSet::new()
                };

                let vehicle = VehicleInfo {
                    number: individual_vehicle_number.clone(),
                    vehicle_class: type_info.vehicle_class,
                    power_sources: type_info.power_sources.clone(),
                    type_code: type_info.vehicle_type.clone(),
                    in_service_since: in_service_since.clone(),
                    out_of_service_since: out_of_service_since.clone(),
                    manufacturer: type_info.manufacturer.clone(),
                    depot: depot.clone(),
                    air_conditioned: type_info.air_conditioned,
                    other_data: other_data.clone(),
                    fixed_coupling,
                };
                number_to_vehicle.insert(individual_vehicle_number.clone(), vehicle);
            }
        }
    }

    // no link to a next page
    None
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_jsbph.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let mut default_headers = HeaderMap::new();
    default_headers.insert("Cookie", HeaderValue::from_static("lang=en; divide=0; shorthh=0"));
    let http_client = reqwest::Client::builder()
        .default_headers(default_headers)
        .user_agent(&config.user_agent)
        .build().expect("failed to build HTTP client");

    let mut number_to_vehicle = BTreeMap::new();
    for start_url in &config.urls {
        let mut url = start_url.clone();
        while let Some(next_url) = obtain_vehicles(&http_client, &url, &config, &mut number_to_vehicle).await {
            url = next_url;
        }
    }

    // derive list of references
    let vehicles: Vec<&VehicleInfo> = number_to_vehicle.values().collect();

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        ciborium::into_writer(&vehicles, f)
            .expect("failed to write vehicles");
    }
}
