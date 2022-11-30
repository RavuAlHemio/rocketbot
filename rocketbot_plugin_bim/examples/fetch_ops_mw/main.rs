use std::collections::BTreeMap;
use std::env::args_os;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use reqwest::Client;
use rocketbot_mediawiki_parsing::WikiParser;
use serde::{Deserialize, Serialize};
use serde_json;
use sxd_document;
use sxd_document::dom::Element;


#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    pub output_path: String,
    pub php_path: Option<String>,
    pub wiki_parse_server_dir: String,
    pub parser_already_running: bool,
    pub page_sources: Vec<PageSource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PageSource {
    pub page_url_pattern: String,
    pub pages: Vec<PageConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PageConfig {
    pub title: String,
    pub region: String,
    pub line_column: String,
    pub operator_column: String,
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


trait ElementExt {
    fn child_elements_named(&self, local_name: &str) -> Vec<Element>;
    fn first_child_element_named(&self, local_name: &str) -> Option<Element>;
    fn first_text_rec(&self) -> Option<String>;
}
impl ElementExt for sxd_document::dom::Element<'_> {
    fn child_elements_named(&self, local_name: &str) -> Vec<Element> {
        self
            .children().into_iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name().local_part() == local_name)
            .collect()
    }
    fn first_child_element_named(&self, local_name: &str) -> Option<Element> {
        self
            .children().into_iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name().local_part() == local_name)
            .nth(0)
    }
    fn first_text_rec(&self) -> Option<String> {
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
}


async fn process_page(
    reqwest_client: &mut Client,
    url_pattern: &str,
    page_config: &PageConfig,
    wiki_parser: &mut WikiParser,
) -> BTreeMap<String, BTreeMap<String, String>> {
    // return value is region -> line -> operator

    let url = url_pattern.replace("{TITLE}", &page_config.title);

    // obtain wikitext from URL
    let pages_json_bytes = reqwest_client.get(url)
        .send().await.expect("sending request failed")
        .error_for_status().expect("response is an error")
        .bytes().await.expect("obtaining response bytes failed");
    let pages_json: serde_json::Value = serde_json::from_reader(pages_json_bytes.as_ref())
        .expect("failed to parse JSON");
    let page_json: &serde_json::Value = &pages_json["query"]["pages"][0];
    let page_title = page_json["title"]
        .as_str().expect("page title in JSON is not a string");
    let page_source = page_json["revisions"][0]["slots"]["main"]["content"]
        .as_str().expect("page content in JSON is not a string");

    let page_xml_notag = wiki_parser.parse_article(page_title, page_source)
        .expect("parsing wikitext failed");
    let page_xml = format!("<?xml version=\"1.0\"?>{}", page_xml_notag);

    let page_package = sxd_document::parser::parse(&page_xml)
        .expect("parsing XML failed");
    let page = page_package.as_document();

    let html = page.document_element().expect("no document element");
    let body = html.first_child_element_named("body").expect("no body element");
    let table = body.first_child_element_named("table").expect("no table element");
    let tbody = table.first_child_element_named("tbody").expect("no tbody element");
    let first_table_rows = tbody.child_elements_named("tr");

    let mut line_column_index_opt = None;
    let mut operator_column_index_opt = None;

    let mut ret = BTreeMap::new();

    for (r, row) in first_table_rows.into_iter().enumerate() {
        let cells = row.children()
            .into_iter()
            .filter_map(|n| n.element())
            .filter(|e| e.name().local_part() == "td" || e.name().local_part() == "th");

        let mut line_opt = None;
        let mut operator_opt = None;
        for (c, cell) in cells.enumerate() {
            let first_text_opt = cell.first_text_rec();
            let first_text = match first_text_opt {
                Some(ft) => ft,
                None => continue,
            };

            if r == 0 {
                // heading row
                if first_text == page_config.line_column {
                    line_column_index_opt = Some(c);
                }
                if first_text == page_config.operator_column {
                    operator_column_index_opt = Some(c);
                }
            } else {
                // data row
                let line_column_index = line_column_index_opt
                    .expect("no line column index known");
                let operator_column_index = operator_column_index_opt
                    .expect("no operator column index known");

                if c == line_column_index {
                    line_opt = Some(first_text.clone());
                }
                if c == operator_column_index {
                    operator_opt = Some(first_text.clone());
                }
            }
        }

        let line = match line_opt {
            Some(l) => l,
            None => continue,
        };
        let operator = match operator_opt {
            Some(o) => o,
            None => continue,
        };

        ret
            .entry(page_config.region.clone())
            .or_insert_with(|| BTreeMap::new())
            .insert(line, operator);
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

    let php_command = config.php_path.as_deref().unwrap_or("php");

    let mut region_to_line_to_operator = BTreeMap::new();

    {
        let mut parser = if config.parser_already_running {
            WikiParser::new_existing()
        } else {
            let parser = WikiParser::new(php_command, &config.wiki_parse_server_dir)
                .expect("error creating parser");

            // wait a bit to allow the parser to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            parser
        };

        let mut reqwest_client = reqwest::Client::new();

        for page_source in &config.page_sources {
            for page in &page_source.pages {
                let this_region_to_line_to_operator = process_page(
                    &mut reqwest_client,
                    &page_source.page_url_pattern,
                    &page,
                    &mut parser,
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

        parser.parsing_done()
            .expect("error signalling end of parsing");
    }

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        serde_json::to_writer_pretty(f, &region_to_line_to_operator)
            .expect("failed to write operators");
    }
}
