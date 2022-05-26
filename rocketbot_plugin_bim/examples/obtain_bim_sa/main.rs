use std::collections::HashMap;
use std::env::args_os;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use reqwest;
use rocketbot_plugin_bim::VehicleInfo;
use scraper::{Html, Node, Selector};
use serde::{Deserialize, Serialize};
use serde_json;


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

    let row_wrapper_sel = Selector::parse("div.row-wrapper")
        .expect("failed to parse row-wrapper selector");
    let a_sel = Selector::parse("a")
        .expect("failed to parse a selector");
    let car_sel = Selector::parse("a.car")
        .expect("failed to parse car selector");
    let number_sel = Selector::parse("span.number")
        .expect("failed to parse number selector");

    let mut vehicles = Vec::new();
    for page_url in &config.pages {
        let page_bytes = obtain_page_bytes(page_url).await;
        let page_string = String::from_utf8(page_bytes)
            .expect("failed to decode page as UTF-8");
        let html = Html::parse_document(&page_string);

        // go through row wrappers
        let mut current_type = None;
        for row_wrapper in html.select(&row_wrapper_sel) {
            if row_wrapper.value().classes().any(|c| c == "type") {
                // new type!
                let type_link = match row_wrapper.select(&a_sel).nth(0) {
                    None => continue,
                    Some(tl) => tl,
                };

                // its first text child is the type
                let mut new_type = None;
                for child in type_link.children() {
                    if let Node::Text(t) = child.value() {
                        new_type = Some(t.trim());
                        break;
                    }
                }

                if let Some(nt) = new_type {
                    // is it mapped?
                    if let Some(mapped) = config.type_mapping.get(nt) {
                        // yes; use that
                        current_type = Some(mapped.clone());
                    } else {
                        // no; improvise
                        current_type = Some(TypeInfo {
                            type_code: nt.to_owned(),
                            manufacturer: None,
                        });
                    }
                }
            } else if row_wrapper.value().classes().any(|c| c == "clearfix") {
                let cur_type = match current_type.as_ref() {
                    Some(ct) => ct,
                    None => continue, // no vehicles without a type
                };

                // car row
                for car in row_wrapper.select(&car_sel) {
                    // classes of trams that we care about:
                    // * nzr = nezařazen = not in service
                    // * zar = zařazen = in regular service
                    // * dilny = dílny = in workshop/repair
                    // * doco = dočasně odstaven = out of service temporarily
                    // * sluz = služební = maintenance (non-passenger) vehicle

                    // classes of trams that we do not care about:
                    // * zrus = zrušen = scrapped
                    // * prod = prodán = sold
                    // * muz = muzeum = in museum
                    // * ods = odstaven = out of service long-term
                    // * vrak = vrak = total hull loss
                    // * nez = neznámý = unknown

                    let car_is_interesting = car.value().classes().any(|c|
                        c == "nzr" || c == "zar" || c == "dilny" || c == "doco" || c == "sluz"
                    );
                    let car_is_not_interesting = car.value().classes().any(|c|
                        c == "zrus" || c == "prod" || c == "muz" || c == "ods" || c == "vrak"
                        || c == "nez"
                    );

                    if car_is_not_interesting {
                        continue;
                    }
                    if !car_is_interesting {
                        // car is neither interesting nor not interesting
                        let classes: Vec<&str> = car.value().classes().collect();
                        eprintln!("warning: car of unknown interest; has classes: {:?}", classes);
                        continue;
                    }

                    let number_span = match car.select(&number_sel).nth(0) {
                        None => continue,
                        Some(ns) => ns,
                    };
  
                    // number is first text child of number span
                    // (sometimes there is a "<sup>IV</sup>" or similar after it)
                    let number_opt = None;
                    for child in number_span.children() {
                        if let Node::Text(t) = child.value() {
                            number_opt = Some(t.trim());
                            break;
                        }
                    }
                    let number = match number_opt {
                        Some(n) => n,
                        None => continue,
                    };
                    let number_u32: u32 = match number.parse() {
                        Ok(n) => n,
                        Err(_) => continue,
                    };

                    let mut vehicle = VehicleInfo::new(number_u32, cur_type.type_code.clone());
                    if let Some(manuf) = cur_type.manufacturer.as_ref() {
                        vehicle.manufacturer = Some(manuf.clone());
                    }
                    vehicles.push(vehicle);
                }
            }
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
