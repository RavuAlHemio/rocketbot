mod aliases;
mod bim;
mod config;
mod quotes;
mod templating;
mod thanks;


use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use askama::Template;
use form_urlencoded;
use hyper::{Body, Method, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use log::error;
use once_cell::sync::OnceCell;
use serde::{Serialize, Deserialize};
use serde_json;
use tokio;
use tokio::sync::{RwLock, RwLockReadGuard};
use tokio_postgres::{self, NoTls};
use toml;

use crate::aliases::{handle_nicks_aliases, handle_plaintext_aliases_for_nick};
use crate::bim::{
    handle_bim_achievements, handle_bim_coverage, handle_bim_coverage_field, handle_bim_detail,
    handle_bim_histogram_by_day_of_week, handle_bim_latest_rider_count_over_time,
    handle_bim_latest_rider_count_over_time_image, handle_bim_line_detail, handle_bim_ride_by_id,
    handle_bim_rides, handle_bim_types, handle_bim_vehicles, handle_top_bims, handle_top_bim_lines,
    handle_wide_bims,
};
use crate::config::WebConfig;
use crate::quotes::{handle_quotes_votes, handle_top_quotes};
use crate::templating::{Error400Template, Error404Template, Error405Template};
use crate::thanks::handle_thanks;


pub(crate) static CONFIG: OnceCell<RwLock<WebConfig>> = OnceCell::new();


#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "index.html", escape = "none")]
struct IndexTemplate;


fn get_query_pairs<T>(request: &Request<T>) -> HashMap<Cow<str>, Cow<str>> {
    if let Some(q) = request.uri().query() {
        form_urlencoded::parse(q.as_bytes())
            .collect()
    } else {
        HashMap::new()
    }
}


// query_pairs is queried for "format" to decide between HTML and JSON
async fn render_response<S: Serialize + Template>(value: &S, query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>, status: u16, headers: Vec<(String, String)>) -> Option<Response<Body>> {
    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        render_json(value, status, headers).await
    } else {
        render_template(value, status, headers).await
    }
}

async fn render_json<S: Serialize>(value: &S, status: u16, headers: Vec<(String, String)>) -> Option<Response<Body>> {
    let rendered = match serde_json::to_string_pretty(value) {
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

async fn render_template<T: Template>(value: &T, status: u16, headers: Vec<(String, String)>) -> Option<Response<Body>> {
    let rendered = match value.render() {
        Ok(s) => s,
        Err(e) => {
            error!("failed to render template: {}", e);
            return None;
        },
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

pub(crate) async fn get_bot_config() -> Option<serde_json::Value> {
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


async fn return_404(query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>) -> Result<Response<Body>, Infallible> {
    let template = Error404Template;
    match render_response(&template, query_pairs, 404, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn return_400(reason: &str, query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>) -> Result<Response<Body>, Infallible> {
    let template = Error400Template {
        reason: reason.to_owned(),
    };
    match render_response(&template, query_pairs, 400, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn return_405(query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>) -> Result<Response<Body>, Infallible> {
    let template = Error405Template {
        allowed_methods: vec!["GET".to_owned()],
    };
    let headers = vec![
        ("Accept".to_owned(), "GET".to_owned()),
    ];
    match render_response(&template, query_pairs, 405, headers).await {
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
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let template = IndexTemplate;
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
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
        "/bim-types" => handle_bim_types(&request).await,
        "/bim-vehicles" => handle_bim_vehicles(&request).await,
        "/bim-coverage" => handle_bim_coverage(&request).await,
        "/bim-detail" => handle_bim_detail(&request).await,
        "/bim-line-detail" => handle_bim_line_detail(&request).await,
        "/top-bims" => handle_top_bims(&request).await,
        "/wide-bims" => handle_wide_bims(&request).await,
        "/bim-coverage-field" => handle_bim_coverage_field(&request).await,
        "/top-bim-lines" => handle_top_bim_lines(&request).await,
        "/bim-achievements" => handle_bim_achievements(&request).await,
        "/bim-ride-by-id" => handle_bim_ride_by_id(&request).await,
        "/bim-latest-rider-count-over-time" => handle_bim_latest_rider_count_over_time(&request).await,
        "/bim-latest-rider-count-over-time/image" => handle_bim_latest_rider_count_over_time_image(&request).await,
        "/bim-histogram-day-of-week" => handle_bim_histogram_by_day_of_week(&request).await,
        _ => {
            let query_pairs = get_query_pairs(&request);
            return_404(&query_pairs).await
        },
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
    CONFIG.set(RwLock::new(config))
        .expect("failed to set initial config");

    let make_service = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    });
    let server = Server::bind(&listen_address)
        .serve(make_service);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
