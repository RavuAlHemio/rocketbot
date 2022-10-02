use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::env::args_os;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use indexmap::IndexSet;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest;
use rocketbot_plugin_bim::{VehicleClass, VehicleInfo, VehicleNumber};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json;


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
    pub pages: Vec<PageInfo>,
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct PageInfo {
    pub url: String,
    pub type_code: String,
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


fn trimmed_text_to_string(text: scraper::element_ref::Text) -> String {
    let text_string: String = text
        .map(|t| t.trim())
        .collect();
    text_string
        .trim().to_owned()
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
            None => PathBuf::from("obtain_bim_sl.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let class_data_table_sel = Selector::parse("div#classdata > table")
        .expect("failed to parse class-data-table selector");
    let class_data_row_sel = Selector::parse("tr")
        .expect("failed to parse class-data-row selector");
    let table_sel = Selector::parse("table#ClassMembersTable")
        .expect("failed to parse vehicle-table selector");
    let header_field_sel = Selector::parse("thead th")
        .expect("failed to parse header field selector");
    let vehicle_row_sel = Selector::parse("tbody tr")
        .expect("failed to parse data row selector");
    let td_sel = Selector::parse("td")
        .expect("failed to parse td selector");

    let mut vehicles: Vec<VehicleInfo> = Vec::new();
    for page_info in &config.pages {
        eprintln!("fetching {}", page_info.url);

        let page_bytes = obtain_page_bytes(&page_info.url).await;
        let page_string = String::from_utf8(page_bytes)
            .expect("failed to decode page as UTF-8");
        let html = Html::parse_document(&page_string);

        // find the common properties
        let mut common_props = BTreeMap::new();
        let common_table = html.root_element().select(&class_data_table_sel)
            .nth(0);
        if let Some(ct) = common_table {
            for common_row in ct.select(&class_data_row_sel) {
                let common_cells: Vec<ElementRef> = common_row.select(&td_sel).collect();
                if common_cells.len() >= 2 {
                    let key = trimmed_text_to_string(common_cells[0].text());
                    let value = trimmed_text_to_string(common_cells[1].text());
                    common_props.insert(key, value);
                }
            }
        }

        // find the vehicle table
        let table = html.root_element().select(&table_sel)
            .nth(0).expect("table not found");

        // find the header fields
        let mut headers: Vec<String> = Vec::new();
        for header_field in table.select(&header_field_sel) {
            headers.push(trimmed_text_to_string(header_field.text()));
        }

        // find the vehicles
        for vehicle_row in table.select(&vehicle_row_sel) {
            let mut kvps: HashMap<String, String> = HashMap::new();
            for (header, data_field) in headers.iter().zip(vehicle_row.select(&td_sel)) {
                let data_string = trimmed_text_to_string(data_field.text());
                if data_string.len() > 0 {
                    kvps.insert(header.clone(), data_string);
                }
            }

            // do we have a formation?
            let mut numbers: Vec<String> = Vec::new();
            if let Some(formation) = kvps.get("Formation") {
                if formation.len() > 0 {
                    numbers.extend(
                        formation.split(',')
                            .map(|n| n.trim().to_owned())
                    );
                }
            }

            if numbers.len() == 0 {
                // no; get the "raw" number
                if let Some(num) = kvps.get("Number") {
                    numbers.push(num.trim().to_owned());
                }
            }

            // collect all properties
            let mut vehicle_props = BTreeMap::new();
            vehicle_props.extend(
                common_props.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
            );
            vehicle_props.extend(
                kvps.iter()
                    .filter(|(k, _v)| *k != "Formation" && *k != "Number")
                    .map(|(k, v)| (k.clone(), v.clone()))
            );

            let builder_opt = vehicle_props.remove("Builder");

            // insert
            for number in &numbers {
                let mut vehicle = VehicleInfoBuilder::new();
                vehicle
                    .number(number.clone().into())
                    .type_code(&page_info.type_code)
                    .vehicle_class(page_info.vehicle_class);
                if numbers.len() > 1 {
                    vehicle.fixed_coupling(numbers.iter().map(|n| n.clone().into()));
                }
                if let Some(builder) = &builder_opt {
                    vehicle.manufacturer(builder);
                }

                for (k, v) in &vehicle_props {
                    vehicle.other_data(k, v);
                }

                let finished_vehicle = vehicle.try_build()
                    .expect("failed to build vehicle");
                vehicles.push(finished_vehicle);
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
