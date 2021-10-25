use std::collections::BTreeSet;
use std::io::Read;
use std::fs::File;

use form_urlencoded;
use rocketbot_plugin_bim::VehicleInfo;
use sxd_document;
use sxd_document::dom::Element;
use sxd_xpath::{self, XPath};

use crate::wiki_parsing::WikiParser;


async fn obtain_content(page_url_pattern: &str, page_title: &str) -> String {
    if page_url_pattern.starts_with("file://") {
        let page_title_no_slashes = page_title.replace("/", "");
        let path = page_url_pattern
            .strip_prefix("file://").unwrap()
            .replace("{TITLE}", &page_title_no_slashes);
        let mut f = File::open(path)
            .expect("failed to open file");
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)
            .expect("failed to read bytes");
        String::from_utf8(bytes)
            .expect("failed to decode as UTF-8")
    } else {
        let page_title_encoded: String = form_urlencoded::byte_serialize(page_title.as_bytes())
            .collect();
        let url = page_url_pattern.replace("{TITLE}", &page_title_encoded);
        let response = reqwest::get(url).await
            .expect("failed to obtain response");
        let response_bytes = response.bytes().await
            .expect("failed to obtain response bytes");
        let response_bytes_vec = response_bytes.to_vec();
        String::from_utf8(response_bytes_vec)
            .expect("failed to decode response as UTF-8")
    }
}


fn compile_xpath(factory: &sxd_xpath::Factory, xpath_str: &str) -> XPath {
    factory.build(xpath_str)
        .expect("failed to parse XPath")
        .expect("XPath is None")
}


fn strip_prefix_or_dont<'a, 'b>(string: &'a str, putative_prefix: &'b str) -> &'a str {
    string.strip_prefix(putative_prefix).unwrap_or(string)
}
fn strip_suffix_or_dont<'a, 'b>(string: &'a str, putative_suffix: &'b str) -> &'a str {
    string.strip_suffix(putative_suffix).unwrap_or(string)
}


pub(crate) fn row_data_to_trams(type_code: &str, row_data: Vec<(String, String)>) -> Vec<VehicleInfo> {
    let mut vehicles = Vec::new();
    let mut vehicle = VehicleInfo::new(0, type_code.to_owned());
    let type_code_parts: Vec<&str> = type_code.split("/").collect();

    let mut numbers_types: Vec<(u32, &str)> = Vec::new();
    let mut fixed_coupling = false;
    for (key, val) in &row_data {
        if val.len() == 0 {
            continue;
        }

        if key == "Nummer" {
            for number_str in val.split("+") {
                if let Ok(number) = number_str.parse() {
                    numbers_types.push((number, type_code));
                }
            }
        } else if key == "Motorwagen" || key == "Steuerwagen" {
            // special case for fixed-composition V/v (and probably X/x) trains
            // V is the one with an engine, v is the one with the driver's cabin
            let this_type_code = if key == "Motorwagen" { type_code_parts[0] } else { type_code_parts[1] };
            if let Ok(number) = val.parse() {
                numbers_types.push((number, this_type_code));
            }

            // also, this creates a fixed coupling
            fixed_coupling = true;
        } else if key == "Instandnahme" || key == "Inbetriebnahme" || key == "Genehmigung" || key == "Erstzulassung" {
            vehicle.in_service_since = Some(val.clone());
        } else if key == "Ausgemustert" || key == "Ausmusterung" {
            vehicle.out_of_service_since = Some(val.clone());
        } else if key == "Firma" {
            vehicle.manufacturer = Some(val.clone());
        } else {
            vehicle.other_data.insert(key.clone(), val.clone());
        }
    }

    let all_numbers: BTreeSet<u32> = numbers_types
        .iter()
        .map(|(num, _tc)| *num)
        .collect();

    for &(vehicle_number, vehicle_type_code) in &numbers_types {
        vehicle.number = vehicle_number;
        vehicle.type_code = vehicle_type_code.to_owned();

        if fixed_coupling {
            // add related vehicles
            let mut coupled_numbers = all_numbers.clone();
            coupled_numbers.remove(&vehicle_number);
            vehicle.fixed_coupling_with = coupled_numbers;
        } else {
            vehicle.fixed_coupling_with = BTreeSet::new();
        }

        vehicles.push(vehicle.clone());
    }

    vehicles
}


pub(crate) fn process_table<F>(vehicles: &mut Vec<VehicleInfo>, table: Element, type_code: &str, mut row_data_to_vehicles: F)
    where F: FnMut(&str, Vec<(String, String)>) -> Vec<VehicleInfo>
{
    let xpath_factory = sxd_xpath::Factory::new();
    let table_head_xpath = compile_xpath(&xpath_factory, ".//th");
    let table_row_xpath = compile_xpath(&xpath_factory, ".//tr");
    let table_data_xpath = compile_xpath(&xpath_factory, ".//td");
    let context = sxd_xpath::Context::new();

    // find table headers
    let mut keys = Vec::new();
    let heads_value = table_head_xpath.evaluate(&context, table)
        .expect("failed to execute table head XPath");
    if let sxd_xpath::Value::Nodeset(heads) = heads_value {
        for head in heads.document_order() {
            keys.push(head.string_value());
        }
    }

    // find table rows
    let rows_value = table_row_xpath.evaluate(&context, table)
        .expect("failed to execute table row XPath");
    if let sxd_xpath::Value::Nodeset(rows) = rows_value {
        for row in rows.document_order() {
            // find data
            let mut row_data = Vec::new();

            let cells_value = table_data_xpath.evaluate(&context, row)
                .expect("failed to execute table data XPath");
            if let sxd_xpath::Value::Nodeset(cells) = cells_value {
                let cells_doc_order = cells.document_order();
                for (key, cell) in keys.iter().zip(cells_doc_order.iter()) {
                    let cell_text = cell.string_value();
                    row_data.push((key.clone(), cell_text));
                }
            }

            let mut these_vehicles = row_data_to_vehicles(type_code, row_data);
            vehicles.append(&mut these_vehicles);
        }
    }
}


pub(crate) async fn process_page<F, G>(page_url_pattern: &str, page_title: &str, parser: &mut WikiParser, mut process_table: F, row_data_to_vehicles: G) -> Vec<VehicleInfo>
    where
        F : FnMut(&mut Vec<VehicleInfo>, Element, &str, G),
        G : FnMut(&str, Vec<(String, String)>) -> Vec<VehicleInfo> + Copy,
{
    let page_json = obtain_content(page_url_pattern, page_title).await;

    // deserialize
    let page: serde_json::Value = serde_json::from_str(&page_json)
        .expect("failed to parse page JSON");

    // get title and body
    let page_dict = page["query"]["pages"].as_object()
        .expect("failed to get page dict")
        .values()
        .nth(0).expect("page dict empty");
    let actual_title = page_dict["title"].as_str().expect("page title not a string");
    let body_wikitext = page_dict["revisions"][0]["*"].as_str().expect("page body not a string");

    let type_code = strip_suffix_or_dont(
        strip_prefix_or_dont(actual_title, "Type "),
        " (Wien)"
    );

    // parse wikitext
    let parsed = parser.parse_article(actual_title, body_wikitext)
        .expect("failed to parse article");
    let parsed_no_doctype = parsed.strip_prefix("<!DOCTYPE html>\n").unwrap_or(&parsed);

    // load as XML
    let xml_package = sxd_document::parser::parse(&parsed_no_doctype)
        .expect("failed to parse processed wikitext as XML");
    let xml = xml_package.as_document();

    // find tables
    let tables_xpath = sxd_xpath::Factory::new().build(".//table")
        .expect("failed to parse tables XPath")
        .expect("failed to obtain XPath");
    let context = sxd_xpath::Context::new();
    let tables = tables_xpath.evaluate(&context, xml.root())
        .expect("failed to execute tables XPath");

    let mut vehicles = Vec::new();
    if let sxd_xpath::Value::Nodeset(table_nodes) = tables {
        for table_node in table_nodes {
            let table_elem = table_node.element().expect("table node is not an element");
            process_table(&mut vehicles, table_elem, type_code, row_data_to_vehicles);
        }
    }

    vehicles
}
