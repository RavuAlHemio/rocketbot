//! Obtains service operator databases from tabular MediaWiki data, e.g. from Wikipedia.


use std::collections::BTreeMap;
use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;

use reqwest::Client;
use rocketbot_bim_common::LineOperatorInfo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error as _;
use sxd_document;
use sxd_document::dom::Element;


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub output_path: String,
        pub page_sources: Vec<PageSource>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct PageSource {
    pub page_url_pattern: String,
    pub authorization_token: Option<String>,
    pub pages: Vec<PageConfig>,
    pub operator_name_to_abbrev: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct PageConfig {
    pub title: String,
    pub region: String,
    pub line_column: String,
    pub operator_spec: Spec,
    pub section: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum Spec {
    Column(String),
    Fixed(String),
}
impl<'de> Deserialize<'de> for Spec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut string: String = Deserialize::deserialize(deserializer)?;
        if string.starts_with("$") {
            string.remove(0);
            Ok(Self::Column(string))
        } else if string.starts_with("=") {
            string.remove(0);
            Ok(Self::Fixed(string))
        } else {
            Err(D::Error::custom("spec string must start with $ (column) or = (fixed string)"))
        }
    }
}
impl Serialize for Spec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let stringified = match self {
            Self::Column(c) => format!("${}", c),
            Self::Fixed(f) => format!("={}", f),
        };
        stringified.serialize(serializer)
    }
}


trait DocumentExt {
    fn document_element(&self) -> Option<Element>;
}
impl DocumentExt for sxd_document::dom::Document<'_> {
    fn document_element(&self) -> Option<Element> {
        self
            .root().children().into_iter()
            .filter_map(|c| c.element())
            .nth(0)
    }
}


trait ElementExt<'d> {
    fn child_elements_named<'s, 'n>(&'s self, local_name: &'n str) -> Vec<Element<'d>>;
    fn descendant_elements_named<'s, 'n>(&'s self, local_name: &'n str) -> Vec<Element<'d>>;
    fn first_child_element_named<'s, 'n>(&'s self, local_name: &'n str) -> Option<Element<'d>>;
    fn first_text_rec<'s>(&'s self) -> Option<String>;
}
impl<'d> ElementExt<'d> for sxd_document::dom::Element<'d> {
    fn child_elements_named<'s, 'n>(&'s self, local_name: &'n str) -> Vec<Element<'d>> {
        self
            .children().into_iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name().local_part() == local_name)
            .collect()
    }
    fn first_child_element_named<'s, 'n>(&'s self, local_name: &'n str) -> Option<Element<'d>> {
        self
            .children().into_iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name().local_part() == local_name)
            .nth(0)
    }
    fn first_text_rec<'s>(&'s self) -> Option<String> {
        use sxd_document::dom::ChildOfElement;
        for child in self.children() {
            match child {
                ChildOfElement::Element(e) => {
                    if let Some(t) = e.first_text_rec() {
                        return Some(t);
                    }

                    // keep going otherwise
                },
                ChildOfElement::Text(t) => return Some(t.text().to_owned()),
                _ => {},
            }
        }

        None
    }

    fn descendant_elements_named<'s, 'n>(&'s self, local_name: &'n str) -> Vec<Element<'d>> {
        let mut element_stack = vec![(true, self.clone())];
        let mut ret = Vec::new();

        while let Some((is_root, element)) = element_stack.pop() {
            // "descendant" means "not myself" so check for root
            if !is_root && element.name().local_part() == local_name {
                ret.push(element.clone());
            }

            let child_elements = element
                .children()
                .into_iter()
                .rev()
                .filter_map(|c| c.element());
            for child_elem in child_elements {
                element_stack.push((false, child_elem));
            }
        }

        ret
    }
}


async fn obtain_xhtml(
    reqwest_client: &mut Client,
    url: &str,
    authorization_token: Option<&str>,
) -> String {
    eprintln!("fetching URL {:?}", url);
    let mut builder = reqwest_client.get(url);
    if let Some(token) = authorization_token {
        builder = builder.bearer_auth(token);
    }
    let page_html_bytes = builder
        .send().await.expect("sending request failed")
        .error_for_status().expect("response is an error")
        .bytes().await.expect("obtaining response bytes failed");
    let page_html_string = String::from_utf8(page_html_bytes.to_vec())
        .expect("article is not UTF-8");
    page_html_string
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Table {
    rows_then_cols: Vec<Vec<Option<String>>>,
    current_row: Option<usize>,
}
impl Table {
    pub const fn new() -> Self {
        Self {
            rows_then_cols: Vec::new(),
            current_row: None,
        }
    }

    pub fn into_rows_some(self) -> Vec<Vec<String>> {
        let mut ret = Vec::with_capacity(self.rows_then_cols.len());
        for row in self.rows_then_cols {
            let mut new_row = Vec::with_capacity(row.len());
            for value in row {
                if let Some(v) = value {
                    new_row.push(v);
                } else {
                    new_row.push(String::with_capacity(0))
                }
            }
            ret.push(new_row);
        }
        ret
    }

    fn make_rectangular(&mut self, up_to_row_index: Option<usize>, up_to_col_index: Option<usize>) {
        let want_rows = if let Some(utri) = up_to_row_index {
            self.row_count().max(utri + 1)
        } else {
            self.row_count()
        };
        let want_cols = if let Some(utci) = up_to_col_index {
            self.col_count().max(utci + 1)
        } else {
            self.col_count()
        };

        while self.rows_then_cols.len() < want_rows {
            self.rows_then_cols.push(Vec::with_capacity(want_cols));
        }
        for row in &mut self.rows_then_cols {
            while row.len() < want_cols {
                row.push(None);
            }
        }
    }

    pub fn set_next_cell(&mut self, row_span: usize, col_span: usize, value: String) {
        // if we have a completely empty table, start the first row
        if self.current_row.is_none() {
            self.current_row = Some(0);
        }
        let current_row = self.current_row.unwrap();
        while current_row >= self.rows_then_cols.len() {
            self.rows_then_cols.push(Vec::new());
        }
        self.make_rectangular(None, None);

        // find the first empty cell
        let mut empty_cell_opt = None;
        for (r, row) in self.rows_then_cols.iter().enumerate().skip(current_row) {
            for (c, cell) in row.iter().enumerate() {
                if cell.is_none() {
                    empty_cell_opt = Some((r, c));
                    break;
                }
            }
            if empty_cell_opt.is_some() {
                break;
            }
        }

        let (empty_r, empty_c) = if let Some(ec) = empty_cell_opt {
            ec
        } else {
            // table is full, we will be extending the final row
            if self.rows_then_cols.len() == 0 {
                self.rows_then_cols.push(Vec::new());
            }
            let last_row_index = self.rows_then_cols.len() - 1;
            let last_row = self.rows_then_cols.last_mut().unwrap();
            last_row.push(None);
            (last_row_index, last_row.len() - 1)
        };

        for r in empty_r..empty_r+row_span {
            for c in empty_c..empty_c+col_span {
                self.set_cell(r, c, Some(value.clone()));
            }
        }
    }

    pub fn start_new_row(&mut self) {
        if let Some(cr) = self.current_row {
            self.current_row = Some(cr + 1);
        } else {
            self.current_row = Some(0);
        }
    }

    pub fn row_count(&self) -> usize {
        let defined_rows = self.rows_then_cols.len();
        if let Some(cr) = self.current_row {
            defined_rows.max(cr)
        } else {
            defined_rows
        }
    }

    pub fn col_count(&self) -> usize {
        self.rows_then_cols
            .iter()
            .map(|r| r.len())
            .max()
            .unwrap_or(0)
    }

    pub fn set_cell(&mut self, row: usize, col: usize, value: Option<String>) {
        // ensure we are rectangular and large enough
        self.make_rectangular(Some(row), Some(col));
        self.rows_then_cols[row][col] = value;
    }

    #[allow(unused)]
    pub fn to_sample_text(&self) -> String {
        use std::fmt::Write as _;
        let mut ret = String::new();
        writeln!(ret, "CR={:?}", self.current_row).unwrap();
        for row in &self.rows_then_cols {
            ret.push_str("|");
            for cell_opt in row {
                if let Some(cell) = cell_opt {
                    write!(ret, " {:5.5} |", cell).unwrap();
                } else {
                    ret.push_str(" ----- |");
                }
            }
            ret.push('\n');
        }
        ret
    }
}


fn reduce_table(table: Element) -> Vec<Vec<String>> {
    let tbody = table.first_child_element_named("tbody")
        .expect("no tbody element");
    let rows = tbody.child_elements_named("tr");
    let mut ret = Table::new();

    for row in rows.into_iter() {
        ret.start_new_row();

        let cells = row.children()
            .into_iter()
            .filter_map(|n| n.element())
            .filter(|e| e.name().local_part() == "td" || e.name().local_part() == "th");

        for cell in cells {
            let first_text_opt = cell.first_text_rec();
            let first_text = match first_text_opt {
                Some(ft) => ft,
                None => {
                    // try the first alt attribute of an <img> we find
                    let mut img_children = cell.descendant_elements_named("img");
                    img_children.retain(|c| c.attribute_value("alt").is_some());
                    if let Some(ic) = img_children.get(0) {
                        ic.attribute_value("alt").unwrap().to_owned()
                    } else {
                        // assume empty
                        String::with_capacity(0)
                    }
                },
            };

            // rowspan? colspan?
            let mut row_span: usize = cell.attribute_value("rowspan")
                .and_then(|r| r.parse().ok())
                .unwrap_or(1);
            if row_span == 0 {
                row_span = 1;
            }
            let mut col_span: usize = cell.attribute_value("colspan")
                .and_then(|r| r.parse().ok())
                .unwrap_or(1);
            if col_span == 0 {
                col_span = 1;
            }

            ret.set_next_cell(row_span, col_span, first_text);
        }
    }

    ret.into_rows_some()
}


async fn process_page(
    reqwest_client: &mut Client,
    url_pattern: &str,
    page_config: &PageConfig,
    operator_name_to_abbrev: &BTreeMap<String, String>,
    authorization_token: Option<&str>,
) -> BTreeMap<String, BTreeMap<String, LineOperatorInfo>> {
    // return value is: region -> line -> operator_info

    let url = url_pattern.replace("{TITLE}", &page_config.title);

    let page_xml_notag = obtain_xhtml(reqwest_client, &url, authorization_token).await;
    let page_xml = format!("<?xml version=\"1.0\"?>{}", page_xml_notag);

    let page_package = sxd_document::parser::parse(&page_xml)
        .expect("parsing XML failed");
    let page = page_package.as_document();

    let html = page.document_element().expect("no document element");
    let body = html.first_child_element_named("body").expect("no body element");

    let mut sections = body.child_elements_named("section");
    if let Some(section_name) = page_config.section.as_ref() {
        sections.retain(|section| {
            // <section><h#>Something</h#></section>
            let heading_elems = section
                .children()
                .into_iter()
                .filter_map(|c| c.element())
                .filter(|e| {
                    let name = e.name().local_part();
                    name.starts_with("h")
                        && !name.chars().skip(1).any(|c| c < '0' || c > '9')
                });
            for heading_elem in heading_elems {
                // we only need to check the first match
                return if let Some(text) = heading_elem.first_text_rec() {
                    section_name == &text
                } else {
                    section_name == ""
                };
            }
            // no heading? not what we're looking for
            false
        });
    }

    let table = sections
        .into_iter()
        .flat_map(|section| section.child_elements_named("table"))
        .nth(0)
        .expect("no table element in any section");
    let reduced_table = reduce_table(table);

    let mut line_column_index_opt = None;
    let mut operator_column_index_opt = None;

    let mut ret = BTreeMap::new();

    for (r, row) in reduced_table.iter().enumerate() {
        let mut line_opt = None;
        let mut operator_opt = None;
        for (c, first_text) in row.iter().enumerate() {
            if r == 0 {
                // heading row
                if first_text == &page_config.line_column {
                    line_column_index_opt = Some(c);
                }
                if let Spec::Column(operator_column) = &page_config.operator_spec {
                    if first_text == operator_column {
                        operator_column_index_opt = Some(c);
                    }
                }
            } else {
                // data row
                let line_column_index = line_column_index_opt
                    .expect("no line column index known");

                if c == line_column_index {
                    line_opt = Some(first_text.clone());
                }
                if let Some(operator_column_index) = operator_column_index_opt {
                    if c == operator_column_index {
                        operator_opt = Some(first_text.clone());
                    }
                }
            }
        }

        if let Spec::Fixed(operator_fixed) = &page_config.operator_spec {
            operator_opt = Some(operator_fixed.clone());
        }

        let line = match line_opt {
            Some(l) => l,
            None => continue,
        };
        let operator_name = match operator_opt {
            Some(o) => o,
            None => continue,
        };
        let operator_abbrev = operator_name_to_abbrev
            .get(&operator_name)
            .map(|oa| oa.clone());
        let operator_info = LineOperatorInfo {
            canonical_line: line.clone(),
            operator_name,
            operator_abbrev,
        };

        ret
            .entry(page_config.region.clone())
            .or_insert_with(|| BTreeMap::new())
            .insert(line.to_lowercase(), operator_info);
    }

    ret
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("fetch_ops_mw.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let mut region_to_line_to_operator = BTreeMap::new();

    let mut reqwest_client = reqwest::Client::new();

    for page_source in &config.page_sources {
        for page in &page_source.pages {
            let this_region_to_line_to_operator = process_page(
                &mut reqwest_client,
                &page_source.page_url_pattern,
                &page,
                &page_source.operator_name_to_abbrev,
                page_source.authorization_token.as_deref(),
            ).await;
            for (this_region, this_line_to_operator) in this_region_to_line_to_operator {
                let line_to_operator = region_to_line_to_operator
                    .entry(this_region)
                    .or_insert_with(|| BTreeMap::new());
                for (this_line, this_operator) in this_line_to_operator {
                    line_to_operator.insert(this_line, this_operator);
                }
            }
        }
    }

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        serde_json::to_writer_pretty(f, &region_to_line_to_operator)
            .expect("failed to write operators");
    }
}
