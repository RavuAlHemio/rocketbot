mod config;


use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::convert::{Infallible, TryInto};
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use form_urlencoded;
use hyper::{Body, Method, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use log::error;
use once_cell::sync::OnceCell;
use serde_json;
use tera::Tera;
use tokio;
use tokio::sync::{RwLock, RwLockReadGuard};
use tokio_postgres::{self, NoTls};
use toml;

use crate::config::WebConfig;


pub(crate) static CONFIG: OnceCell<RwLock<WebConfig>> = OnceCell::new();
pub(crate) static TERA: OnceCell<RwLock<Tera>> = OnceCell::new();


fn get_query_pairs<T>(request: &Request<T>) -> HashMap<Cow<str>, Cow<str>> {
    if let Some(q) = request.uri().query() {
        form_urlencoded::parse(q.as_bytes())
            .collect()
    } else {
        HashMap::new()
    }
}


async fn render_template(name: &str, context: &tera::Context, status: u16, headers: Vec<(String, String)>) -> Option<Response<Body>> {
    let tera_lock = match TERA.get() {
        Some(t) => t,
        None => {
            error!("no Tera set");
            return None;
        },
    };
    let tera_guard = tera_lock.read().await;
    let rendered = match tera_guard.render(name, context) {
        Ok(s) => s,
        Err(e) => {
            error!("failed to render template {:?}: {:?}", name, e);
            return None;
        }
    };

    let mut builder = Response::builder()
        .status(status)
        .header("Content-Type", "text/html; charset=utf-8");
    for (k, v) in &headers {
        builder = builder.header(k, v);
    }
    match builder.body(Body::from(rendered)) {
        Ok(r) => Some(r),
        Err(e) => {
            error!("failed to assemble response: {}", e);
            None
        },
    }
}


async fn render_json(json_value: &serde_json::Value, status: u16, headers: Vec<(String, String)>) -> Option<Response<Body>> {
    let rendered = match serde_json::to_string_pretty(json_value) {
        Ok(s) => s,
        Err(e) => {
            error!("failed to render JSON: {}", e);
            return None;
        },
    };

    let mut builder = Response::builder()
        .status(status)
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        builder = builder.header(k, v);
    }
    match builder.body(Body::from(rendered)) {
        Ok(r) => Some(r),
        Err(e) => {
            error!("failed to assemble response: {}", e);
            None
        },
    }
}

async fn get_config() -> Option<RwLockReadGuard<'static, WebConfig>> {
    let config_lock = match CONFIG.get() {
        Some(c) => c,
        None => {
            error!("no config set");
            return None;
        },
    };
    Some(config_lock.read().await)
}

async fn get_bot_config() -> Option<serde_json::Value> {
    let bot_config_path = {
        let config_guard = get_config().await?;
        config_guard.bot_config_path.clone()
    };

    let bot_config_file = match File::open(bot_config_path) {
        Ok(f) => f,
        Err(e) => {
            error!("failed to open bot config file: {}", e);
            return None;
        },
    };
    match serde_json::from_reader(bot_config_file) {
        Ok(v) => Some(v),
        Err(e) => {
            error!("failed to parse bot config file: {}", e);
            return None;
        },
    }
}

async fn connect_to_db() -> Option<tokio_postgres::Client> {
    let config_guard = get_config().await?;
    let (client, connection) = match tokio_postgres::connect(&config_guard.db_conn_string, NoTls).await {
        Ok(cc) => cc,
        Err(e) => {
            error!("failed to connect to postgres: {}", e);
            return None;
        }
    };
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("connection error: {}", e);
        }
    });
    Some(client)
}


async fn return_404() -> Result<Response<Body>, Infallible> {
    let ctx = tera::Context::new();
    match render_template("404.html.tera", &ctx, 404, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn return_405() -> Result<Response<Body>, Infallible> {
    let mut ctx = tera::Context::new();
    ctx.insert("allowed_methods", &serde_json::json!["GET"]);
    let ctx = tera::Context::new();
    let headers = vec![
        ("Accept".to_owned(), "GET".to_owned()),
    ];
    match render_template("405.html.tera", &ctx, 405, headers).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

fn return_500() -> Result<Response<Body>, Infallible> {
    let response_res = Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from("500 Internal Server Error"));
    match response_res {
        Err(e) => panic!("failed to construct 500 response: {}", e),
        Ok(b) => Ok(b),
    }
}

async fn handle_index(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let ctx = tera::Context::new();
    match render_template("index.html.tera", &ctx, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn handle_top_quotes(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut quotes: Vec<serde_json::Value> = Vec::new();
    let query_res = db_conn.query("
        SELECT
            q.quote_id, q.author, q.message_type, q.body, CAST(COALESCE(SUM(CAST(v.points AS bigint)), 0) AS bigint) vote_sum
        FROM
            quotes.quotes q
            LEFT OUTER JOIN quotes.quote_votes v ON v.quote_id = q.quote_id
        GROUP BY
            q.quote_id, q.author, q.message_type, q.body
        ORDER BY
            vote_sum DESC
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query top quotes: {}", e);
            return return_500();
        },
    };
    let mut last_score = None;
    for row in rows {
        //let quote_id: i64 = row.get(0);
        let author: String = row.get(1);
        let message_type: String = row.get(2);
        let body_in_db: String = row.get(3);
        let vote_sum_opt: Option<i64> = row.get(4);

        let vote_sum = vote_sum_opt.unwrap_or(0);

        let score_changed = if last_score != Some(vote_sum) {
            last_score = Some(vote_sum);
            true
        } else {
            false
        };

        // render the quote
        let body = match message_type.as_str() {
            "F" => body_in_db,
            "M" => format!("<{}> {}", author, body_in_db),
            "A" => format!("* {} {}", author, body_in_db),
            other => format!("{}? <{}> {}", other, author, body_in_db),
        };
        quotes.push(serde_json::json!({
            "score": vote_sum,
            "score_changed": score_changed,
            "body": body,
        }));
    }

    let mut ctx = tera::Context::new();
    ctx.insert("quotes", &quotes);
    match render_template("topquotes.html.tera", &ctx, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn handle_quotes_votes(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut quotes: Vec<serde_json::Value> = Vec::new();
    let query_res = db_conn.query("
        SELECT q.quote_id, q.author, q.message_type, q.body
        FROM quotes.quotes q
        ORDER BY q.quote_id DESC
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query top quotes: {}", e);
            return return_500();
        },
    };
    for row in rows {
        let quote_id: i64 = row.get(0);
        let author: String = row.get(1);
        let message_type: String = row.get(2);
        let body_in_db: String = row.get(3);

        // render the quote
        let body = match message_type.as_str() {
            "F" => body_in_db,
            "M" => format!("<{}> {}", author, body_in_db),
            "A" => format!("* {} {}", author, body_in_db),
            other => format!("{}? <{}> {}", other, author, body_in_db),
        };
        quotes.push(serde_json::json!({
            "id": quote_id,
            "body": body,
        }));
    }

    // add votes
    let vote_statement_res = db_conn.prepare("
        SELECT v.voter_lowercase, CAST(v.points AS bigint) FROM quotes.quote_votes v WHERE v.quote_id = $1 ORDER BY v.vote_id
    ").await;
    let vote_statement = match vote_statement_res {
        Ok(s) => s,
        Err(e) => {
            error!("failed to prepare vote statement: {}", e);
            return return_500();
        },
    };
    for quote in &mut quotes {
        let quote_id = quote["id"].as_i64().expect("quote ID is not i64");
        let rows = match db_conn.query(&vote_statement, &[&quote_id]).await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to obtain votes for quote {}: {}", quote_id, e);
                return return_500();
            },
        };
        let mut votes = Vec::new();
        let mut total_points: i64 = 0;
        for row in &rows {
            let voter: String = row.get(0);
            let points: i64 = row.get(1);
            total_points += points;
            votes.push(serde_json::json!({
                "voter": voter,
                "value": points,
            }));
        }
        quote["score"] = total_points.into();
        quote["votes"] = votes.into();
    }

    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let quotes_json = serde_json::Value::Array(quotes);
        match render_json(&quotes_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        let mut ctx = tera::Context::new();
        ctx.insert("quotes", &quotes);
        match render_template("quotesvotes.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}

async fn handle_plaintext_aliases_for_nick(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let nick_opt = query_pairs.get("nick");
    let nick = match nick_opt {
        Some(n) => n.clone().into_owned(),
        None => {
            return Response::builder()
                .status(400)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(Body::from("GET parameter \"nick\" required."))
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

    let mut base_to_aliases: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut alias_to_base: HashMap<String, String> = HashMap::new();

    if let Some(plugins) = bot_config["plugins"].as_array() {
        for plugin in plugins {
            if plugin["name"] == "config_user_alias" && plugin["enabled"].as_bool().unwrap_or(false) {
                if let Some(latu) = plugin["config"]["lowercase_alias_to_username"].as_object() {
                    for (alias, base_nick_val) in latu {
                        if let Some(base_nick) = base_nick_val.as_str() {
                            alias_to_base.insert(alias.clone(), base_nick.to_owned());
                            base_to_aliases.entry(base_nick.to_owned())
                                .or_insert_with(|| BTreeSet::new())
                                .insert(alias.clone());
                        }
                    }
                }
            }
        }
    }

    let base = alias_to_base.get(&nick).unwrap_or(&nick);
    let body = match base_to_aliases.get(base) {
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
        .body(Body::from(body))
        .or_else(|e| {
            error!("failed to assemble plaintext response: {}", e);
            return return_500();
        })
}

async fn handle_nicks_aliases(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

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
    for (base_nick, alias) in &alias_list {
        aliases.push(serde_json::json!({
            "nick": base_nick,
            "alias": alias,
        }));
    }

    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let aliases_json = serde_json::Value::Array(aliases);
        match render_json(&aliases_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        let mut last_nick = None;
        for alias_value in &mut aliases {
            let alias_obj = alias_value.as_object_mut().unwrap();

            let new_last_nick = alias_obj["nick"].as_str().map(|s| s.to_owned());
            let nick_changed = if last_nick == new_last_nick {
                false
            } else {
                last_nick = new_last_nick;
                true
            };

            alias_obj.insert("nick_changed".to_owned(), serde_json::Value::Bool(nick_changed));
        }

        let mut ctx = tera::Context::new();
        ctx.insert("aliases", &aliases);
        match render_template("aliases.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}

async fn handle_thanks(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut user_name_set = BTreeSet::new();
    let mut thanks_counts: HashMap<(String, String), i64> = HashMap::new();
    let query_res = db_conn.query("
        SELECT t.thanker_lowercase, t.thankee_lowercase, COUNT(*) thanks_count
        FROM thanks.thanks t
        WHERE t.deleted = FALSE
        GROUP BY t.thanker_lowercase, t.thankee_lowercase
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query thanks: {}", e);
            return return_500();
        },
    };
    for row in rows {
        let thanker: String = row.get(0);
        let thankee: String = row.get(1);
        let count: i64 = row.get(2);

        user_name_set.insert(thanker.clone());
        user_name_set.insert(thankee.clone());

        thanks_counts.insert((thanker, thankee), count);
    }

    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let mut from_to_thanks: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for ((thanker, thankee), count) in thanks_counts.iter() {
            let to_thanks = from_to_thanks.entry(thanker.clone())
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut().unwrap();

            let thanks_value = to_thanks.entry(thankee.clone())
                .or_insert_with(|| serde_json::json!(0));

            let current_value = thanks_value.as_i64().unwrap();
            *thanks_value = serde_json::json!(current_value + *count);
        }

        let from_to_thanks_json = serde_json::Value::Object(from_to_thanks);
        match render_json(&from_to_thanks_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        let user_names: Vec<String> = user_name_set.iter()
            .map(|un| un.clone())
            .collect();

        // complete the values
        for thanker in &user_names {
            for thankee in &user_names {
                thanks_counts.entry((thanker.clone(), thankee.clone()))
                    .or_insert(0);
            }
        }

        let mut total_given: HashMap<String, i64> = user_names.iter()
            .enumerate()
            .map(|(i, _name)| (i.to_string(), 0))
            .collect();
        let mut total_received: HashMap<String, i64> = total_given.clone();
        let mut thanks_from_to: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut total_count = 0;

        for (r, thanker) in user_names.iter().enumerate() {
            let r_string = r.to_string();
            let thanks_to = thanks_from_to.entry(r_string.clone())
                .or_insert_with(|| HashMap::new());

            for (e, thankee) in user_names.iter().enumerate() {
                let e_string = e.to_string();

                let pair_count = *thanks_counts.get(&(thanker.clone(), thankee.clone())).unwrap();

                *total_given.get_mut(&r_string).unwrap() += pair_count;
                *total_received.get_mut(&e_string).unwrap() += pair_count;
                thanks_to.insert(e_string, pair_count);
                total_count += pair_count;
            }
        }

        let users: Vec<serde_json::Value> = user_names.iter()
            .enumerate()
            .map(|(i, name)| serde_json::json!({
                "index": i.to_string(),
                "name": name,
            }))
            .collect();

        let mut ctx = tera::Context::new();
        ctx.insert("users", &users);
        ctx.insert("thanks_from_to", &thanks_from_to);
        ctx.insert("total_given", &total_given);
        ctx.insert("total_received", &total_received);
        ctx.insert("total_count", &total_count);
        match render_template("thanks.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}

async fn handle_bim_rides(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    if request.method() != Method::GET {
        return return_405().await;
    }

    let query_pairs = get_query_pairs(request);

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    async fn assemble_fixed_couplings() -> HashMap<String, HashMap<u32, Vec<u32>>> {
        let mut ret = HashMap::new();

        let bot_config = match get_bot_config().await {
            Some(bc) => bc,
            None => return ret,
        };

        let plugins = match bot_config["plugins"].as_array() {
            Some(ps) => ps,
            None => return ret,
        };
        let bim_plugin_opt = plugins.iter()
            .filter(|p| p["enabled"].as_bool().unwrap_or(false))
            .filter(|p| p["name"].as_str().map(|n| n == "bim").unwrap_or(false))
            .nth(0);
        let bim_plugin = match bim_plugin_opt {
            Some(bp) => bp,
            None => return ret,
        };

        let company_to_bim_database_path = match bim_plugin["config"]["company_to_bim_database_path"].as_object() {
            Some(ctbdpo) => ctbdpo,
            None => return ret,
        };
        for (company, bim_database_path_object) in company_to_bim_database_path.iter() {
            if bim_database_path_object.is_null() {
                ret.insert(
                    company.clone(),
                    HashMap::new(),
                );
                continue;
            }

            let bim_database_path = match bim_database_path_object.as_str() {
                Some(bdp) => bdp,
                None => continue,
            };
            let file = match File::open(bim_database_path) {
                Ok(f) => f,
                Err(e) => {
                    error!("failed to open bim database file {:?}: {}", bim_database_path, e);
                    continue;
                },
            };
            let bim_database: serde_json::Value = match serde_json::from_reader(file) {
                Ok(bd) => bd,
                Err(e) => {
                    error!("failed to parse bim database file {:?}: {}", bim_database_path, e);
                    continue;
                }
            };
            let bim_array = match bim_database.as_array() {
                Some(ba) => ba,
                None => {
                    error!("bim database file {:?} not a list", bim_database_path);
                    continue;
                },
            };
            for bim in bim_array {
                let number_i64 = match bim["number"].as_i64() {
                    Some(bn) => bn,
                    None => {
                        error!("number in {} in bim database file {:?} not an i64", bim, bim_database_path);
                        continue;
                    },
                };
                let number_u32: u32 = match number_i64.try_into() {
                    Ok(n) => n,
                    Err(_) => {
                        error!("number {} in bim database file {:?} not convertible to u32", number_i64, bim_database_path);
                        continue;
                    },
                };
                let fixed_coupling = match bim["fixed_coupling"].as_array() {
                    Some(fc) => fc,
                    None => {
                        error!("fixed_coupling in {} in bim database file {:?} not an array", bim, bim_database_path);
                        continue;
                    },
                };
                let mut fixed_coupling_u32s: Vec<u32> = Vec::new();
                for fixed_coupling_value in fixed_coupling {
                    let fc_i64 = match fixed_coupling_value.as_i64() {
                        Some(n) => n,
                        None => {
                            error!("fixed coupling value {} in bim database file {:?} not an i64", fixed_coupling_value, bim_database_path);
                            continue;
                        },
                    };
                    let fc_u32: u32 = match fc_i64.try_into() {
                        Ok(n) => n,
                        Err(_) => {
                            error!("fixed coupling value {} in bim database file {:?} not a u32", fc_i64, bim_database_path);
                            continue;
                        },
                    };
                    fixed_coupling_u32s.push(fc_u32);
                }
                if fixed_coupling_u32s.len() > 0 {
                    ret.entry(company.clone())
                        .or_insert_with(|| HashMap::new())
                        .insert(number_u32, fixed_coupling_u32s);
                }
            }
        }

        ret
    }
    let company_to_vehicle_to_fixed_coupling = assemble_fixed_couplings()
        .await;

    let mut rides: Vec<(String, String, i64, Option<String>)> = Vec::new();
    let query_res = db_conn.query("
        SELECT lr.company, lr.vehicle_number, CAST(SUM(lr.ride_count) AS bigint), MAX(lr.last_line)
        FROM bim.last_rides lr
        GROUP BY lr.company, lr.vehicle_number
        ORDER BY lr.company, lr.vehicle_number
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query rides: {}", e);
            return return_500();
        },
    };
    let mut company_to_known_fixed_couplings: HashMap<String, HashSet<Vec<u32>>> = HashMap::new();
    for row in rows {
        let company: String = row.get(0);
        let vehicle_number_i64: i64 = row.get(1);
        let ride_count: i64 = row.get(2);
        let last_line: Option<String> = row.get(3);

        let vehicle_number_u32: u32 = vehicle_number_i64.try_into().unwrap();
        let fixed_coupling_opt = company_to_vehicle_to_fixed_coupling
            .get(&company)
            .map(|v2fc| v2fc.get(&vehicle_number_u32))
            .flatten();
        if let Some(fixed_coupling) = fixed_coupling_opt {
            let known_fixed_couplings = company_to_known_fixed_couplings
                .entry(company.clone())
                .or_insert_with(|| HashSet::new());
            if known_fixed_couplings.contains(fixed_coupling) {
                // we've already output this one
                continue;
            }

            // remember this coupling
            known_fixed_couplings.insert(fixed_coupling.clone());

            // output it
            let vehicle_number_strings: Vec<String> = fixed_coupling
                .iter()
                .map(|n| n.to_string())
                .collect();
            let vehicle_number_string = vehicle_number_strings.join("+");
            rides.push((company, vehicle_number_string, ride_count, last_line));
        } else {
            // not a fixed coupling; output
            let vehicle_number_string = vehicle_number_u32.to_string();
            rides.push((company, vehicle_number_string, ride_count, last_line));
        }
    }

    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        let rides_json_vec: Vec<serde_json::Value> = rides.iter()
            .map(|(comp, veh_num, ride_count, last_line)| serde_json::json!({
                "company": comp.clone(),
                "vehicle_numbers": *veh_num,
                "ride_count": *ride_count,
                "last_line": last_line.clone(),
            }))
            .collect();
        let rides_json = serde_json::Value::Array(rides_json_vec);
        match render_json(&rides_json, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    } else {
        let mut ctx = tera::Context::new();
        ctx.insert("rides", &rides);
        match render_template("bimrides.html.tera", &ctx, 200, vec![]).await {
            Some(r) => Ok(r),
            None => return_500(),
        }
    }
}


async fn handle_request(request: Request<Body>) -> Result<Response<Body>, Infallible> {
    match request.uri().path() {
        "/" => handle_index(&request).await,
        "/topquotes" => handle_top_quotes(&request).await,
        "/quotesvotes" => handle_quotes_votes(&request).await,
        "/nicks-aliases" => handle_nicks_aliases(&request).await,
        "/aliases" => handle_plaintext_aliases_for_nick(&request).await,
        "/thanks" => handle_thanks(&request).await,
        "/bim-rides" => handle_bim_rides(&request).await,
        _ => return_404().await,
    }
}


#[tokio::main]
async fn main() {
    env_logger::init();

    // get config path
    let args: Vec<OsString> = env::args_os().collect();
    let config_path = if args.len() < 2 {
        PathBuf::from("webconfig.toml")
    } else {
        PathBuf::from(&args[1])
    };

    let config: WebConfig = {
        let mut file = File::open(config_path)
            .expect("failed to open config file");
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .expect("failed to read config file");
        toml::from_slice(&bytes)
            .expect("failed to parse config file")
    };
    let listen_address = config.listen.clone();
    let template_glob = config.template_glob.clone();
    CONFIG.set(RwLock::new(config))
        .expect("failed to set initial config");

    let tera = Tera::new(&template_glob)
        .expect("failed to initialize Tera");
    TERA.set(RwLock::new(tera))
        .expect("failed to set initial Tera");

    let make_service = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    });
    let server = Server::bind(&listen_address)
        .serve(make_service);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
