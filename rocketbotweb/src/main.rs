mod aliases;
mod bim;
mod config;
mod line_graph_drawing;
mod quotes;
mod templating;
mod thanks;
mod util;


use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;

use askama::Template;
use form_urlencoded;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use serde::{Serialize, Deserialize};
use serde_json;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, RwLockReadGuard};
use tokio_postgres::{self, NoTls};
use toml;
use tracing::{debug, error};

use crate::aliases::{handle_nicks_aliases, handle_plaintext_aliases_for_nick};
use crate::bim::achievements::handle_bim_achievements;
use crate::bim::charts::{
    handle_bim_depot_last_rider_pie, handle_bim_first_rider_pie,
    handle_bim_fixed_monopolies_over_time, handle_bim_global_stats,
    handle_bim_histogram_by_day_of_week, handle_bim_histogram_by_line_ride_count_group,
    handle_bim_histogram_by_vehicle_ride_count_group, handle_bim_histogram_fixed_coupling,
    handle_bim_last_rider_pie, handle_bim_latest_rider_count_over_time,
    handle_bim_latest_rider_count_over_time_image, handle_bim_last_rider_histogram_by_fixed_pos,
    handle_bim_type_histogram,
};
use crate::bim::coverage::{
    handle_bim_coverage, handle_bim_coverage_field, handle_bim_line_coverage,
};
use crate::bim::details::{handle_bim_detail, handle_bim_line_detail, handle_bim_ride_by_id};
use crate::bim::drilldown::handle_bim_drilldown;
use crate::bim::query::{handle_bim_query, handle_bim_vehicle_status};
use crate::bim::tables::{
    handle_bim_odds_ends, handle_bim_rides, handle_bim_types, handle_bim_vehicles,
};
use crate::bim::top::{
    handle_bim_fixed_monopolies, handle_bim_last_riders, handle_explorer_bims, handle_top_bim_days,
    handle_top_bim_lines, handle_top_bims, handle_wide_bims,
};
use crate::config::WebConfig;
use crate::quotes::{handle_quotes_votes, handle_top_quotes};
use crate::templating::{Error400Template, Error404Template, Error405Template};
use crate::thanks::handle_thanks;


pub(crate) static CONFIG: OnceCell<RwLock<WebConfig>> = OnceCell::new();

static STATIC_FILE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "/static/",
    "(?P<static_filename>",
        "[A-Za-z0-9_-]+",
        "(?:",
            "[.]",
            "[a-z0-9]+",
        ")+",
    ")",
    "$",
)).expect("failed to compile static file regex"));


#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "index.html")]
struct IndexTemplate;


fn get_query_pairs<'a, T>(request: &'a Request<T>) -> HashMap<Cow<'a, str>, Cow<'a, str>> {
    get_query_pairs_vec(request)
        .into_iter()
        .collect()
}

fn get_query_pairs_multiset<'a, T>(request: &'a Request<T>) -> HashMap<Cow<'a, str>, Vec<Cow<'a, str>>> {
    let mut ret = HashMap::new();
    for (key, value) in get_query_pairs_vec(request) {
        ret
            .entry(key)
            .or_insert_with(|| Vec::with_capacity(1))
            .push(value)
    }
    ret
}

fn get_query_pairs_vec<'a, T>(request: &'a Request<T>) -> Vec<(Cow<'a, str>, Cow<'a, str>)> {
    if let Some(q) = request.uri().query() {
        form_urlencoded::parse(q.as_bytes())
            .collect()
    } else {
        Vec::with_capacity(0)
    }
}


// query_pairs is queried for "format" to decide between HTML and JSON
async fn render_response<S: Serialize + Template>(value: &S, query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>, status: u16, headers: Vec<(String, String)>) -> Option<Response<Full<Bytes>>> {
    if query_pairs.get("format").map(|f| f == "json").unwrap_or(false) {
        render_json(value, status, headers).await
    } else {
        render_template(value, status, headers).await
    }
}

async fn render_json<S: Serialize>(value: &S, status: u16, headers: Vec<(String, String)>) -> Option<Response<Full<Bytes>>> {
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
    match builder.body(Full::new(Bytes::from(rendered))) {
        Ok(r) => Some(r),
        Err(e) => {
            error!("failed to assemble response: {}", e);
            None
        },
    }
}

async fn render_template<T: Template>(value: &T, status: u16, headers: Vec<(String, String)>) -> Option<Response<Full<Bytes>>> {
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
    match builder.body(Full::new(Bytes::from(rendered))) {
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


async fn return_404(query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>) -> Result<Response<Full<Bytes>>, Infallible> {
    let template = Error404Template;
    match render_response(&template, query_pairs, 404, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn return_400(reason: &str, query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>) -> Result<Response<Full<Bytes>>, Infallible> {
    let template = Error400Template {
        reason: reason.to_owned(),
    };
    match render_response(&template, query_pairs, 400, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}

async fn return_405(query_pairs: &HashMap<Cow<'_, str>, Cow<'_, str>>) -> Result<Response<Full<Bytes>>, Infallible> {
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

fn return_500() -> Result<Response<Full<Bytes>>, Infallible> {
    let response_res = Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from("500 Internal Server Error")));
    match response_res {
        Err(e) => panic!("failed to construct 500 response: {}", e),
        Ok(b) => Ok(b),
    }
}

async fn handle_index(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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

async fn handle_static(request: &Request<Incoming>, caps: &regex::Captures<'_>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(&request);
    let filename = caps.name("static_filename")
        .expect("failed to match static_filename")
        .as_str();

    let mut static_path = {
        let config_guard = CONFIG
            .get().expect("CONFIG not set?!")
            .read().await;
        config_guard.static_path.clone()
    };
    static_path.push(filename);

    if !static_path.is_file() {
        return return_404(&query_pairs).await;
    }
    let static_data = match std::fs::read(&static_path) {
        Ok(sd) => sd,
        Err(e) => {
            error!("failed to read static file {:?}: {}", static_path, e);
            return return_500();
        },
    };

    // filename must have an extension because regex matches a dot
    let extension = filename.split('.').last();
    let content_type = match extension {
        Some("js") => "text/javascript",
        Some("ts") => "application/x-typescript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    };

    let response_res = Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .body(Full::new(Bytes::from(static_data)));
    match response_res {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("failed to construct static response: {}", e);
            return return_500();
        },
    }
}


async fn handle_request(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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
        "/bim-line-coverage" => handle_bim_line_coverage(&request).await,
        "/bim-detail" => handle_bim_detail(&request).await,
        "/bim-line-detail" => handle_bim_line_detail(&request).await,
        "/top-bims" => handle_top_bims(&request).await,
        "/wide-bims" => handle_wide_bims(&request).await,
        "/explorer-bims" => handle_explorer_bims(&request).await,
        "/bim-coverage-field" => handle_bim_coverage_field(&request).await,
        "/top-bim-lines" => handle_top_bim_lines(&request).await,
        "/bim-achievements" => handle_bim_achievements(&request).await,
        "/bim-ride-by-id" => handle_bim_ride_by_id(&request).await,
        "/bim-latest-rider-count-over-time" => handle_bim_latest_rider_count_over_time(&request).await,
        "/bim-latest-rider-count-over-time/image" => handle_bim_latest_rider_count_over_time_image(&request).await,
        "/bim-histogram-day-of-week" => handle_bim_histogram_by_day_of_week(&request).await,
        "/bim-histogram-vehicle-ride-count-group" => handle_bim_histogram_by_vehicle_ride_count_group(&request).await,
        "/bim-histogram-line-ride-count-group" => handle_bim_histogram_by_line_ride_count_group(&request).await,
        "/bim-query" => handle_bim_query(&request).await,
        "/bim-last-rider-pie" => handle_bim_last_rider_pie(&request).await,
        "/bim-histogram-fixed-coupling" => handle_bim_histogram_fixed_coupling(&request).await,
        "/bim-global-stats" => handle_bim_global_stats(&request).await,
        "/top-bim-days" => handle_top_bim_days(&request).await,
        "/bim-vehicle-status" => handle_bim_vehicle_status(&request).await,
        "/bim-first-rider-pie" => handle_bim_first_rider_pie(&request).await,
        "/bim-type-histogram" => handle_bim_type_histogram(&request).await,
        "/bim-fixed-monopolies-over-time" => handle_bim_fixed_monopolies_over_time(&request).await,
        "/bim-last-rider-by-pos" => handle_bim_last_rider_histogram_by_fixed_pos(&request).await,
        "/bim-odds-ends" => handle_bim_odds_ends(&request).await,
        "/bim-drilldown" => handle_bim_drilldown(&request).await,
        "/bim-depot-last-rider-pie" => handle_bim_depot_last_rider_pie(&request).await,
        "/bim-last-riders" => handle_bim_last_riders(&request).await,
        "/bim-fixed-monopolies" => handle_bim_fixed_monopolies(&request).await,
        _ => {
            if let Some(caps) = STATIC_FILE_REGEX.captures(request.uri().path()) {
                debug!(
                    "serving static file {:?}; you want to configure your web server to bypass the application for this",
                    caps.name("static_filename").expect("failed to capture static_filename").as_str(),
                );
                handle_static(&request, &caps).await
            } else {
                let query_pairs = get_query_pairs(&request);
                return_404(&query_pairs).await
            }
        },
    }
}


#[tokio::main]
async fn main() {
    // set up tracing
    let (stderr_non_blocking, _guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(false)
        .finish(std::io::stderr());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(stderr_non_blocking)
        .init();

    // get config path
    let args: Vec<OsString> = env::args_os().collect();
    let config_path = if args.len() < 2 {
        PathBuf::from("webconfig.toml")
    } else {
        PathBuf::from(&args[1])
    };

    let config: WebConfig = {
        let s = std::fs::read_to_string(config_path)
            .expect("failed to read config file");
        toml::from_str(&s)
            .expect("failed to parse config file")
    };
    let listen_address = config.listen.clone();
    CONFIG.set(RwLock::new(config))
        .expect("failed to set initial config");

    let listener = TcpListener::bind(listen_address).await
        .expect("failed to create TCP listener");
    loop {
        let (stream, remote_addr) = listener.accept().await
            .expect("failed to accept incoming TCP connection");
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            let serve_result = Builder::new(TokioExecutor::new())
                .http1()
                .http2()
                .serve_connection(io, service_fn(handle_request))
                .await;
            if let Err(e) = serve_result {
                error!("error serving connection from {}: {}", remote_addr, e);
            }
        });
    }
}
