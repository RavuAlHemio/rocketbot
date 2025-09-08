//! Obtains vehicles from Ilostan Pojazdów Trakcyjnych (https://ilostan.forumkolejowe.pl/).


use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;
use std::env::args_os;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ego_tree::NodeRef;
use indexmap::IndexSet;
use regex::Regex;
use rocketbot_bim_common::{PowerSource, VehicleClass, VehicleInfo, VehicleNumber};
use rocketbot_string::regex::EnjoyableRegex;
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub output_path: String,
    pub vehicle_pages: Vec<VehiclePageConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct VehiclePageConfig {
    pub url: String,
    pub type_code: String,
    pub short_vehicle_number_extractor: EnjoyableRegex,
    pub vehicle_class: VehicleClass,
    #[serde(default)] pub power_sources: BTreeSet<PowerSource>,
    #[serde(default)] pub manufacturer: Option<String>,
    #[serde(default)] pub other_data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct VehicleEntry {
    pub vehicle_number: VehicleNumber,
    pub full_vehicle_number: String,
    pub operator: String,
    pub variant: String,
}


static VEHICLE_NUMBER_LINE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    "^",
    "(?P<vehnum>",
        "[^(]+",
    ")",
    "[ ]",
    "[(]",
    "(?P<operator>",
        "[^ ]+",
    ")",
    "(?:",
        "[ ]",
        "(?P<variant>",
            ".+",
        ")",
    ")?",
    "[)]",
    "$",
)).expect("failed to compile vehicle number line regex"));
static IN_SERVICE_SINCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    "[(]",
    "(?P<in_service_since>",
        ".+",
    ")",
    "[)]",
)).expect("failed to compile serial/year line regex"));


const fn windows_1250_map_byte_to_char(windows_1250_byte: u8) -> char {
    // https://www.unicode.org/Public/MAPPINGS/VENDORS/MICSFT/WINDOWS/CP1250.TXT
    match windows_1250_byte {
        0x00..=0x7F => {
            // low ASCII is equal to ISO 8859-1 and thereby to Unicode
            char::from_u32(windows_1250_byte as u32).unwrap()
        },
        0x80 => '\u{20AC}', 0x82 => '\u{201A}', 0x84 => '\u{201E}', 0x85 => '\u{2026}',
        0x86 => '\u{2020}', 0x87 => '\u{2021}', 0x89 => '\u{2030}', 0x8A => '\u{0160}',
        0x8B => '\u{2039}', 0x8C => '\u{015A}', 0x8D => '\u{0164}', 0x8E => '\u{017D}',
        0x8F => '\u{0179}', 0x91 => '\u{2018}', 0x92 => '\u{2019}', 0x93 => '\u{201C}',
        0x94 => '\u{201D}', 0x95 => '\u{2022}', 0x96 => '\u{2013}', 0x97 => '\u{2014}',
        0x99 => '\u{2122}', 0x9A => '\u{0161}', 0x9B => '\u{203A}', 0x9C => '\u{015B}',
        0x9D => '\u{0165}', 0x9E => '\u{017E}', 0x9F => '\u{017A}', 0xA1 => '\u{02C7}',
        0xA2 => '\u{02D8}', 0xA3 => '\u{0141}', 0xA5 => '\u{0104}', 0xAA => '\u{015E}',
        0xAF => '\u{017B}', 0xB2 => '\u{02DB}', 0xB3 => '\u{0142}', 0xB9 => '\u{0105}',
        0xBA => '\u{015F}', 0xBC => '\u{013D}', 0xBD => '\u{02DD}', 0xBE => '\u{013E}',
        0xBF => '\u{017C}', 0xC0 => '\u{0154}', 0xC3 => '\u{0102}', 0xC5 => '\u{0139}',
        0xC6 => '\u{0106}', 0xC8 => '\u{010C}', 0xCA => '\u{0118}', 0xCC => '\u{011A}',
        0xCF => '\u{010E}', 0xD0 => '\u{0110}', 0xD1 => '\u{0143}', 0xD2 => '\u{0147}',
        0xD5 => '\u{0150}', 0xD8 => '\u{0158}', 0xD9 => '\u{016E}', 0xDB => '\u{0170}',
        0xDE => '\u{0162}', 0xE0 => '\u{0155}', 0xE3 => '\u{0103}', 0xE5 => '\u{013A}',
        0xE6 => '\u{0107}', 0xE8 => '\u{010D}', 0xEA => '\u{0119}', 0xEC => '\u{011B}',
        0xEF => '\u{010F}', 0xF0 => '\u{0111}', 0xF1 => '\u{0144}', 0xF2 => '\u{0148}',
        0xF5 => '\u{0151}', 0xF8 => '\u{0159}', 0xF9 => '\u{016F}', 0xFB => '\u{0171}',
        0xFE => '\u{0163}', 0xFF => '\u{02D9}',
        0xA0|0xA4|0xA6..=0xA9|0xAB..=0xAE|0xB0|0xB1|0xB4..=0xB8|0xBB|0xC1|0xC2|0xC4|0xC7|0xC9|0xCB
                |0xCD|0xCE|0xD3|0xD4|0xD6|0xD7|0xDA|0xDC|0xDD|0xDF|0xE1|0xE2|0xE4|0xE7|0xE9|0xEB
                |0xED|0xEE|0xF3|0xF4|0xF6|0xF7|0xFA|0xFC|0xFD => {
            // these are equal to ISO 8859-1 and thereby to Unicode
            char::from_u32(windows_1250_byte as u32).unwrap()
        },
        0x81|0x83|0x88|0x90|0x98 => {
            // undefined; consider it equal to ISO 8859-1
            char::from_u32(windows_1250_byte as u32).unwrap()
        },
    }
}


async fn obtain_page_bytes(url: &str) -> Vec<u8> {
    if let Some(file_path) = url.strip_prefix("file://") {
        // it's a local file
        let mut f = File::open(file_path)
            .expect("failed to open local page");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .expect("failed to read local page");
        buf
    } else {
        let response = reqwest::get(url).await
            .expect("failed to obtain response");
        let response_bytes = response.bytes().await
            .expect("failed to obtain response bytes");
        response_bytes.to_vec()
    }
}


fn output_usage() {
    eprintln!("Usage: obtain_bim_ipt [CONFIG.JSON]");
}


fn collect_text(node: NodeRef<'_, Node>, string_to_fill: &mut String) {
    match node.value() {
        Node::Document|Node::Fragment|Node::Element(_) => {
            // run through the children
            for child in node.children() {
                collect_text(child, string_to_fill);
            }
        },
        Node::Comment(_)|Node::ProcessingInstruction(_)|Node::Doctype(_) => {
            // nothing
        },
        Node::Text(text) => {
            string_to_fill.push_str(text);
        },
    }
}

fn br_lines(element: ElementRef<'_>) -> Vec<String> {
    let mut current_string = String::new();
    let mut lines = Vec::new();

    for child in element.children() {
        if let Node::Element(elem) = child.value() {
            if elem.name() == "br" {
                // end-of-line
                lines.push(current_string);
                current_string = String::new();
                continue;
            }
        }

        collect_text(child, &mut current_string);
    }

    lines.push(current_string);
    lines
}


#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<OsString> = args_os().collect();
    if args.len() > 2 {
        output_usage();
        return ExitCode::FAILURE;
    }
    let config_path = if args.len() > 1 {
        if args[1] == "--help" || args[1] == "-h" {
            output_usage();
            return ExitCode::SUCCESS;
        }
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("obtain_bim_ipt.json")
    };

    let config_str = std::fs::read_to_string(&config_path)
        .expect("failed to read config file");
    let config: Config = serde_json::from_str(&config_str)
        .expect("failed to parse config file");

    // prepare CSS selectors
    let table_row_selector = Selector::parse("tbody#myTable > tr").unwrap();

    let mut number_to_vehicle = BTreeMap::new();
    for vehicle_page_config in &config.vehicle_pages {
        let page_bytes = obtain_page_bytes(&vehicle_page_config.url).await;

        // page advertises its charset as UTF-8, but it's actually Windows-1250
        let page_string: String = page_bytes
            .iter()
            .map(|b| windows_1250_map_byte_to_char(*b))
            .collect();

        // parse it
        let page_doc = Html::parse_document(&page_string);
        let table_rows = page_doc.select(&table_row_selector);
        for table_row in table_rows {
            let css_classes: HashSet<&str> = table_row.value().classes().collect();
            if css_classes.contains("wiersz_stat_mod") {
                // modernized to a different type; skip
                continue;
            }
            if css_classes.contains("wiersz_stat_pochodzenie") {
                // "origin"?
                eprintln!("warning: skipping row with CSS class \"wiersz_stat_pochodzenie\" because its purpose is not yet understood");
                continue;
            }
            let is_museal = css_classes.contains("wiersz_stat_eksponat");
            let is_scrapped = css_classes.contains("wiersz_stat_kasacja");

            let table_cells = table_row.child_elements()
                .filter(|c| c.value().name() == "td");

            let mut in_service_since_opt = None;
            let mut out_of_service_since_opt = None;
            let mut serial_number_opt = None;
            let mut vehicle_entries = Vec::new();
            for (i, table_cell) in table_cells.enumerate() {
                if i == 0 {
                    // the most interesting cell, with the vehicle numbers
                    let lines = br_lines(table_cell);
                    // lines[0]: numbering within series: "EN69-420"
                    // lines[1]: serial number and/or year when taken into service: "L-1337-666 (1969)"
                    // lines[2..]: UIC number, operator and model variant: "94 51 6 969 696-6 (PL-ROFL abc)"

                    if lines.len() < 3 {
                        // no full vehicle numbers :-(
                        continue;
                    }

                    // find the last match of the in-service-since regex
                    (serial_number_opt, in_service_since_opt) = if let Some(in_service_since_caps) = IN_SERVICE_SINCE_RE.captures_iter(&lines[1]).last() {
                        // the serial number is everything until the start of the match (trimming spaces from both ends)
                        let iss_start = in_service_since_caps.get(0).unwrap().start();
                        let serial_number = &lines[1][..iss_start].trim_matches(' ');
                        let serial_number_opt = if serial_number.len() == 0 {
                            None
                        } else {
                            Some((*serial_number).to_owned())
                        };

                        // the in-service-since is within the capture
                        let in_service_since = in_service_since_caps.name("in_service_since").unwrap().as_str();
                        (serial_number_opt, Some(in_service_since.to_owned()))
                    } else {
                        // only the serial number
                        (Some(lines[1].clone()), None)
                    };

                    for vehicle_number_line in &lines[2..] {
                        // split vehicle number and operator code
                        let Some(line_caps) = VEHICLE_NUMBER_LINE_RE.captures(&vehicle_number_line) else {
                            eprintln!("vehicle number line {:?} does not match regex; skipping", vehicle_number_line);
                            continue;
                        };

                        let vehicle_number_match = line_caps.name("vehnum").unwrap();
                        let operator_match = line_caps.name("operator").unwrap();
                        let variant_match = line_caps.name("variant").unwrap();

                        let Some(vn_caps) = vehicle_page_config.short_vehicle_number_extractor.captures(vehicle_number_match.as_str()) else {
                            eprintln!("extractor regex did not match vehicle number {:?}; skipping", vehicle_number_match.as_str());
                            continue;
                        };
                        let mut short_vehicle_number = String::with_capacity(vehicle_number_line.len());
                        for subcap_opt in vn_caps.iter().skip(1) {
                            let Some(subcap) = subcap_opt else { continue };
                            short_vehicle_number.push_str(subcap.as_str());
                        }

                        vehicle_entries.push(VehicleEntry {
                            vehicle_number: VehicleNumber::from(short_vehicle_number),
                            full_vehicle_number: vehicle_number_match.as_str().to_owned(),
                            operator: operator_match.as_str().to_owned(),
                            variant: variant_match.as_str().to_owned(),
                        });
                    }
                } else if i == 2 {
                    // if scrapped: date scrapped
                    let mut scrap_date = String::new();
                    collect_text(*table_cell, &mut scrap_date);
                    out_of_service_since_opt = Some(scrap_date);
                }
            }

            // okay, assemble the vehicles
            let in_service_since = Some(
                in_service_since_opt
                    .unwrap_or_else(|| "?".to_owned())
            );
            let out_of_service_since = if is_scrapped {
                Some(
                    out_of_service_since_opt
                        .unwrap_or_else(|| "?".to_owned())
                )
            } else {
                None
            };

            let fixed_coupling: IndexSet<VehicleNumber> = if vehicle_entries.len() > 1 {
                vehicle_entries
                    .iter()
                    .map(|ve| ve.vehicle_number.clone())
                    .collect()
            } else {
                IndexSet::new()
            };

            for vehicle_entry in vehicle_entries {
                let mut other_data = vehicle_page_config.other_data.clone();
                other_data.insert(
                    "vollst\u{00E4}ndiger UIC-Code".to_owned(),
                    vehicle_entry.full_vehicle_number,
                );
                other_data.insert(
                    "Fahrzeughalter".to_owned(),
                    vehicle_entry.operator,
                );
                other_data.insert(
                    "Variante".to_owned(),
                    vehicle_entry.variant,
                );
                if let Some(serial_number) = serial_number_opt.as_ref() {
                    other_data.insert(
                        "Seriennummer".to_owned(),
                        serial_number.clone(),
                    );
                }
                if is_museal {
                    other_data.insert(
                        "Nutzung".to_owned(),
                        "museal".to_owned(),
                    );
                }

                let vehicle = VehicleInfo {
                    number: vehicle_entry.vehicle_number,
                    vehicle_class: vehicle_page_config.vehicle_class,
                    power_sources: vehicle_page_config.power_sources.clone(),
                    type_code: vehicle_page_config.type_code.clone(),
                    in_service_since: in_service_since.clone(),
                    out_of_service_since: out_of_service_since.clone(),
                    manufacturer: vehicle_page_config.manufacturer.clone(),
                    depot: None,
                    other_data,
                    fixed_coupling: fixed_coupling.clone(),
                };

                if let Some(known_vehicle) = number_to_vehicle.get(&vehicle.number) {
                    eprintln!(
                        "DUPE {:?}: we already know vehicle {:?} and are trying to add vehicle {:?}",
                        vehicle.number, known_vehicle, vehicle,
                    );
                    continue;
                }
                number_to_vehicle.insert(vehicle.number.clone(), vehicle);
            }
        }
    }

    let vehicles: Vec<VehicleInfo> = number_to_vehicle
        .into_iter()
        .map(|(_vn, v)| v)
        .collect();

    // write out to CBOR file
    {
        let mut out_file = File::create(&config.output_path)
            .expect("failed to open output file");
        ciborium::into_writer(&vehicles, &mut out_file)
            .expect("failed to write out CBOR data");
        out_file.flush()
            .expect("failed to flush output file");
    }

    ExitCode::SUCCESS
}
