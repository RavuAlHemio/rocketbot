//! Obtains vehicle databases from seznam-autobusu.cz (Czechia) or evidencia-dopravcov.eu
//! (Slovakia).


use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env::args_os;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use indexmap::IndexSet;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest;
use rocketbot_bim_common::{VehicleClass, VehicleInfo, VehicleNumber};
use scraper::{ElementRef, Html, Node, Selector};
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
            "[ *\\t]*",
            "(?:",
                "\\+",
                "[0-9]+",
                "[ *\\t]*",
            ")*",
        ")",
    "\\}",
)).expect("failed to parse fixed coupling regex"));
static WHITESPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "\\s+",
)).expect("failed to parse whitespace regex"));
static DATE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?P<first>[0-9]+)",
    "(?:",
        "\\.",
        "(?P<a_month>[0-9]+)",
        "\\.",
        "(?P<a_year>[0-9]+)",
    "|",
        "/",
        "(?P<b_year>[0-9]+)",
    ")?",
)).expect("failed to parse date regex"));


#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct Config {
    pub pages: Vec<String>,
    pub multiline_tables: bool,
    pub output_path: String,
    #[serde(default)] pub type_mapping: HashMap<String, TypeInfo>,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct TypeInfo {
    pub type_code: String,
    pub manufacturer: Option<String>,
    pub vehicle_class: VehicleClass,
    #[serde(default)] pub other_data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VehicleInfoBuilder {
    number: Option<VehicleNumber>,
    type_code: Option<String>,
    vehicle_class: Option<VehicleClass>,
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
            vehicle_class: None,
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

    pub fn vehicle_class(&mut self, vehicle_class: VehicleClass) -> &mut Self {
        self.vehicle_class = Some(vehicle_class);
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

    pub fn modify_with_type_mapping(&mut self, type_mapping: &HashMap<String, TypeInfo>) {
        if let Some(type_code) = &self.type_code {
            if let Some(tc) = type_mapping.get(type_code) {
                self.manufacturer = tc.manufacturer.clone();
                self.type_code = Some(tc.type_code.clone());
                self.vehicle_class(tc.vehicle_class);
                for (k, v) in &tc.other_data {
                    self.other_data
                        .entry(k.clone())
                        .or_insert_with(|| v.clone());
                }
            }
        }
    }

    pub fn try_build(self) -> Result<VehicleInfo, Self> {
        let number = match &self.number {
            Some(n) => n.clone(),
            None => return Err(self),
        };
        let vehicle_class = match self.vehicle_class {
            Some(vc) => vc,
            None => return Err(self),
        };
        let type_code = match self.type_code {
            Some(tc) => tc,
            None => return Err(self),
        };
        Ok(VehicleInfo {
            number,
            type_code,
            vehicle_class,
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


fn text_of_first_text_child(n: &ElementRef) -> Option<String> {
    for child in n.children() {
        if let Node::Text(text_child) = child.value() {
            return Some(text_child.to_string());
        }
    }
    None
}


fn date_tuple(s: &str) -> Option<(u64, u64, u64)> {
    if let Some(caps) = DATE_RE.captures(s) {
        let first_m = caps.name("first").expect("first not captured");
        let first = match first_m.as_str().parse() {
            Ok(f) => f,
            Err(_) => return None,
        };

        if let Some(a_month_m) = caps.name("a_month") {
            let a_year_m = caps.name("a_year").expect("a_month captured but a_year not");

            let a_month = match a_month_m.as_str().parse() {
                Ok(m) => m,
                Err(_) => return None,
            };
            let a_year = match a_year_m.as_str().parse() {
                Ok(y) => y,
                Err(_) => return None,
            };
            Some((a_year, a_month, first))
        } else if let Some(b_year_m) = caps.name("b_year") {
            let b_year = match b_year_m.as_str().parse() {
                Ok(y) => y,
                Err(_) => return None,
            };
            Some((b_year, first, 0))
        } else if first >= 1000 {
            Some((first, 0, 0))
        } else {
            None
        }
    } else {
        None
    }
}


fn compare_date_strings(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(l), Some(r)) => {
            // if one is a question mark, assume it to be older
            if l == "?" && r != "?" {
                return Ordering::Less;
            } else if l != "?" && r == "?" {
                return Ordering::Greater;
            }

            // try to interpret them as dates
            let l_tuple = date_tuple(l);
            let r_tuple = date_tuple(r);
            if let Some(l_date) = l_tuple {
                if let Some(r_date) = r_tuple {
                    return l_date.cmp(&r_date);
                }
            }

            // naive string comparison
            l.cmp(r)
        },
        (None, None) => Ordering::Equal,
    }

}


fn compare_age(left: &VehicleInfo, right: &VehicleInfo) -> Ordering {
    compare_date_strings(
        left.out_of_service_since.as_ref().map(|o| o.as_str()),
        right.out_of_service_since.as_ref().map(|o| o.as_str()),
    )
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
    let last_plate_span_sel = Selector::parse("span.\\000031")
        .expect("failed to parse last-plate-span selector");
    let last_number_span_sel = Selector::parse("span.\\000031.number")
        .expect("failed to parse last-number-span selector");
    let next_link_sel = Selector::parse(".next-link[href]")
        .expect("failed to parse next-link selector");

    let mut vehicles = Vec::new();
    for page_url in &config.pages {
        let mut cur_page_url = reqwest::Url::parse(&page_url)
            .expect("failed to parse page URL");
        loop {
            eprintln!("fetching {}", cur_page_url);

            let page_bytes = obtain_page_bytes(cur_page_url.as_str()).await;
            let page_string = String::from_utf8(page_bytes)
                .expect("failed to decode page as UTF-8");
            let html = Html::parse_document(&page_string);

            let mut current_vehicle = VehicleInfoBuilder::new();
            for car_line in html.root_element().select(&car_line_sel) {
                if config.multiline_tables {
                    let tr_classes: HashSet<&str> = car_line.value().classes().collect();
                    if tr_classes.contains("first-line") {
                        // new vehicle!
                        current_vehicle.modify_with_type_mapping(&config.type_mapping);
                        if let Ok(veh) = current_vehicle.try_build() {
                            vehicles.push(veh);
                        }
                        current_vehicle = VehicleInfoBuilder::new();
                    }

                    for td in car_line.select(&td_sel) {
                        let td_classes: HashSet<&str> = td.value().classes().collect();
                        if td_classes.contains("manufacturer-type") {
                            // first text child is the manufacturer
                            let manuf = text_of_first_text_child(&td);
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
                            // do we have a fixed-coupling specification in the note?
                            if let Some(caps) = FIXED_COUPLING_RE.captures(&note) {
                                let mut fixed_coupling = IndexSet::new();
                                let couple_strs = caps
                                    .name("coupling").expect("coupling not captured")
                                    .as_str()
                                    .split("+");
                                for couple_str in couple_strs {
                                    let couple_str_no_asterisk = couple_str.trim_end_matches([' ', '*', '\t']);
                                    let couple = VehicleNumber::from_string(couple_str_no_asterisk.to_owned());
                                    fixed_coupling.insert(couple);
                                }
                                current_vehicle.fixed_coupling(fixed_coupling);

                                // remove the fixed coupling info from the note
                                let caps_range = caps.get(0).unwrap().range();
                                std::mem::drop(caps);
                                note.replace_range(caps_range, "");
                            }
                            current_vehicle.other_data("Anmerkung", trim_text(&note));
                        } else if td_classes.contains("plates") {
                            let latest_plate_span = td.select(&last_plate_span_sel).nth(0);
                            if let Some(lps) = latest_plate_span {
                                let lps_text = text_of_first_text_child(&lps);
                                if let Some(t) = lps_text {
                                    current_vehicle.other_data("Kennzeichen", t);
                                }
                            }
                        } else if td_classes.contains("numbers") {
                            let latest_num_span = td.select(&last_number_span_sel).nth(0);
                            if let Some(lns) = latest_num_span {
                                // take text from first text child
                                // (this ignores Roman numerals in <sup> tags)
                                let lns_text = text_of_first_text_child(&lns);
                                if let Some(t) = lns_text {
                                    let lns_number = VehicleNumber::from_string(t);
                                    current_vehicle.number(lns_number);
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
                } else {
                    // single-line tables -- much easier
                    let mut current_vehicle = VehicleInfoBuilder::new();

                    for (i, td) in car_line.select(&td_sel).enumerate() {
                        let td_classes: HashSet<&str> = td.value().classes().collect();
                        if td_classes.contains("plates") {
                            let plate: String = trim_text(&td.text().collect::<String>());
                            if plate.len() > 0 {
                                current_vehicle.other_data("Kennzeichen", trim_text(&plate));
                            }
                        } else if td_classes.contains("numbers") {
                            // last non-empty text child is the number
                            // (ignore superscripted Roman numerals)
                            let number_string_opt = td.children()
                                .filter_map(|c| c.value().as_text())
                                .map(|t| trim_text(&*t))
                                .filter(|t| t.len() > 0)
                                .last();
                            if let Some(number_string) = number_string_opt {
                                let num = trim_text(&number_string).into();
                                current_vehicle.number(num);
                            }
                        // no CSS classes beyond this point :-(
                        } else if i == 4 {
                            // type
                            let type_string: String = trim_text(&td.text().collect::<String>());
                            current_vehicle.type_code(type_string);
                        } else if i == 5 {
                            // dates of entry: construction, (delivery), [in service]
                            let mut texts = Vec::with_capacity(3);
                            for child in td.children() {
                                if let Some(t) = child.value().as_text() {
                                    texts.push(trim_text(t));
                                }
                            }
                            for text in texts {
                                if let Some(unpfx) = text.strip_prefix('[') {
                                    if let Some(infix) = unpfx.strip_suffix(']') {
                                        current_vehicle.in_service_since(infix);
                                    }
                                }
                            }
                        } else if i == 6 {
                            // dates of exit: (inactivation), out of service, [liquidation]
                            let mut texts = Vec::with_capacity(3);
                            for child in td.children() {
                                if let Some(t) = child.value().as_text() {
                                    texts.push(trim_text(t));
                                }
                            }
                            for text in texts {
                                if !text.starts_with('(') && !text.starts_with('[') {
                                    current_vehicle.out_of_service_since(text);
                                }
                            }
                        }
                    }

                    current_vehicle.modify_with_type_mapping(&config.type_mapping);
                    match current_vehicle.try_build() {
                        Ok(veh) => {
                            vehicles.push(veh);
                        },
                        Err(veh_builder) => {
                            eprintln!("incomplete vehicle: {:?}", veh_builder);
                        },
                    }
                }
            }

            if config.multiline_tables {
                // construct final vehicle
                current_vehicle.modify_with_type_mapping(&config.type_mapping);
                if let Ok(veh) = current_vehicle.try_build() {
                    vehicles.push(veh);
                }
            }

            // does another page await us?
            let next_link = html.root_element().select(&next_link_sel)
                .nth(0);
            if let Some(nl) = &next_link {
                if let Some(href) = nl.value().attr("href") {
                    match cur_page_url.join(href) {
                        Ok(u) => {
                            cur_page_url = u;
                            continue;
                        },
                        Err(e) => {
                            eprintln!("failed to join {:?} with {:?}: {}", cur_page_url.as_str(), href, e);
                            // assume no more page
                        },
                    }
                }
            }

            // no more page
            break;
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
            // remove the older one
            match compare_age(left, right) {
                Ordering::Less|Ordering::Equal => {
                    vehicles.remove(i - 1);
                },
                Ordering::Greater => {
                    vehicles.remove(i);
                },
            }
        } else {
            i += 1;
        }
    }

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        serde_json::to_writer_pretty(f, &vehicles)
            .expect("failed to write vehicles");
    }
}
