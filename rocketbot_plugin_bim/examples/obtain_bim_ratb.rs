//! Obtains vehicle databases from ratb.stfp.net (Bucharest, Romania).


use std::collections::{BTreeMap, BTreeSet};
use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;

use once_cell::sync::Lazy;
use reqwest;
use rocketbot_bim_common::{VehicleClass, VehicleInfo, VehicleNumber};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub output_path: PathBuf,
    pub types: Vec<TypeConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct TypeConfig {
    pub type_code: String,
    pub first_page_url: String,
    #[serde(default)] pub manufacturer: Option<String>,
    #[serde(default)] pub other_properties: BTreeMap<String, String>,
}


static PAGING_TABLE_CELL_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(
    "table[noborder=\"\"][width=\"100%\"] tr td[align=\"left\"]"
).expect("failed to parse paging table cell selector"));
static VEHICLE_TABLE_CELL_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(
    "table[cellpadding=\"5\"][noborder=\"\"][width=\"99%\"] tr[align=\"center\"] td"
).expect("failed to parse vehicle table cell selector"));
static VEHICLE_NUMBER_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(
    "font[size=\"+2\"]"
).expect("failed to parse vehicle number selector"));
static DISPOSED_IMAGE_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(
    "img[title=\"Casat\"]"
).expect("failed to parse disposed image selector"));


fn extract_page_vehicles(
    doc: &Html,
    extant_vehicle_numbers: &mut BTreeSet<String>,
    disposed_vehicle_numbers: &mut BTreeSet<String>,
) {
    // find vehicle table cells
    let vehicle_table_cells = doc.root_element().select(&VEHICLE_TABLE_CELL_SELECTOR);
    for vehicle_table_cell in vehicle_table_cells {
        // get the vehicle number
        let vehicle_numbers: String = vehicle_table_cell.select(&VEHICLE_NUMBER_SELECTOR)
            .nth(0).expect("failed to find vehicle number")
            .text()
            .collect();
        let is_disposed = vehicle_table_cell.select(&DISPOSED_IMAGE_SELECTOR)
            .nth(0).is_some();
        for vehicle_number in vehicle_numbers.split(" si ") {
            if is_disposed {
                disposed_vehicle_numbers.insert(vehicle_number.to_owned());
            } else {
                extant_vehicle_numbers.insert(vehicle_number.to_owned());
            }
        }
    }
}


async fn obtain_page(url: Url) -> Html {
    let page_bytes = reqwest::get(url.clone())
        .await.expect("failed to obtain page")
        .bytes().await.expect("failed to obtain page bytes");
    let page_str = std::str::from_utf8(&page_bytes)
        .expect("failed to decode page bytes");
    Html::parse_document(page_str)
}


async fn get_vehicles_of_type(
    type_config: &TypeConfig,
    vehicle_number_to_extant: &mut BTreeMap<VehicleNumber, VehicleInfo>,
    vehicle_number_to_disposed: &mut BTreeMap<VehicleNumber, VehicleInfo>,
) {
    // obtain base page
    let base_page_url = Url::parse(&type_config.first_page_url)
        .expect("failed to parse base page URL");
    let base_page = obtain_page(base_page_url.clone()).await;

    let mut extant_vehicle_numbers = BTreeSet::new();
    let mut disposed_vehicle_numbers = BTreeSet::new();
    extract_page_vehicles(&base_page, &mut extant_vehicle_numbers, &mut disposed_vehicle_numbers);

    // find paging table's relevant cell
    let paging_table_cell = base_page.root_element().select(&PAGING_TABLE_CELL_SELECTOR)
        .nth(0).expect("did not find paging table cell");

    for child in paging_table_cell.children() {
        // only look at child elements
        let Some(child_elem) = child.value().as_element() else { continue };

        // break at first <br> (no more pagination links)
        if child_elem.name() == "br" {
            break;
        }

        if child_elem.name() == "a" {
            if let Some(where_to) = child_elem.attr("href") {
                // URL of a later page
                let paginated_url = base_page_url.join(where_to)
                    .expect("failed to join pagination URL to base page URL");

                // fetch that
                let paginated_page = obtain_page(paginated_url).await;

                // extract its vehicle numbers
                extract_page_vehicles(&paginated_page, &mut extant_vehicle_numbers, &mut disposed_vehicle_numbers);
            }
        }
    }

    // assemble vehicle database
    for (numbers, is_disposed) in [(&extant_vehicle_numbers, false), (&disposed_vehicle_numbers, true)] {
        for number in numbers {
            let mut vehicle = VehicleInfo::new(
                VehicleNumber::from_string(number.clone()),
                VehicleClass::Tram,
                type_config.type_code.clone(),
            );
            vehicle.in_service_since = Some("?".to_owned());
            if is_disposed {
                vehicle.out_of_service_since = Some("?".to_owned());
            }
            vehicle.manufacturer = type_config.manufacturer.clone();
            vehicle.other_data = type_config.other_properties.clone();

            if is_disposed {
                vehicle_number_to_disposed.insert(vehicle.number.clone(), vehicle);
            } else {
                vehicle_number_to_extant.insert(vehicle.number.clone(), vehicle);
            }
        }
    }
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_ratb.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    // load all vehicle types
    let mut vehicle_number_to_extant = BTreeMap::new();
    let mut vehicle_number_to_disposed = BTreeMap::new();
    for vehicle_type in config.types {
        get_vehicles_of_type(
            &vehicle_type,
            &mut vehicle_number_to_extant,
            &mut vehicle_number_to_disposed,
        ).await;
    }

    // merge (disposed first so they get overwritten by extant)
    let mut number_to_vehicle = BTreeMap::new();
    number_to_vehicle.append(&mut vehicle_number_to_disposed);
    number_to_vehicle.append(&mut vehicle_number_to_extant);

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
