use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;
use regex::Regex;
use reqwest::{Client, Method};
use rocketbot_plugin_elementary::{Element, ElementData};
use serde_json;


const WIKIDATA_SPARQL_URL: &str = "https://query.wikidata.org/sparql?format=json";
const QUERY_FORMAT: &str = "
SELECT
    ?atomic_number
    ?symbol
    {NAME_FIELDS}
WHERE {
    ?element wdt:P31 wd:Q11344. # is-an element
    ?element wdt:P1086 ?atomic_number.
    ?element wdt:P246 ?symbol.
    OPTIONAL {
        {LABEL_STATEMENTS}
    }
    FILTER NOT EXISTS {
        ?element wdt:P31 wd:Q1299291. # is-a hypothetical element
    }
    FILTER NOT EXISTS {
        ?element wdt:P31 wd:Q21286738. # is-a permanent duplicate item
    }
}
ORDER BY ?atomic_number
";
const LABEL_STATEMENT_FORMAT: &str = "?element rdfs:label ?name_{LANG} FILTER (LANG(?name_{LANG}) = \"{LANG}\").";


#[derive(Parser)]
struct Opts {
    pub output_file: PathBuf,
    pub languages: Vec<String>,
}


#[tokio::main]
async fn main() {
    let opts = Opts::parse();
    let lang_regex = Regex::new("^[a-z]+$").expect("failed to compile language regex");
    for lang in &opts.languages {
        if !lang_regex.is_match(lang) {
            panic!("language code {:?} is invalid; must be a sequence of at least one lowercase letter a-z", lang);
        }
    }

    let name_fields: Vec<String> = opts.languages.iter()
        .map(|lang| format!("?name_{}", lang))
        .collect();
    let label_statements: Vec<String> = opts.languages.iter()
        .map(|lang| LABEL_STATEMENT_FORMAT.replace("{LANG}", lang))
        .collect();

    let query = QUERY_FORMAT
        .replace("{NAME_FIELDS}", &name_fields.join(" "))
        .replace("{LABEL_STATEMENTS}", &label_statements.join(" "));

    let client = Client::builder()
        .build()
        .expect("failed to build reqwest client");
    let mut body_fields = BTreeMap::new();
    body_fields.insert("query", query);
    let response = client
        .request(Method::POST, WIKIDATA_SPARQL_URL)
        .header("User-Agent", "obtain_elements/0.0 (github.com/RavuAlHemio/rocketbot, rocketbot_plugin_elementary example)")
        .form(&body_fields)
        .send().await.expect("failed to run SPARQL query");
    let response_code = response.status();
    let response_data = response
        .bytes().await.expect("failed to obtain SPARQL results")
        .to_vec();
    let response_string = String::from_utf8(response_data)
        .expect("SPARQL results are not UTF-8");
    if response_code != 200 {
        panic!("Wikidata SPARQL query response is {}: {}", response_code, response_string);
    }

    let response_json: serde_json::Value = serde_json::from_str(&response_string)
        .expect("failed to parse SPARQL response as JSON");
    let bindings = response_json["results"]["bindings"].as_array()
        .expect("SPARQL response $.results.bindings is not an array");
    let mut elements = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let atomic_number_str = match binding["atomic_number"]["value"].as_str() {
            Some(ans) => ans,
            None => {
                eprintln!("element has no atomic number; skipping: {:?}", binding);
                continue;
            }
        };
        let atomic_number = match atomic_number_str.parse() {
            Ok(an) => an,
            Err(_) => {
                eprintln!("element has atomic number {:?} that does not fit into u32; skipping: {:?}", atomic_number_str, binding);
                continue;
            }
        };
        let symbol = match binding["symbol"]["value"].as_str() {
            Some(s) => s.to_owned(),
            None => {
                eprintln!("element has no symbol; skipping: {:?}", binding);
                continue;
            },
        };

        let mut language_to_name = BTreeMap::new();
        for lang in &opts.languages {
            let lang_specific_name = format!("name_{}", lang);
            let name = match binding[&lang_specific_name]["value"].as_str() {
                Some(s) => s,
                None => continue, // missing in this language
            };
            language_to_name.insert(lang.clone(), name.to_owned());
        }

        let element = Element {
            atomic_number,
            symbol,
            language_to_name,
        };
        elements.push(element);
    }

    // output as TOML
    let data = ElementData {
        elements,
    };
    let data_toml = toml::to_string(&data)
        .expect("failed to serialize elements to TOML");
    std::fs::write(&opts.output_file, &data_toml)
        .expect("failed to write elements TOML");
}
