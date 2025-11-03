//! Obtain vehicle databases from the SpotLog LocoList.


use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env::args_os;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::time::Duration;

use ciborium;
use csv;
use indexmap::{IndexMap, IndexSet};
use reqwest;
use rocketbot_bim_common::{PowerSource, VehicleClass, VehicleInfo, VehicleNumber};
use serde::{Deserialize, Serialize};
use serde_json;


#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct Config {
    pub pages: Vec<PageInfo>,
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct PageInfo {
    pub csv_url: String,
    pub subsets: BTreeSet<String>,
    pub timeout_ms: Option<u64>,
    pub class_to_export_class: HashMap<String, ExportClass>,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct ExportClass {
    pub type_code: String,
    pub vehicle_class: VehicleClass,
    #[serde(default)] pub manufacturer: Option<String>,
    #[serde(default)] pub power_sources: BTreeSet<PowerSource>,
    #[serde(default)] pub other_data: BTreeMap<String, String>,
    #[serde(default)] pub include_deleted: bool,
}

trait EmptyNoneElseCloned {
    type Output;
    fn empty_none_else_cloned(&self) -> Self::Output;
}
impl EmptyNoneElseCloned for Option<&String> {
    type Output = Option<String>;
    fn empty_none_else_cloned(&self) -> Self::Output {
        if let Some(s) = self {
            if s.len() == 0 {
                None
            } else {
                Some((*s).to_owned())
            }
        } else {
            None
        }
    }
}


async fn obtain_page_bytes(url: &str, timeout: Option<Duration>) -> Vec<u8> {
    if let Some(file_path) = url.strip_prefix("file://") {
        // it's a local file
        let mut f = File::open(file_path)
            .expect("failed to open local page");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .expect("failed to read local page");
        buf
    } else {
        let mut client_builder = reqwest::Client::builder();
        if let Some(to) = timeout {
            client_builder = client_builder.timeout(to);
        }
        let client = client_builder.build()
            .expect("failed to build client");
        let request = client.get(url)
            .build().expect("failed to build request");
        let response = client.execute(request).await
            .expect("failed to obtain response");
        let response_bytes = response.bytes().await
            .expect("failed to obtain response bytes");
        response_bytes.to_vec()
    }
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_sl.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let mut vehicles: Vec<VehicleInfo> = Vec::new();
    for page in &config.pages {
        eprintln!("fetching {}", page.csv_url);

        let timeout = page.timeout_ms.map(|ms| Duration::from_millis(ms));

        let page_bytes_utf16le = obtain_page_bytes(&page.csv_url, timeout).await;
        let page_words: Vec<u16> = page_bytes_utf16le
            .chunks(2)
            .map(|ch| u16::from_le_bytes(ch.try_into().unwrap()))
            .collect();
        let mut page_text = String::from_utf16(&page_words)
            .expect("failed to interpret page text as UTF-16LE");
        if page_text.starts_with("\u{FEFF}") {
            page_text.remove(0);
        }
        let page_bytes_cursor = Cursor::new(&page_text);
        let mut csv_reader = csv::ReaderBuilder::new()
            .delimiter(b',')
            .has_headers(true)
            .quote(b'"')
            .quoting(true)
            .double_quote(true)
            .escape(None)
            .from_reader(page_bytes_cursor);

        let headers: Vec<String> = csv_reader.headers()
            .expect("failed to read headers")
            .iter()
            .map(|h| h.to_owned())
            .collect();
        for record_res in csv_reader.records() {
            let record = record_res.expect("invalid CSV record");
            let map: IndexMap<String, String> = headers
                .iter()
                .zip(record.iter())
                .map(|(k, v)| (k.clone(), v.to_owned()))
                .collect();

            if !map.get("Subset").map(|s| page.subsets.contains(s)).unwrap_or(true) {
                // wrong subset
                continue;
            }

            let Some(cls) = map.get("Class") else { continue };
            let Some(class_def) = page.class_to_export_class.get(cls) else { continue };

            let formation_opt = map.get("Form");
            let Some(number) = map.get("Number") else { continue };
            let coupled_vehicles = if let Some(formation_str) = formation_opt {
                // vehicles in formation may be split by "," or ", "
                let pieces: Vec<&str> = formation_str
                    .split(",")
                    .map(|piece| if let Some(rest) = piece.strip_prefix(" ") {
                        rest
                    } else {
                        piece
                    })
                    .collect();
                if pieces.len() == 0 || (pieces.len() == 1 && pieces[0].len() == 0) {
                    vec![number.as_str()]
                } else {
                    pieces
                }
            } else {
                vec![number.as_str()]
            };

            let depot = map
                .get("Depot")
                .empty_none_else_cloned();
            let status = map
                .get("Status")
                .empty_none_else_cloned()
                .unwrap_or_else(|| "A".to_owned());

            let (want_from, want_to) = match status.as_str() {
                "A" => {
                    // active
                    (true, false)
                },
                "W"|"X" => {
                    // withdrawn/scrapped
                    (true, true)
                },
                _ => {
                    // other (assume active)
                    (true, false)
                },
            };

            let in_service_since = if want_from {
                Some(
                    map
                        .get("InService")
                        .empty_none_else_cloned()
                        .unwrap_or_else(|| "?".to_owned())
                )
            } else {
                None
            };
            let out_of_service_since = if want_to {
                Some(
                    map
                        .get("InService")
                        .empty_none_else_cloned()
                        .or_else(|| map
                            .get("Scrapped")
                            .empty_none_else_cloned()
                        )
                        .unwrap_or_else(|| "?".to_owned())
                )
            } else {
                None
            };
            let pool = map.get("Pool").empty_none_else_cloned();

            let mut other_data = class_def.other_data.clone();
            if let Some(p) = pool {
                other_data.insert("Pool".to_owned(), p);
            }

            // create the vehicles
            let coupled_vehicle_numbers: Vec<_> = coupled_vehicles
                .iter()
                .map(|cv| VehicleNumber::from((*cv).to_owned()))
                .collect();
            let fixed_coupling = if coupled_vehicle_numbers.len() > 1 {
                coupled_vehicle_numbers
                    .iter()
                    .cloned()
                    .collect()
            } else {
                IndexSet::with_capacity(0)
            };
            for coupled_vehicle_number in &coupled_vehicle_numbers {
                let vehicle = VehicleInfo {
                    number: coupled_vehicle_number.clone(),
                    vehicle_class: class_def.vehicle_class,
                    power_sources: class_def.power_sources.clone(),
                    type_code: class_def.type_code.clone(),
                    in_service_since: in_service_since.clone(),
                    out_of_service_since: out_of_service_since.clone(),
                    manufacturer: class_def.manufacturer.clone(),
                    depot: depot.clone(),
                    other_data: other_data.clone(),
                    fixed_coupling: fixed_coupling.clone(),
                };
                vehicles.push(vehicle);
            }
        }
    }

    vehicles.sort_unstable_by_key(|v| v.number.clone());

    // clear out duplicates
    let mut i = 1;
    while i < vehicles.len() {
        let left = &vehicles[i-1];
        let right = &vehicles[i];
        if left == right {
            vehicles.remove(i);
            continue;
        }

        if left.number == right.number {
            println!("dupe! {:?} vs. {:?}", left, right);
            // remove the older one (assume it's the one that came first)
            vehicles.remove(i - 1);
        } else {
            i += 1;
        }
    }

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        ciborium::into_writer(&vehicles, f)
            .expect("failed to write vehicles");
    }
}
