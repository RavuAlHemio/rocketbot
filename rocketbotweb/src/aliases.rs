use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;

use askama::Template;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use regex::Regex;
use rocketbot_string::regex::EnjoyableRegex;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::{get_bot_config, get_query_pairs, render_response, return_405, return_500};


#[derive(Clone, Debug, Deserialize, Hash, Eq, Template, PartialEq, Serialize)]
#[template(path = "aliases.html")]
struct AliasesTemplate {
    pub aliases: Vec<AliasPart>,
}
#[derive(Clone, Debug, Deserialize, Hash, Eq, PartialEq, Serialize)]
struct AliasPart {
    pub nick_changed: bool,
    pub nick: String,
    pub alias: String,
}


pub(crate) async fn handle_plaintext_aliases_for_nick(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let nick_opt = query_pairs.get("nick");
    let nick = match nick_opt {
        Some(n) => n,
        None => {
            return Response::builder()
                .status(400)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(Full::new(Bytes::from("GET parameter \"nick\" required.")))
                .or_else(|e| {
                    error!("failed to assemble plaintext response: {}", e);
                    return return_500();
                });
        },
    };

    // read bot config
    let bot_config = match get_bot_config().await {
        Some(bc) => bc,
        None => return return_500(),
    };

    let mut regexes_and_bases: Vec<(EnjoyableRegex, String)> = Vec::new();
    let mut base_to_regex_strings: HashMap<String, BTreeSet<String>> = HashMap::new();
    if let Some(plugins) = bot_config["plugins"].as_array() {
        for plugin in plugins {
            if plugin["name"] == "config_user_alias" && plugin["enabled"].as_bool().unwrap_or(false) {
                if let Some(artu) = plugin["config"]["alias_regex_to_username"].as_object() {
                    for (alias_regex_str, base_nick_val) in artu {
                        if let Some(base_nick) = base_nick_val.as_str() {
                            if let Ok(re) = Regex::new(&alias_regex_str) {
                                let alias_regex = EnjoyableRegex::from(re);
                                regexes_and_bases.push((alias_regex, base_nick.to_owned()));
                                base_to_regex_strings
                                    .entry(base_nick.to_owned())
                                    .or_insert_with(|| BTreeSet::new())
                                    .insert(alias_regex_str.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut base_opt = None;
    for (regex, base) in &regexes_and_bases {
        if regex.is_match(nick) {
            base_opt = Some(base.clone());
            break;
        }
    }
    let body = match base_opt {
        Some(b) => {
            let empty_set = BTreeSet::new();
            let regex_strings = base_to_regex_strings
                .get(&b)
                .unwrap_or(&empty_set);
            let mut lines = Vec::with_capacity(regex_strings.len() + 1);
            lines.push(b);
            for regex_string in regex_strings {
                lines.push(regex_string.clone());
            }
            lines.join("\n")
        },
        None => {
            // this nick is not known
            String::new()
        },
    };

    Response::builder()
        .status(200)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(body)))
        .or_else(|e| {
            error!("failed to assemble plaintext response: {}", e);
            return return_500();
        })
}

pub(crate) async fn handle_nicks_aliases(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    // read bot config
    let bot_config = match get_bot_config().await {
        Some(bc) => bc,
        None => return return_500(),
    };

    let mut alias_list = Vec::new();
    if let Some(plugins) = bot_config["plugins"].as_array() {
        for plugin in plugins {
            if plugin["name"] == "config_user_alias" && plugin["enabled"].as_bool().unwrap_or(false) {
                if let Some(latu) = plugin["config"]["lowercase_alias_to_username"].as_object() {
                    for (alias, base_nick_val) in latu {
                        if let Some(base_nick) = base_nick_val.as_str() {
                            alias_list.push((base_nick.to_owned(), alias.clone()));
                        }
                    }
                }
            }
        }
    }
    alias_list.sort_unstable();

    let mut aliases = Vec::new();

    {
        let mut last_nick = None;
        for (base_nick, alias) in alias_list.drain(..) {
            let nick_changed = if let Some(ln) = &last_nick {
                &base_nick != ln
            } else {
                true
            };
            last_nick = Some(base_nick.clone());
            let alias_template = AliasPart {
                nick_changed,
                nick: base_nick,
                alias,
            };
            aliases.push(alias_template);
        }
    }

    let template = AliasesTemplate {
        aliases,
    };

    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
