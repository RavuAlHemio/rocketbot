//! Obtains vehicle databases from busphoto.eu/transphoto.org.


use std::collections::{BTreeMap, BTreeSet};
use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use indexmap::IndexSet;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use rocketbot_bim_common::{PowerSource, VehicleClass, VehicleInfo, VehicleNumber};
use rocketbot_string::regex::EnjoyableRegex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;


static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("\\s+").expect("failed to compile whitespace regex"));
static REGEX_CACHE: Mutex<BTreeMap<String, Regex>> = Mutex::new(BTreeMap::new());


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub output_path: PathBuf,
    pub user_agent: String,
    pub table_css_selector: String,
    pub next_page_link_selector: String,
    pub header_row_css_classes: BTreeSet<String>,
    pub interesting_row_css_classes: BTreeSet<String>,
    pub type_mapping: BTreeMap<String, VehicleTypeConfig>,
    pub column_keys: ColumnKeyConfig,
    pub urls: Vec<String>,
    pub values_to_ignore: BTreeSet<String>,
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
    #[serde(default)] pub common_other_data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct ColumnKeyConfig {
    pub number_column: EnjoyableRegex,
    pub type_column: EnjoyableRegex,
    pub in_service_since_column: EnjoyableRegex,
    pub out_of_service_since_column: Option<EnjoyableRegex>,
    pub depot_column: Option<EnjoyableRegex>,
    pub other_info_names_to_columns: BTreeMap<String, EnjoyableRegex>,
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
    let table_selector = match Selector::parse(&config.table_css_selector) {
        Ok(ts) => ts,
        Err(e) => panic!("invalid table selector {:?}: {}", config.table_css_selector, e),
    };
    let next_page_link_selector = match Selector::parse(&config.next_page_link_selector) {
        Ok(npls) => npls,
        Err(e) => panic!("invalid next page link selector {:?}: {}", config.next_page_link_selector, e),
    };
    let row_selector = Selector::parse("tr")
        .expect("failed to parse row selector");

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
    let response_string = match String::from_utf8(response_bytes.to_vec()) {
        Ok(rs) => rs,
        Err(e) => panic!("failed to decode bytes for {:?} as UTF-8: {}", url, e),
    };

    let html = Html::parse_document(&response_string);

    // pick out the interesting tables
    for interesting_table in html.select(&table_selector) {
        // go through the rows
        let mut headers = Vec::new();
        for row in interesting_table.select(&row_selector) {
            let row_classes_str = row.attr("class")
                .unwrap_or("");
            let row_classes: Vec<&str> = WHITESPACE_RE.split(row_classes_str)
                .filter(|c| c.len() > 0)
                .collect();

            let any_header_class = row_classes.iter()
                .any(|c| config.header_row_css_classes.contains(*c));
            if any_header_class {
                // this is a header row; fish out fresh headers
                headers.clear();

                let row_headers = row.child_elements()
                    .filter(|e| e.value().name() == "th");
                for header in row_headers {
                    let header_text: String = header.text().collect();
                    headers.push(header_text);
                }

                // don't bother parsing as vehicle row
                continue;
            }

            let any_interesting_class = row_classes.iter()
                .any(|c| config.interesting_row_css_classes.contains(*c));
            if !any_interesting_class {
                // this row is not interesting for us
                continue;
            }

            let row_cells = row.child_elements()
                .filter(|e| e.value().name() == "td")
                .enumerate();
            let mut vehicle_number = None;
            let mut raw_type = None;
            let mut in_service_since = None;
            let mut out_of_service_since = None;
            let mut depot = None;
            let mut other_data = BTreeMap::new();
            for (i, cell) in row_cells {
                if i >= headers.len() {
                    // more cells than headers?!
                    continue;
                }

                let cell_text: String = cell.text().collect();
                if cell_text.len() == 0 {
                    continue;
                }
                if config.values_to_ignore.contains(&cell_text) {
                    continue;
                }

                let this_header = &headers[i];
                if config.column_keys.number_column.is_match(&this_header) {
                    vehicle_number = Some(VehicleNumber::from_string(cell_text.clone()));
                }
                if config.column_keys.type_column.is_match(&this_header) {
                    raw_type = Some(cell_text.clone());
                }
                if config.column_keys.in_service_since_column.is_match(&this_header) {
                    in_service_since = Some(cell_text.clone());
                }
                if config.column_keys.out_of_service_since_column.as_ref().map(|re| re.is_match(&this_header)).unwrap_or(false) {
                    out_of_service_since = Some(cell_text.clone());
                }
                if config.column_keys.depot_column.as_ref().map(|re| re.is_match(&this_header)).unwrap_or(false) {
                    depot = Some(cell_text.clone());
                }
                let interesting_keys = config.column_keys.other_info_names_to_columns
                    .iter()
                    .filter(|(_key, oi_col_re)| oi_col_re.is_match(&this_header))
                    .map(|(key, _oi_col_re)| key);
                for key in interesting_keys {
                    other_data.insert(key.clone(), cell_text.clone());
                }
            }

            if vehicle_number.is_none() || raw_type.is_none() {
                eprintln!("skipping incomplete vehicle {:?}/{:?}", vehicle_number, raw_type);
                continue;
            }
            let Some(type_info) = config.type_mapping.get(raw_type.as_ref().unwrap()) else {
                eprintln!("skipping vehicle {:?} of unmapped type {:?}", vehicle_number, raw_type);
                continue;
            };

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
                    other_data: other_data.clone(),
                    fixed_coupling,
                };
                number_to_vehicle.insert(individual_vehicle_number.clone(), vehicle);
            }
        }
    }

    // do we have a link to the next page?
    let next_page_link = html.select(&next_page_link_selector)
        .nth(0)
        .and_then(|e| e.attr("href"))
        .map(|u|
            Url::parse(url).expect("invalid base URL")
                .join(u).expect("invalid joined URL")
                .as_str()
                .to_owned()
        );
    next_page_link
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_bph.json"),
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
