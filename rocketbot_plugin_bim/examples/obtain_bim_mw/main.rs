//! Obtains vehicle databases from tabular MediaWiki data, e.g. from Stadtverkehr Austria Wiki.


mod extract_info;


use std::collections::{BTreeMap, HashMap};
use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use ciborium;
use indexmap::IndexMap;
use regex::Regex;
use rocketbot_bim_common::{VehicleClass, VehicleInfo};
use rocketbot_interface::serde::{serde_opt_regex, serde_regex};
use rocketbot_mediawiki_parsing::WikiParser;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::extract_info::{process_page, process_table, row_data_to_trams};


#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    pub output_path: String,
    pub php_path: Option<String>,
    pub wiki_parse_server_dir: String,
    pub parser_already_running: bool,
    pub page_sources: Vec<PageSource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PageSource {
    pub page_url_pattern: String,
    pub pages: Vec<PageConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PageConfig {
    pub title: String,
    pub type_code: String,
    pub vehicle_class: VehicleClass,
    #[serde(default)] pub fixed_couplings: bool,
    #[serde(default)] pub number_matcher: Option<MatcherTransformerConfig>,
    #[serde(default, with = "serde_opt_regex")] pub number_separator_regex: Option<Regex>,
    #[serde(default)] pub type_specific_number_name_matchers: Vec<TypeMatchConfig>,
    #[serde(default)] pub in_service_since_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub out_of_service_since_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub manufacturer_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub depot_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub common_props: IndexMap<String, String>,
    #[serde(default)] pub common_type_code: Option<String>,
    #[serde(default)] pub specific_type_code_property: Option<String>,
    #[serde(default)] pub type_code_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub section_stack_regexes: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MatcherTransformerConfig {
    #[serde(with = "serde_regex")] pub column_name_regex: Regex,
    #[serde(default)] pub value_replacements: Vec<ReplacementConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ReplacementConfig {
    #[serde(with = "serde_regex")] pub subject_regex: Regex,
    pub replacement: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TypeMatchConfig {
    pub matcher: MatcherTransformerConfig,
    pub type_code: String,
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_mw.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let php_command = config.php_path.as_deref().unwrap_or("php");

    let mut all_vehicles = BTreeMap::new();

    {
        let mut parser = if config.parser_already_running {
            WikiParser::new_existing()
        } else {
            let parser = WikiParser::new(php_command, &config.wiki_parse_server_dir)
                .expect("error creating parser");

            // wait a bit to allow the parser to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            parser
        };

        let mut page_cache = HashMap::new();
        for page_source in &config.page_sources {
            for page in &page_source.pages {
                let mut vehicles = process_page(
                    &page_source.page_url_pattern,
                    &page,
                    &mut parser,
                    &mut page_cache,
                    process_table,
                    row_data_to_trams,
                ).await;
                all_vehicles.append(&mut vehicles);
            }
        }

        parser.parsing_done()
            .expect("error signalling end of parsing");
    }

    let all_vehicles_vec: Vec<&VehicleInfo> = all_vehicles.values().collect();

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        ciborium::into_writer(&all_vehicles_vec, f)
            .expect("failed to write vehicles");
    }
}
