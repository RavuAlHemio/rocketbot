use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;

use askama::Template;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
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
    let nick_lowercase = match nick_opt {
        Some(n) => n.to_lowercase(),
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

    let mut lower_base_to_aliases: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut lower_alias_to_base: HashMap<String, String> = HashMap::new();

    if let Some(plugins) = bot_config["plugins"].as_array() {
        for plugin in plugins {
            if plugin["name"] == "config_user_alias" && plugin["enabled"].as_bool().unwrap_or(false) {
                if let Some(latu) = plugin["config"]["lowercase_alias_to_username"].as_object() {
                    for (alias, base_nick_val) in latu {
                        if let Some(base_nick) = base_nick_val.as_str() {
                            lower_alias_to_base.insert(alias.to_lowercase(), base_nick.to_owned());
                            lower_base_to_aliases.entry(base_nick.to_lowercase())
                                .or_insert_with(|| BTreeSet::new())
                                .insert(alias.clone());
                        }
                    }
                }
            }
        }
    }

    let base = lower_alias_to_base.get(&nick_lowercase).unwrap_or(&nick_lowercase);
    let base_lowercase = base.to_lowercase();
    let body = match lower_base_to_aliases.get(&base_lowercase) {
        Some(aliases) => {
            let mut lines = Vec::with_capacity(aliases.len() + 1);
            lines.push(base.clone());
            for alias in aliases {
                lines.push(alias.clone());
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
