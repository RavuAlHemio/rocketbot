use std::sync::Arc;

use async_trait::async_trait;
use bytes::Buf;
use rand::{RngCore, Rng};
use regex::Regex;
use rocketbot_interface::sync::Mutex;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_encode};
use serde::{Deserialize, Serialize};
use serde_json;
use url::Url;

use crate::interface::{FactError, FactProvider};


pub const MEDIAWIKI_URL_SAFE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');


#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    pub prefix_text: Option<String>,
    #[serde(with = "serde_url")]
    pub source_uri: Url,
    #[serde(with = "serde_url")]
    pub wikitext_link_base_uri: Url,
    #[serde(with = "serde_regex")]
    pub start_strip_regex: Regex,
    #[serde(with = "serde_regex")]
    pub end_strip_regex: Regex,
}

mod serde_url {
    use serde::{Deserialize, Deserializer, Serializer};
    use url::Url;

    pub fn serialize<S: Serializer>(url: &Url, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(url.as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Url, D::Error> {
        let string = String::deserialize(deserializer)?;
        Url::parse(&string)
            .map_err(|e| serde::de::Error::custom(e))
    }
}
mod serde_regex {
    use regex::Regex;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(re: &Regex, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(re.as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Regex, D::Error> {
        let ser = String::deserialize(deserializer)?;
        Regex::new(&ser)
            .map_err(|e| serde::de::Error::custom(e))
    }
}

fn transform_wikitext_links(config: &Config, wikitext: &str) -> String {
    let mut markdown = String::new();
    let mut cur_index = 0usize;
    loop {
        let opening_bracket_index = match wikitext[cur_index..].find("[[") {
            Some(i) => cur_index + i,
            None => break,
        };
        if opening_bracket_index + 2 >= wikitext.len() {
            break;
        }
        let closing_bracket_index = match wikitext[opening_bracket_index+2..].find("]]") {
            Some(i) => opening_bracket_index + 2 + i,
            None => break,
        };

        let pipe_index = wikitext[opening_bracket_index..].find("|")
            .map(|pi| opening_bracket_index + pi)
            // ignore pipe if it is not within the brackets
            .filter(|pi| *pi < closing_bracket_index);

        let (uri_text, body_text) = match pipe_index {
            None => {
                // both are the same
                let common_text = &wikitext[opening_bracket_index+2..closing_bracket_index];
                (common_text, common_text)
            },
            Some(pi) => {
                // split by pipe
                let uri_text = &wikitext[opening_bracket_index+2..pi];
                let body_text = &wikitext[pi+1..closing_bracket_index];
                if body_text.len() == 0 {
                    // URI text with prefixes stripped
                    let last_colon_index = uri_text.rfind(":");
                    if let Some(lci) = last_colon_index {
                        (uri_text, &uri_text[lci+1..])
                    } else {
                        // no colons; they are the same
                        (uri_text, uri_text)
                    }
                } else {
                    (uri_text, body_text)
                }
            },
        };

        let mut uri_builder = uri_text.to_owned();

        // remove leading colons (:File:Test.jpeg => File:Test.jpeg)
        while uri_builder.starts_with(":") {
            uri_builder.remove(0);
        }

        // replace spaces with underscores
        uri_builder = uri_builder.replace(" ", "_");

        // capitalize first letter
        let mut cap_builder = String::with_capacity(uri_builder.len());
        for (i, c) in uri_builder.chars().enumerate() {
            if i == 0 {
                for cc in c.to_uppercase() {
                    cap_builder.push(cc);
                }
            } else {
                cap_builder.push(c);
            }
        }
        uri_builder = cap_builder;

        // URL-encode
        let url_encoded = percent_encode(uri_builder.as_bytes(), MEDIAWIKI_URL_SAFE)
            .to_string();

        let link_uri = match config.wikitext_link_base_uri.join(&url_encoded) {
            Ok(lu) => lu,
            Err(e) => {
                panic!("failed to join {} and {:?}: {}", config.wikitext_link_base_uri, url_encoded, e)
            },
        };

        // append the (link-free) text until here
        markdown.push_str(&wikitext[cur_index..opening_bracket_index]);

        // append the link in markdown format
        markdown.push_str(&format!("[{}]({})", body_text, link_uri));

        // go forth
        cur_index = closing_bracket_index + 2;
    }

    // and the rest
    markdown.push_str(&wikitext[cur_index..]);

    markdown
}

pub(crate) struct UncyclopediaProvider {
    facts: Vec<String>,
}
impl UncyclopediaProvider {
}
#[async_trait]
impl FactProvider for UncyclopediaProvider {
    async fn new(config_json: serde_json::Value) -> Self where Self: Sized {
        let config: Config = match serde_json::from_value(config_json) {
            Ok(c) => c,
            Err(e) => {
                panic!("failed to parse config: {}", e);
            },
        };

        // obtain data from the URI and parse it as JSON
        let data = match reqwest::get(config.source_uri.clone()).await {
            Ok(r) => r,
            Err(e) => {
                panic!("failed to fetch facts: {}", e);
            },
        };
        let data_bytes = match data.bytes().await {
            Ok(b) => b,
            Err(e) => {
                panic!("failed to fetch response bytes: {}", e);
            },
        };
        let page_data: serde_json::Value = match serde_json::from_reader(data_bytes.reader()) {
            Ok(v) => v,
            Err(e) => {
                panic!("failed to parse response: {}", e);
            },
        };
        let page_wikitext = match page_data["query"]["pages"][0]["revisions"][0]["slots"]["main"]["content"].as_str() {
            Some(v) => v,
            None => {
                panic!("failed to obtain wikitext from page");
            },
        };

        let mut facts = Vec::new();
        let mut cur_index = 0usize;
        loop {
            let start_match = match config.start_strip_regex.find_at(page_wikitext, cur_index) {
                Some(m) => m,
                None => break,
            };
            let end_match = match config.end_strip_regex.find_at(page_wikitext, start_match.range().end) {
                Some(m) => m,
                None => break,
            };

            let fact_wikitext = &page_wikitext[start_match.range().end..end_match.range().start];
            let fact_linked = transform_wikitext_links(&config, fact_wikitext);

            let mut fact_bits = String::new();
            if let Some(pt) = &config.prefix_text {
                fact_bits.push_str(pt);
            }
            fact_bits.push_str(&fact_linked);
            facts.push(fact_bits);

            cur_index = end_match.range().end;
        }

        Self {
            facts,
        }
    }

    async fn get_random_fact(&self, rng: Arc<Mutex<Box<dyn RngCore + Send>>>) -> Result<String, FactError> {
        if self.facts.len() == 0 {
            return Err(FactError::new("no facts available".to_owned()));
        }

        let rand_index = {
            let mut rng_guard = rng.lock().await;
            rng_guard.gen_range(0..self.facts.len())
        };

        Ok(self.facts[rand_index].clone())
    }
}
