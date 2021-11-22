mod droppable_child;
mod extract_info;
mod serde_impls;
mod wiki_parsing;


use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::extract_info::{process_page, process_table, row_data_to_trams};
use crate::serde_impls::{serde_opt_regex, serde_regex};
use crate::wiki_parsing::WikiParser;


#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    pub output_path: String,
    pub php_path: Option<String>,
    pub wiki_parse_server_dir: String,
    pub parser_already_running: bool,
    pub page_url_pattern: String,
    pub pages: Vec<PageConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PageConfig {
    pub title: String,
    pub type_code: String,
    #[serde(default)] pub fixed_couplings: bool,
    #[serde(default)] pub number_matcher: Option<MatcherTransformerConfig>,
    #[serde(default, with = "serde_opt_regex")] pub number_separator_regex: Option<Regex>,
    #[serde(default)] pub type_specific_number_name_matchers: Vec<TypeMatchConfig>,
    #[serde(default)] pub in_service_since_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub out_of_service_since_matcher: Option<MatcherTransformerConfig>,
    #[serde(default)] pub manufacturer_matcher: Option<MatcherTransformerConfig>,
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
            None => PathBuf::from("obtain_bim.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let php_command = config.php_path.as_deref().unwrap_or("php");

    let mut all_vehicles = Vec::new();

    {
        let mut parser = if config.parser_already_running {
            WikiParser::new_existing()
        } else {
            WikiParser::new(php_command, &config.wiki_parse_server_dir)
                .expect("error creating parser")
        };

        for page in &config.pages {
            let mut vehicles = process_page(
                &config.page_url_pattern,
                &page,
                &mut parser,
                process_table,
                row_data_to_trams,
            ).await;
            all_vehicles.append(&mut vehicles);
        }

        parser.parsing_done()
            .expect("error signalling end of parsing");
    }

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        serde_json::to_writer_pretty(f, &all_vehicles)
            .expect("failed to write vehicles");
    }
}
