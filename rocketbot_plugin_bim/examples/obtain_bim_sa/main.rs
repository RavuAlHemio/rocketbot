use std::collections::{BTreeMap, HashMap, HashSet};
use std::env::args_os;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

use indexmap::IndexSet;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest;
use rocketbot_plugin_bim::{VehicleInfo, VehicleNumber};
use scraper::{Html, Node, Selector};
use serde::{Deserialize, Serialize};
use serde_json;


static DATE_RANGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "\\s*",
    "\\(",
        "(?:",
            "od ",
            "(?P<date_from>.+)",
        "|",
            "(?P<range_from>.+)",
            " - ",
            "(?P<range_to>.+)",
        ")",
    "\\)",
    "\\s*",
    "$",
)).expect("failed to parse date range regex"));
static FIXED_COUPLING_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "\\{",
        "(?P<coupling>",
            "[0-9]+",
            "(?:",
                "\\+",
                "[0-9]+",
            ")*",
        ")",
    "\\}",
    "\\s*",
    "$",
)).expect("failed to parse fixed coupling regex"));
static WHITESPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "\\s+",
)).expect("failed to parse whitespace regex"));


#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct Config {
    pub pages: Vec<String>,
    pub output_path: String,
    #[serde(default)] pub type_mapping: HashMap<String, TypeInfo>,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct TypeInfo {
    pub type_code: String,
    pub manufacturer: Option<String>,
}

struct VehicleInfoBuilder {
    number: Option<VehicleNumber>,
    type_code: Option<String>,
    in_service_since: Option<String>,
    out_of_service_since: Option<String>,
    manufacturer: Option<String>,
    other_data: BTreeMap<String, String>,
    fixed_coupling: IndexSet<VehicleNumber>,
}
impl VehicleInfoBuilder {
    pub fn new() -> Self {
        Self {
            number: None,
            type_code: None,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: BTreeMap::new(),
            fixed_coupling: IndexSet::new(),
        }
    }

    pub fn number(&mut self, number: VehicleNumber) -> &mut Self {
        self.number = Some(number);
        self
    }

    pub fn type_code<T: Into<String>>(&mut self, type_code: T) -> &mut Self {
        self.type_code = Some(type_code.into());
        self
    }

    pub fn in_service_since<S: Into<String>>(&mut self, since: S) -> &mut Self {
        self.in_service_since = Some(since.into());
        self
    }

    pub fn out_of_service_since<S: Into<String>>(&mut self, since: S) -> &mut Self {
        self.out_of_service_since = Some(since.into());
        self
    }

    pub fn manufacturer<M: Into<String>>(&mut self, manuf: M) -> &mut Self {
        self.manufacturer = Some(manuf.into());
        self
    }

    pub fn other_data<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> &mut Self {
        self.other_data.insert(key.into(), value.into());
        self
    }

    pub fn fixed_coupling<C: IntoIterator<Item = VehicleNumber>>(&mut self, coupling: C) -> &mut Self {
        self.fixed_coupling = coupling.into_iter().collect();
        self
    }

    pub fn try_build(self) -> Result<VehicleInfo, Self> {
        let number = match self.number {
            Some(n) => n,
            None => return Err(self),
        };
        let type_code = match self.type_code {
            Some(tc) => tc,
            None => return Err(self),
        };
        Ok(VehicleInfo {
            number,
            type_code,
            in_service_since: self.in_service_since,
            out_of_service_since: self.out_of_service_since,
            manufacturer: self.manufacturer,
            other_data: self.other_data,
            fixed_coupling: self.fixed_coupling,
        })
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


fn trim_text(s: &str) -> String {
    let replaced = WHITESPACE_RE.replace_all(s, " ");
    replaced.trim().to_owned()
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_sa.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let car_line_sel = Selector::parse("table.cars-table tr.car-line")
        .expect("failed to parse car-line selector");
    let td_sel = Selector::parse("td")
        .expect("failed to parse td selector");
    let b_sel = Selector::parse("b")
        .expect("failed to parse b selector");
    let dates_span_sel = Selector::parse("span.dates")
        .expect("failed to parse dates-span selector");
    let last_number_span_sel = Selector::parse("span.\\000031.number")
        .expect("failed to parse last-number-span selector");
    let next_link_sel = Selector::parse(".next-link")
        .expect("failed to parse next-link selector");

    let mut vehicles = Vec::new();
    for page_url in &config.pages {
        let mut page_number: usize = 1;
        loop {
            let mut page_page_url = reqwest::Url::parse(&page_url)
                .expect("failed to parse page URL");
            page_page_url.query_pairs_mut()
                .append_pair("page", &page_number.to_string());

            let page_bytes = obtain_page_bytes(page_page_url.as_str()).await;
            let page_string = String::from_utf8(page_bytes)
                .expect("failed to decode page as UTF-8");
            let html = Html::parse_document(&page_string);

            let mut current_vehicle = VehicleInfoBuilder::new();
            for car_line in html.root_element().select(&car_line_sel) {
                let tr_classes: HashSet<&str> = car_line.value().classes().collect();
                if tr_classes.contains("first-line") {
                    // new vehicle!

                    if let Ok(veh) = current_vehicle.try_build() {
                        vehicles.push(veh);
                    }
                    current_vehicle = VehicleInfoBuilder::new();
                }

                for td in car_line.select(&td_sel) {
                    let td_classes: HashSet<&str> = td.value().classes().collect();
                    if td_classes.contains("manufacturer-type") {
                        // first text child is the manufacturer
                        let mut manuf = None;
                        for child in td.children() {
                            if let Node::Text(text_child) = child.value() {
                                manuf = Some(text_child.to_string());
                                break;
                            }
                        }
                        if let Some(m) = manuf {
                            current_vehicle.manufacturer(trim_text(&m));
                        }

                        // last <b> child's text is the current type code
                        let last_b_child = td.select(&b_sel).last();
                        if let Some(lbc) = last_b_child {
                            let text: String = lbc.text().collect();
                            current_vehicle.type_code(text);
                        }

                        // next two columns are date built and date scrapped; skip these
                        // as we are more interested in "entered service" and "left service" with company
                    } else if td_classes.contains("note") {
                        let mut note: String = td.text().collect();
                        // do we have a fixed-coupling specification at the end of the note?
                        if let Some(caps) = FIXED_COUPLING_RE.captures(&note) {
                            let mut fixed_coupling = IndexSet::new();
                            let couple_strs = caps
                                .name("coupling").expect("coupling not captured")
                                .as_str()
                                .split("+");
                            let mut failed = false;
                            for couple_str in couple_strs {
                                if let Ok(couple) = VehicleNumber::from_str(couple_str) {
                                    fixed_coupling.insert(couple);
                                } else {
                                    failed = true;
                                    break;
                                }
                            }
                            if !failed {
                                current_vehicle.fixed_coupling(fixed_coupling);

                                // remove the fixed coupling info from the note
                                let caps_range = caps.get(0).unwrap().range();
                                std::mem::drop(caps);
                                note.replace_range(caps_range, "");
                            }
                        }
                        current_vehicle.other_data("Anmerkung", trim_text(&note));
                    } else if td_classes.contains("numbers") {
                        let latest_num_span = td.select(&last_number_span_sel).nth(0);
                        if let Some(lns) = latest_num_span {
                            // take text from first text child
                            // (this ignores Roman numerals in <sup> tags)
                            let mut lns_text = None;
                            for lns_child in lns.children() {
                                if let Node::Text(tc) = lns_child.value() {
                                    lns_text = Some(tc.to_string());
                                }
                            }
                            if let Some(t) = lns_text {
                                if let Ok(lns_number) = VehicleNumber::from_str(&t) {
                                    current_vehicle.number(lns_number);
                                }
                            }
                        }
                    } else if td_classes.contains("current-operator") {
                        let dates_span = td.select(&dates_span_sel).nth(0);
                        if let Some(ds) = dates_span {
                            let ds_text: String = ds.text().collect();
                            if let Some(caps) = DATE_RANGE_RE.captures(&ds_text) {
                                if let Some(df) = caps.name("date_from") {
                                    current_vehicle.in_service_since(df.as_str());
                                } else if let Some(rf) = caps.name("range_from") {
                                    let range_to = caps.name("range_to").expect("captured range_from but not range_to");
                                    current_vehicle.in_service_since(rf.as_str());
                                    current_vehicle.out_of_service_since(range_to.as_str());
                                }
                                // there doesn't appear to be a "until only" format
                            }
                        }
                    } else if td_classes.contains("depots") {
                        // first <b> child's text is the depot abbreviation
                        let depot_b = td.select(&b_sel).nth(0);
                        if let Some(db) = depot_b {
                            let db_text: String = db.text().collect();
                            current_vehicle.other_data("Remise", db_text);
                        }
                    }
                }
            }

            // construct final vehicle
            if let Ok(veh) = current_vehicle.try_build() {
                vehicles.push(veh);
            }

            // does another page await us?
            if !html.root_element().select(&next_link_sel).any(|_| true) {
                // no
                break;
            }

            // yes; increase the current page number
            page_number += 1;
        }
    }

    vehicles.sort_unstable_by_key(|v| v.number);

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        serde_json::to_writer_pretty(f, &vehicles)
            .expect("failed to write vehicles");
    }
}
