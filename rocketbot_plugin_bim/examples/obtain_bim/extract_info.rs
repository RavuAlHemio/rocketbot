use std::io::Read;
use std::fs::File;

use form_urlencoded;
use indexmap::IndexSet;
use regex::Regex;
use rocketbot_plugin_bim::{VehicleInfo, VehicleNumber};
use sxd_document;
use sxd_document::dom::Element;
use sxd_xpath::{self, XPath};

use crate::{MatcherTransformerConfig, PageConfig};
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


fn value_match(matcher: &MatcherTransformerConfig, key: &str, value: &str) -> Option<String> {
    if !matcher.column_name_regex.is_match(key) {
        return None;
    }

    let mut result = value.to_owned();
    for replacer in &matcher.value_replacements {
        result = replacer.subject_regex
            .replace_all(&result, &replacer.replacement)
            .into_owned();
    }
    Some(result)
}


fn parse_vehicle_numbers(text_value: &str, number_separator_regex: &Option<Regex>) -> Vec<VehicleNumber> {
    let mut nums = Vec::new();
    if let Some(nsr) = number_separator_regex {
        for piece in nsr.split(text_value) {
            if let Ok(vn) = piece.parse() {
                nums.push(vn);
            }
            // skip invalid values
        }
    } else {
        if let Ok(vn) = text_value.parse() {
            nums.push(vn);
        }
        // skip the value if it is invalid
    }
    nums
}


pub(crate) fn row_data_to_trams(page_config: &PageConfig, row_data: Vec<(String, String)>) -> Vec<VehicleInfo> {
    let mut vehicles = Vec::new();
    let mut vehicle = VehicleInfo::new(0, page_config.type_code.clone());

    let all_rows = page_config.common_props.iter()
        .chain(row_data.iter().map(|(k, v)| (k, v)));

    let mut numbers_types: Vec<(VehicleNumber, String)> = Vec::new();
    for (key, val) in all_rows {
        if val.len() == 0 {
            continue;
        }

        let mut is_matched = false;

        for number_matcher in &page_config.type_specific_number_name_matchers {
            if let Some(number_text) = value_match(&number_matcher.matcher, &key, &val) {
                is_matched = true;
                let vehicle_numbers = parse_vehicle_numbers(&number_text, &page_config.number_separator_regex);
                for vehicle_number in vehicle_numbers {
                    numbers_types.push((vehicle_number, number_matcher.type_code.clone()));
                }
                break;
            }
        }

        if !is_matched {
            if let Some(nm) = &page_config.number_matcher {
                if let Some(number_text) = value_match(&nm, &key, &val) {
                    is_matched = true;
                    let vehicle_numbers = parse_vehicle_numbers(&number_text, &page_config.number_separator_regex);
                    for vehicle_number in vehicle_numbers {
                        numbers_types.push((vehicle_number, page_config.type_code.clone()));
                    }
                }
            }
        }

        if !is_matched {
            if let Some(issm) = &page_config.in_service_since_matcher {
                if let Some(iss) = value_match(&issm, &key, &val) {
                    is_matched = true;
                    vehicle.in_service_since = Some(iss);
                }
            }
        }

        if !is_matched {
            if let Some(oossm) = &page_config.out_of_service_since_matcher {
                if let Some(ooss) = value_match(&oossm, &key, &val) {
                    is_matched = true;
                    vehicle.out_of_service_since = Some(ooss);
                }
            }
        }

        if !is_matched {
            if let Some(mm) = &page_config.manufacturer_matcher {
                if let Some(m) = value_match(&mm, &key, &val) {
                    is_matched = true;
                    vehicle.manufacturer = Some(m);
                }
            }
        }

        if !is_matched {
            vehicle.other_data.insert(key.clone(), val.clone());
        }
    }

    let fixed_coupling_partners: IndexSet<VehicleNumber> = if page_config.fixed_couplings {
        numbers_types
            .iter()
            .map(|(num, _tc)| *num)
            .collect()
    } else {
        IndexSet::new()
    };

    for (vehicle_number, vehicle_type_code) in &numbers_types {
        vehicle.number = *vehicle_number;
        vehicle.type_code = vehicle_type_code.clone();

        vehicle.fixed_coupling = fixed_coupling_partners.clone();

        vehicles.push(vehicle.clone());
    }

    vehicles
}


pub(crate) fn process_table<F>(vehicles: &mut Vec<VehicleInfo>, table: Element, page_config: &PageConfig, mut row_data_to_vehicles: F)
    where F: FnMut(&PageConfig, Vec<(String, String)>) -> Vec<VehicleInfo>
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

            let mut these_vehicles = row_data_to_vehicles(&page_config, row_data);
            vehicles.append(&mut these_vehicles);
        }
    }
}


pub(crate) async fn process_page<F, G>(page_url_pattern: &str, page_config: &PageConfig, parser: &mut WikiParser, mut process_table: F, row_data_to_vehicles: G) -> Vec<VehicleInfo>
    where
        F : FnMut(&mut Vec<VehicleInfo>, Element, &PageConfig, G),
        G : FnMut(&PageConfig, Vec<(String, String)>) -> Vec<VehicleInfo> + Copy,
{
    let page_json = obtain_content(page_url_pattern, &page_config.title).await;

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
            process_table(&mut vehicles, table_elem, &page_config, row_data_to_vehicles);
        }
    }

    vehicles
}
