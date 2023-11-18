use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::io::{Cursor, Read, Seek};

use sxd_document::{QName, Package};
use sxd_document::dom::Element;
use zip::ZipArchive;

use crate::{Config, RgbaColor};


const BASE_DOC_PATH: &str = "xl/workbook.xml";
const BASE_DOC_REL_PATH: &str = "xl/_rels/workbook.xml.rels";
const STYLES_PATH: &str = "xl/styles.xml";
const STRINGS_PATH: &str = "xl/sharedStrings.xml";

const SSML_NSURL: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELS_NSURL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RELS_PACK_NSURL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellContents {
    pub text: String,
    pub background_color: RgbaColor,
}


fn parse_xml_from_zip<F: Read + Seek>(zip_archive: &mut ZipArchive<F>, path: &str) -> sxd_document::Package {
    let mut bytes = Vec::new();
    {
        let mut entry = match zip_archive.by_name(path) {
            Ok(e) => e,
            Err(e) => panic!("failed to find {:?} in .xlsx: {}", path, e),
        };
        if let Err(e) = entry.read_to_end(&mut bytes) {
            panic!("failed to extract {:?} from .xlsx: {}", path, e);
        }
    }
    let s = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => panic!("{:?} in .xlsx is not encoded in UTF-8", path),
    };
    match sxd_document::parser::parse(s) {
        Ok(p) => p,
        Err(e) => panic!("failed to parse {:?} in xlsx: {}", path, e),
    }
}

fn name_matches(name: QName, ns_uri: &str, local_name: &str) -> bool {
    name.namespace_uri() == Some(ns_uri) && name.local_part() == local_name
}

fn get_doc_elem(package: &Package) -> Option<Element> {
    package
        .as_document()
        .root()
        .children()
        .iter()
        .filter_map(|c| c.element())
        .nth(0)
}

fn obtain_strings<F: Read + Seek>(zip_archive: &mut ZipArchive<F>) -> Vec<String> {
    let xml = parse_xml_from_zip(zip_archive, STRINGS_PATH);
    let doc_elem = get_doc_elem(&xml)
        .expect("string document does not have a document element");
    if !name_matches(doc_elem.name(), SSML_NSURL, "sst") {
        panic!("string doc {:?} has wrong top-level element: {:?}", STRINGS_PATH, doc_elem.name());
    }

    let string_elems: Vec<Element> = doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "si"))
        .collect();
    let mut strings = Vec::new();
    for string_elem in string_elems {
        let mut text = String::new();
        let t_elems: Vec<Element> = string_elem
            .children()
            .iter()
            .filter_map(|c| c.element())
            .filter(|e| name_matches(e.name(), SSML_NSURL, "t"))
            .collect();
        for t_elem in t_elems {
            let t_elem_children = t_elem
                .children();
            let text_children = t_elem_children
                .iter()
                .filter_map(|c| c.text());
            for text_child in text_children {
                text.push_str(text_child.text());
            }
        }
        strings.push(text);
    }
    strings
}

fn obtain_style_to_fill<F: Read + Seek>(zip_archive: &mut ZipArchive<F>) -> HashMap<usize, RgbaColor> {
    let xml = parse_xml_from_zip(zip_archive, STYLES_PATH);
    let doc_elem = get_doc_elem(&xml)
        .expect("styles document does not have a document element");
    if !name_matches(doc_elem.name(), SSML_NSURL, "styleSheet") {
        panic!("styles doc {:?} has wrong top-level element: {:?}", STYLES_PATH, doc_elem.name());
    }

    let fill_elems: Vec<Element> = doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "fills"))
        .flat_map(|e| e.children())
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "fill"))
        .collect();
    let mut fill_colors = Vec::with_capacity(fill_elems.len());
    let fill_elems_len = fill_elems.len();
    for fill_elem in fill_elems {
        let pattern_fill = fill_elem
            .children()
            .iter()
            .filter_map(|c| c.element())
            .filter(|e| name_matches(e.name(), SSML_NSURL, "patternFill"))
            .nth(0)
            .expect("fill does not have patternFill child");
        let pattern_type = pattern_fill.attribute_value("patternType")
            .expect("patternFill does not have patternType");
        let rgba = if pattern_type == "none" {
            RgbaColor::none()
        } else if pattern_type == "gray125" {
            RgbaColor::gray125()
        } else if pattern_type == "solid" {
            let fg_color = pattern_fill
                .children()
                .iter()
                .filter_map(|c| c.element())
                .filter(|e| name_matches(e.name(), SSML_NSURL, "fgColor"))
                .nth(0)
                .expect("solid patternFill does not have a fgColor");
            let fb_rgb = fg_color
                .attribute_value("rgb")
                .expect("fgColor does not have rgb attribute");
            fb_rgb.parse()
                .expect("failed to parse fgColor as RGBA")
        } else {
            panic!("unknown patternType {:?}", pattern_type);
        };
        fill_colors.push(rgba);
    }
    assert_eq!(fill_colors.len(), fill_elems_len);

    // now, map styles to fills
    let mut style_to_fill = HashMap::new();
    let xf_elems: Vec<Element> = doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "cellXfs"))
        .flat_map(|e| e.children())
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "xf"))
        .collect();
    for (i, xf_elem) in xf_elems.into_iter().enumerate() {
        let fill_id: usize = xf_elem
            .attribute_value("fillId").expect("missing fillId")
            .parse().expect("failed to parse fillId");

        style_to_fill.insert(i, fill_colors[fill_id]);
    }

    style_to_fill
}

/// Converts spreadsheet coordinates (`"A2"`) to Cartesian coordinates (`(0, 1)`).
///
/// Spreadsheet coordinates are assumed to be 1-based (for rows)/A-based (columns) while Cartesian
/// coordinates are 0-based. The Cartesian value is returned as `(column, row)`. In contrast to
/// geometric tradition, and keeping with Latin reading order, row numbers (Y coordinates) increase
/// downwards and not upwards.
fn parse_coordinates(coord_str: &str) -> (usize, usize) {
    let mut letters_byte_count = 0;
    for c in coord_str.chars() {
        if c >= 'A' && c <= 'Z' {
            letters_byte_count += c.len_utf8();
        } else {
            break;
        }
    }

    let letters_slice = &coord_str[..letters_byte_count];
    let digits_slice = &coord_str[letters_byte_count..];

    // try parsing digits first (fail quickly)
    let mut row: usize = digits_slice.parse()
        .expect("invalid row in cell name");
    row -= 1; // we are 0-based

    if letters_slice.len() == 0 {
        panic!("no letters at beginning");
    }
    let mut column = 0;
    // columns are equivalent to base-26 numbers represented by A to Z
    for b in letters_slice.bytes() {
        assert!(b >= b'A' && b <= b'Z');
        column *= 26;
        column += usize::from(b - b'A');
    }

    (column, row)
}

fn stringify_coordinates(x: usize, y: usize) -> String {
    let mut column = x;

    let mut text_coord = if column == 0 {
        "A".to_owned()
    } else {
        let mut column_name = String::new();
        while column > 0 {
            let column_letter_index = column % 26;
            column /= 26;

            let column_char = char::from_u32(('A' as u32) + u32::try_from(column_letter_index).unwrap()).unwrap();
            column_name.insert(0, column_char);
        }
        column_name
    };
    write!(text_coord, "{}", y + 1).unwrap();
    text_coord
}

fn process_current_group_vehicles(
    vehicles: &mut Vec<serde_json::Value>,
    this_group_vehicles: &mut Vec<serde_json::Value>,
) {
    // collect vehicle numbers in this group
    let mut fixed_coupling_numbers: Vec<String> = this_group_vehicles
        .iter()
        .map(|val| val["number"].as_str().unwrap().to_owned())
        .collect();

    // reverse them
    fixed_coupling_numbers.reverse();

    // store them as the fixed coupling
    for this_group_vehicle in this_group_vehicles.iter_mut() {
        let fixed_coupling_array = this_group_vehicle["fixed_coupling"].as_array_mut().unwrap();
        for number in &fixed_coupling_numbers {
            fixed_coupling_array.push(serde_json::Value::String(number.clone()));
        }
    }

    // append the group vehicles to our full list
    vehicles.append(this_group_vehicles);
}

fn process_sheet<F: Read + Seek>(
    config: &Config,
    zip_archive: &mut ZipArchive<F>,
    strings: &[String],
    style_to_fill: &HashMap<usize, RgbaColor>,
    sheet_name: &str,
    sheet_path: &str,
    grouped_vehicles: bool,
    vehicles: &mut Vec<serde_json::Value>,
) {
    eprintln!("sheet {} ({:?})", sheet_path, sheet_name);
    let sheet_package = parse_xml_from_zip(zip_archive, &format!("xl/{}", sheet_path));
    let doc_elem = get_doc_elem(&sheet_package)
        .expect("worksheet doc is missing document element");

    let mut merged_to_first_cell: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let cell_merge_elements: Vec<Element> = doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "mergeCells"))
        .into_iter()
        .flat_map(|c| c.children())
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "mergeCell"))
        .collect();
    for cell_merge_element in cell_merge_elements {
        let ref_str = match cell_merge_element.attribute_value("ref") {
            Some(rs) => rs,
            None => {
                eprintln!("mergeCell without ref attribute: {:?}", cell_merge_element);
                continue;
            },
        };
        let (range_start_str, range_end_str) = match ref_str.split_once(':') {
            Some(rsre) => rsre,
            None => {
                eprintln!("mergeCell with unsplittable ref {:?}", ref_str);
                continue;
            },
        };
        let range_start = parse_coordinates(range_start_str);
        let range_end = parse_coordinates(range_end_str);
        let first_cell = (range_start.0, range_start.1);
        for x in range_start.0..=range_end.0 {
            for y in range_start.1..=range_end.1 {
                let this_cell = (x, y);
                if this_cell == first_cell {
                    continue;
                }

                merged_to_first_cell.insert(this_cell, first_cell);
            }
        }
    }

    let mut cells: BTreeMap<(usize, usize), CellContents> = BTreeMap::new();
    let rows: Vec<Element> = doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "sheetData"))
        .into_iter()
        .flat_map(|c| c.children())
        .filter_map(|c| c.element())
        .filter(|e| name_matches(e.name(), SSML_NSURL, "row"))
        .collect();
    for row in rows {
        let columns: Vec<Element> = row
            .children()
            .iter()
            .filter_map(|c| c.element())
            .filter(|e| name_matches(e.name(), SSML_NSURL, "c"))
            .collect();

        for column in columns {
            let coord_str = column
                .attribute_value("r").expect("missing attribute 'r'");
            let style_id: usize = column
                .attribute_value("s").expect("missing attribute 's'")
                .parse().expect("failed to parse style from 's'");
            let background_color = *style_to_fill.get(&style_id)
                .expect("failed to find style");
            let type_str = column
                .attribute_value("t").unwrap_or("");

            let coord = parse_coordinates(coord_str);
            if let Some(first_cell_coord) = merged_to_first_cell.get(&coord) {
                // take value from first merged cell instead
                if let Some(val) = cells.get(first_cell_coord) {
                    cells.insert(coord, val.clone());
                    continue;
                }
            }

            let value_element_opt = column
                .children()
                .iter()
                .filter_map(|c| c.element())
                .filter(|e| name_matches(e.name(), SSML_NSURL, "v"))
                .nth(0);
            let text = match value_element_opt {
                Some(ve) => {
                    let value_index_text = ve
                        .children()
                        .iter()
                        .filter_map(|c| c.text())
                        .map(|t| t.text())
                        .nth(0)
                        .expect("'v' element does not have a text child");
                    if type_str == "s" {
                        // string from string list
                        let value_index: usize = value_index_text.parse()
                            .expect("failed to parse value index text");
                        strings[value_index].clone()
                    } else {
                        // immediate value
                        String::from(value_index_text)
                    }
                },
                None => String::new()
            };

            cells.insert(
                coord,
                CellContents {
                    text,
                    background_color,
                },
            );
        }
    }

    // find the extents of the table
    // (minimum and maximum X and Y coordinates that are not the background color)
    let mut min_x = usize::MAX;
    let mut max_x = 0;
    let mut min_y = usize::MAX;
    let mut max_y = 0;
    for (&(x, y), cell) in &cells {
        if !config.background_colors.contains(&cell.background_color) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }

    eprintln!("extents: x={}..={} y={}..={}", min_x, max_x, min_y, max_y);

    // filter out all the data beyond the table's extents
    cells.retain(|&(x, y), _cell| x >= min_x && x <= max_x && y >= min_y && y <= max_y);

    // convert to row -> column -> cell
    let mut row_col_cell: BTreeMap<usize, BTreeMap<usize, &CellContents>> = BTreeMap::new();
    for (&(x, y), cell) in &cells {
        row_col_cell
            .entry(y)
            .or_insert_with(|| BTreeMap::new())
            .insert(x, cell);
    }

    // process the cells
    let mut header_names = BTreeMap::new();
    let mut have_vehicles = false;
    let mut this_group_vehicles = Vec::new();
    'row_loop: for (&y, x_cells) in &row_col_cell {
        let mut header_row = false;
        let mut out_of_order = false;
        let mut type_code_opt = None;
        let mut vehicle_code_opt = None;
        let mut other_fields = BTreeMap::new();

        for (&x, cell) in x_cells {
            eprintln!("{} ({}, {}) bg={} text={}", stringify_coordinates(x, y), x, y, cell.background_color, cell.text);
            if x == 0 {
                if config.background_colors.contains(&cell.background_color) {
                    // rows that start with the background color are not interesting
                    // (at best, they contain column headers of column headers)

                    if have_vehicles {
                        // okay, we have already collected vehicles and we have a background color again
                        // enough!
                        break 'row_loop;
                    } else {
                        continue 'row_loop;
                    }
                } else if config.header_colors.contains(&cell.background_color) && header_names.len() == 0 {
                    // this must be the header row
                    header_row = true;
                }
            }

            if header_row {
                header_names.insert(x, &cell.text);
                continue;
            } else if header_names.len() == 0 {
                // don't bother with the row until we have a header
                continue 'row_loop;
            }

            // obtain the name of this column
            let column_name = header_names.get(&x)
                .map(|hn| hn.as_str())
                .unwrap_or("");

            if config.state_color_column.matches_cell(x, column_name) {
                if cell.background_color == config.out_of_service_color {
                    out_of_order = true;
                } else if cell.background_color == config.rebuilt_color {
                    // skip this row as the vehicle probably appears elsewhere
                    continue 'row_loop;
                }
            } else if !grouped_vehicles && config.code_column.matches_cell(x, column_name) {
                vehicle_code_opt = Some(cell.text.clone());
            } else if !grouped_vehicles && config.type_column.matches_cell(x, column_name) {
                type_code_opt = Some(cell.text.clone());
            } else if grouped_vehicles && config.grouped_code_column.matches_cell(x, column_name) {
                vehicle_code_opt = Some(cell.text.clone());
            } else if grouped_vehicles && config.grouped_type_column.matches_cell(x, column_name) {
                type_code_opt = Some(cell.text.clone());
            } else if config.ignore_column_names.iter().any(|icn| icn.is_match(column_name)) {
                // skip this column
                continue;
            } else {
                // additional data
                other_fields.insert(column_name.to_owned(), cell.text.clone());
            }
        }

        let vehicle_code = match vehicle_code_opt {
            Some(val) => val,
            None => continue,
        };
        let type_code = match type_code_opt {
            Some(val) => val,
            None => continue,
        };

        if vehicle_code.len() == 0 && type_code.len() == 0 {
            if grouped_vehicles && this_group_vehicles.len() > 0 {
                // take care of the current group
                process_current_group_vehicles(
                    vehicles,
                    &mut this_group_vehicles,
                );
            }
            continue;
        }

        for conversion in &config.code_conversions {
            if !conversion.code_extractor_regex.is_match(&vehicle_code) {
                continue;
            }

            if conversion.code_replacements.len() > 0 {
                let mut generated_names = Vec::with_capacity(conversion.code_replacements.len());
                for replacement in &conversion.code_replacements {
                    generated_names.push(conversion.code_extractor_regex.replace_all(&vehicle_code, replacement));
                }
                let fixed_coupling = if conversion.code_replacements.len() > 1 {
                    generated_names.clone()
                } else {
                    Vec::new()
                };

                let mut my_other_fields = other_fields.clone();
                if let Some(full_code_key) = &config.full_code_additional_field {
                    my_other_fields.insert(full_code_key.clone(), vehicle_code.clone());
                }
                if let Some(type_additional_key) = &config.original_type_additional_field {
                    my_other_fields.insert(type_additional_key.clone(), type_code.clone());
                }
                if let Some(worksheet_name_key) = &config.worksheet_name_additional_field {
                    my_other_fields.insert(worksheet_name_key.clone(), sheet_name.to_owned());
                }
                for (k, v) in &conversion.common_props {
                    my_other_fields
                        .entry(k.clone())
                        .or_insert_with(|| v.clone());
                }

                let my_type_code = if let Some(overridden_type) = &conversion.overridden_type {
                    overridden_type
                } else {
                    &type_code
                };

                for generated_name in &generated_names {
                    let vehicle = serde_json::json!({
                        "number": generated_name,
                        "vehicle_class": conversion.vehicle_class,
                        "type_code": my_type_code,
                        "in_service_since": "?",
                        "out_of_service_since": if out_of_order { Some("?") } else { None },
                        "manufacturer": serde_json::Value::Null,
                        "other_data": my_other_fields,
                        "fixed_coupling": fixed_coupling,
                    });
                    if grouped_vehicles {
                        this_group_vehicles.push(vehicle);
                    } else {
                        vehicles.push(vehicle);
                    }
                }
                have_vehicles = true;
            }

            // if we have no code replacements, this is a throwaway type but still valid; keep going
            continue 'row_loop;
        }
        panic!("unmatched vehicle code {:?}", vehicle_code);
    }

    // take care of the current group if anything remains
    if this_group_vehicles.len() > 0 {
        // take care of the current group
        process_current_group_vehicles(
            vehicles,
            &mut this_group_vehicles,
        );
    }
}

fn process_sheets<F: Read + Seek>(config: &Config, zip_archive: &mut ZipArchive<F>, vehicles: &mut Vec<serde_json::Value>) {
    // obtain strings and fills
    let strings = obtain_strings(zip_archive);
    let style_to_fill = obtain_style_to_fill(zip_archive);

    // obtain base doc
    let base_package = parse_xml_from_zip(zip_archive, BASE_DOC_PATH);

    let doc_elem = get_doc_elem(&base_package)
        .expect("base doc is missing document element");
    if !name_matches(doc_elem.name(), SSML_NSURL, "workbook") {
        panic!("base doc {:?} has wrong top-level element: {:?}", BASE_DOC_PATH, doc_elem.name());
    }

    let sheets: Vec<Element> = doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|c| name_matches(c.name(), SSML_NSURL, "sheets"))
        .flat_map(|c| c.children())
        .filter_map(|c| c.element())
        .filter(|c| name_matches(c.name(), SSML_NSURL, "sheet"))
        .collect();
    let mut sheet_names_and_rels = Vec::with_capacity(sheets.len());
    let rel_id_name = QName::with_namespace_uri(Some(RELS_NSURL), "id");
    for sheet in sheets {
        let sheet_name = match sheet.attribute_value("name") {
            Some(sn) => sn,
            None => continue,
        };
        let ignore_this_sheet = config.ignore_sheet_names
            .iter()
            .any(|re| re.is_match(sheet_name));
        if ignore_this_sheet {
            continue;
        }
        let rel_id = match sheet.attribute_value(rel_id_name) {
            Some(ri) => ri,
            None => continue,
        };
        sheet_names_and_rels.push((sheet_name, rel_id));
    }

    // parse relationships document
    let base_rel_package = parse_xml_from_zip(zip_archive, BASE_DOC_REL_PATH);
    let base_rel_doc_elem = get_doc_elem(&base_rel_package)
        .expect("base relationship doc is missing document element");
    if !name_matches(base_rel_doc_elem.name(), RELS_PACK_NSURL, "Relationships") {
        panic!("base relationship doc {:?} has wrong top-level element: {:?}", BASE_DOC_REL_PATH, base_rel_doc_elem.name());
    }
    let mut rel_target_to_zip_path = HashMap::new();
    let rel_elems: Vec<Element> = base_rel_doc_elem
        .children()
        .iter()
        .filter_map(|c| c.element())
        .filter(|c| name_matches(c.name(), RELS_PACK_NSURL, "Relationship"))
        .collect();
    for rel_elem in rel_elems {
        let id = match rel_elem.attribute_value("Id") {
            Some(i) => i,
            None => continue,
        };
        let target = match rel_elem.attribute_value("Target") {
            Some(t) => t,
            None => continue,
        };
        rel_target_to_zip_path.insert(id, target);
    }

    for (sheet_name, rel) in &sheet_names_and_rels {
        let zip_path = rel_target_to_zip_path.get(rel)
            .expect("failed to open worksheet path");
        let has_grouped_vehicles = config.grouped_train_sheet_names.iter().any(|gtsn| gtsn.is_match(sheet_name));
        process_sheet(config, zip_archive, &strings, &style_to_fill, sheet_name, zip_path, has_grouped_vehicles, vehicles);
    }
}


pub(crate) fn extract_xlsx_vehicles(config: &Config, xlsx_bytes: &[u8]) -> Vec<serde_json::Value> {
    let cursor = Cursor::new(xlsx_bytes);
    let mut zip_archive = ZipArchive::new(cursor)
        .expect("failed to open ZIP archive");
    let mut vehicles = Vec::new();
    process_sheets(config, &mut zip_archive, &mut vehicles);
    vehicles
}
