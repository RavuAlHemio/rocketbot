use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::fs::File;

use form_urlencoded;
use indexmap::IndexSet;
use regex::Regex;
use rocketbot_bim_common::{PowerSource, VehicleInfo, VehicleNumber};
use rocketbot_mediawiki_parsing::WikiParser;
use sxd_document;
use sxd_document::dom::Element;
use sxd_xpath::{self, XPath};

use crate::{MatcherTransformerConfig, PageConfig};


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
        eprintln!("obtaining {:?} from URL {:?}", page_title, url);
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
            nums.push(piece.to_owned().into());
            // skip invalid values
        }
    } else {
        nums.push(text_value.to_owned().into());
        // skip the value if it is invalid
    }
    nums
}


pub(crate) fn row_data_to_trams(page_config: &PageConfig, row_data: Vec<(String, String)>) -> BTreeMap<VehicleNumber, VehicleInfo> {
    let mut vehicles = BTreeMap::new();
    let mut vehicle = VehicleInfo::new("0".to_owned().into(), page_config.vehicle_class, page_config.type_code.clone());

    vehicle.depot = page_config.default_depot.clone();

    let all_props: Vec<(&String, &String)> = page_config.common_props.iter()
        .chain(row_data.iter().map(|(k, v)| (k, v)))
        .collect();

    let mut type_code = page_config.type_code.clone();
    if let Some(type_code_matcher) = &page_config.type_code_matcher {
        for (key, val) in &all_props {
            if let Some(matched_type_code) = value_match(type_code_matcher, key, val) {
                type_code = matched_type_code;
            }
        }
    }

    let mut numbers_types_powersources: Vec<(VehicleNumber, String, BTreeSet<PowerSource>)> = Vec::new();
    for (key, val) in &all_props {
        if val.len() == 0 {
            continue;
        }

        let mut is_matched = false;

        for number_matcher in &page_config.type_specific_number_name_matchers {
            if let Some(number_text) = value_match(&number_matcher.matcher, &key, &val) {
                is_matched = true;
                let vehicle_numbers = parse_vehicle_numbers(&number_text, &page_config.number_separator_regex);
                for vehicle_number in vehicle_numbers {
                    // type-specific matcher type code trumps general type code
                    let power_sources = if number_matcher.power_sources.len() > 0 {
                        number_matcher.power_sources.clone()
                    } else {
                        page_config.power_sources.clone()
                    };
                    numbers_types_powersources.push((vehicle_number, number_matcher.type_code.clone(), power_sources));
                }
                break;
            }
        }

        if !is_matched {
            if let Some(type_code_matcher) = &page_config.type_code_matcher {
                if let Some(_type_code) = value_match(&type_code_matcher, &key, &val) {
                    is_matched = true;
                }
            }
        }

        if !is_matched {
            if let Some(nm) = &page_config.number_matcher {
                if let Some(number_text) = value_match(&nm, &key, &val) {
                    is_matched = true;
                    let vehicle_numbers = parse_vehicle_numbers(&number_text, &page_config.number_separator_regex);
                    for vehicle_number in vehicle_numbers {
                        numbers_types_powersources.push((vehicle_number, type_code.clone(), page_config.power_sources.clone()));
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
            if let Some(dm) = &page_config.depot_matcher {
                if let Some(d) = value_match(&dm, &key, &val) {
                    is_matched = true;
                    vehicle.depot = Some(d);
                }
            }
        }

        if !is_matched {
            vehicle.other_data.insert((*key).clone(), (*val).clone());
        }
    }

    let fixed_coupling_partners: IndexSet<VehicleNumber> = if page_config.fixed_couplings {
        numbers_types_powersources
            .iter()
            .map(|(num, _tc, _ps)| num.clone())
            .collect()
    } else {
        IndexSet::new()
    };

    for (vehicle_number, vehicle_type_code, power_sources) in &numbers_types_powersources {
        vehicle.number = vehicle_number.clone();

        if let Some(ctc) = &page_config.common_type_code {
            vehicle.type_code = ctc.clone();
        } else {
            vehicle.type_code = vehicle_type_code.clone();
        }

        vehicle.power_sources = power_sources.clone();

        if let Some(stcp) = &page_config.specific_type_code_property {
            vehicle.other_data.insert(stcp.clone(), vehicle_type_code.clone());
        }

        vehicle.fixed_coupling = fixed_coupling_partners.clone();

        vehicles.insert(vehicle.number.clone(), vehicle.clone());
    }

    vehicles
}


pub(crate) fn process_table<F>(vehicles: &mut BTreeMap<VehicleNumber, VehicleInfo>, table: Element, page_config: &PageConfig, mut row_data_to_vehicles: F)
    where F: FnMut(&PageConfig, Vec<(String, String)>) -> BTreeMap<VehicleNumber, VehicleInfo>
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


/// Walks up the XML tree starting from descendant_element, recursively collects the <section>s in
/// which this element is contained, and returns their headings (text within the first child of the
/// section if it is a <h#> element).
fn get_title_stack<'a>(descendant_element: Element<'a>) -> Vec<&'a str> {
    let mut current_element = descendant_element;
    let mut ret = Vec::new();
    loop {
        let parent_element_opt = current_element
            .parent()
            .and_then(|p| p.element());
        current_element = match parent_element_opt {
            Some(e) => e,
            None => {
                // it is done
                break;
            },
        };
        let current_name = current_element.name();
        if current_name.namespace_uri().is_some() {
            continue;
        }
        if current_name.local_part() != "section" {
            continue;
        }

        // this is a section element; find a heading child
        let first_child_opt = current_element
            .children()
            .iter()
            .filter_map(|c| c.element())
            .nth(0);
        let first_child = match first_child_opt {
            Some(fc) => fc,
            None => continue,
        };
        let child_name = first_child.name();
        if child_name.namespace_uri().is_some() {
            continue;
        }
        let child_local = child_name.local_part();
        if child_local.len() != 2 {
            continue;
        }
        if !child_local.starts_with('h') {
            continue;
        }
        if !child_local.chars().skip(1).all(|c| c.is_ascii_digit()) {
            continue;
        }

        // okay, we got a heading; remember its text
        let first_child_text_opt = first_child.children()
            .iter()
            .filter_map(|c| c.text())
            .nth(0);
        let first_child_text = match first_child_text_opt {
            Some(fct) => fct.text(),
            None => "",
        };
        ret.push(first_child_text);
    }
    ret.reverse();
    ret
}

fn section_stack_matches(section_stack: &[&str], section_stack_regexes: &[Regex]) -> bool {
    if section_stack.len() != section_stack_regexes.len() {
        return false;
    }
    for (section, regex) in section_stack.iter().zip(section_stack_regexes.iter()) {
        if !regex.is_match(section) {
            return false;
        }
    }
    true
}

pub(crate) async fn process_page<F, G>(page_url_pattern: &str, page_config: &PageConfig, parser: &mut WikiParser, page_cache: &mut HashMap<String, String>, mut process_table: F, row_data_to_vehicles: G) -> BTreeMap<VehicleNumber, VehicleInfo>
    where
        F : FnMut(&mut BTreeMap<VehicleNumber, VehicleInfo>, Element, &PageConfig, G),
        G : FnMut(&PageConfig, Vec<(String, String)>) -> BTreeMap<VehicleNumber, VehicleInfo> + Copy,
{
    let section_stack_regexes: Vec<Vec<Regex>> = page_config.section_stack_regexes
        .iter()
        .map(|ssr| ssr
            .iter()
            .map(|s| Regex::new(s).expect("failed to compile section stack regex"))
            .collect()
        )
        .collect();
    let page_json = if let Some(page_content) = page_cache.get(&page_config.title) {
        page_content.clone()
    } else {
        let fresh_page_content = obtain_content(page_url_pattern, &page_config.title).await;
        page_cache.insert(page_config.title.clone(), fresh_page_content.clone());
        fresh_page_content
    };

    // deserialize
    let page: serde_json::Value = serde_json::from_str(&page_json)
        .expect("failed to parse page JSON");

    // get title and body
    let page_dict = page["query"]["pages"]
        .as_array().expect("failed to get pages array")
        .get(0).expect("page array empty");
    let actual_title = page_dict["title"].as_str().expect("page title not a string");
    let rev0 = &page_dict["revisions"][0];
    let body_wikitext = if let Some(wt) = rev0["slots"]["main"]["content"].as_str() {
        // newer MediaWiki
        wt
    } else {
        // older MediaWiki
        rev0["content"].as_str().expect("page body not a string")
    };

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

    let mut vehicles = BTreeMap::new();
    if let sxd_xpath::Value::Nodeset(table_nodes) = tables {
        for table_node in table_nodes {
            let table_elem = table_node.element().expect("table node is not an element");

            if section_stack_regexes.len() > 0 {
                // we only want tables in specific sections
                let section_stack = get_title_stack(table_elem);
                eprintln!("  table in section {:?}", section_stack);
                if section_stack_regexes.iter().all(|ssr| !section_stack_matches(&section_stack, ssr)) {
                    // we are not interested in this table
                    continue;
                }
            }

            process_table(&mut vehicles, table_elem, &page_config, row_data_to_vehicles);
        }
    }

    vehicles
}
