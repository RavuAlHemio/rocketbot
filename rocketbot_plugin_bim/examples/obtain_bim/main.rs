mod droppable_child;
mod extract_info;
mod wiki_parsing;


use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json;

use crate::extract_info::{process_page, process_table, row_data_to_trams};
use crate::wiki_parsing::WikiParser;


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct Config {
    pub php_path: Option<String>,
    pub wiki_parse_server_dir: String,
    pub parser_already_running: bool,
    pub page_url_pattern: String,
    pub page_names: Vec<String>,
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

        for page_title in &config.page_names {
            let mut vehicles = process_page(
                &config.page_url_pattern,
                page_title,
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
        let f = File::create("bims.json")
            .expect("failed to open output file");
        serde_json::to_writer_pretty(f, &all_vehicles)
            .expect("failed to write vehicles");
    }
}
