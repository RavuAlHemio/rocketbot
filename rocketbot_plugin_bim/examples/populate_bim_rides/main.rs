use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs::File;

use chrono::{Local, NaiveDateTime, TimeZone};
use rocketbot_bim_common::{VehicleInfo, VehicleNumber};
use rocketbot_plugin_bim::{CompanyDefinition, increment_rides_by_spec};
use serde::{Deserialize, Serialize};
use serde_json;
use tokio;
use tokio_postgres::NoTls;


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct LogEntry {
    pub id: String,
    pub message: String,
    pub timestamp: String,
    pub username: String,
}
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct Log {
    pub messages: Vec<LogEntry>,
}


#[tokio::main]
async fn main() {
    // load messages
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() < 4 || args.len() > 5 {
        eprintln!("Usage: populate_bim_rides DBCONNSTRING COMPANY MESSAGES [BIMDATABASE]");
        std::process::exit(1);
    }

    let mut log: Log = {
        let file = File::open(&args[3])
            .expect("failed to open file");
        serde_json::from_reader(file)
            .expect("failed to load log")
    };
    log.messages.sort_unstable_by_key(|e| e.timestamp.clone());

    let bim_database_opt = if let Some(bdfn) = args.get(4) {
        let f = File::open(bdfn)
            .expect("failed to open bim database");
        let mut vehicles: Vec<VehicleInfo> = serde_json::from_reader(f)
            .expect("failed to parse bim database");
        let vehicle_hash_map: HashMap<VehicleNumber, VehicleInfo> = vehicles.drain(..)
            .map(|vi| (vi.number.clone(), vi))
            .collect();
        Some(vehicle_hash_map)
    } else {
        None
    };

    let company = args[2].to_str().expect("company name is not valid UTF-8");

    let conn_string = args[1].to_str().expect("connection string is not valid UTF-8");
    let (mut db_client, db_conn) = tokio_postgres::connect(conn_string, NoTls).await
        .expect("failed to connect to Postgres server");
    tokio::spawn(async move {
        if let Err(e) = db_conn.await {
            eprintln!("database connection error: {}", e);
        }
    });
    let placeholder_company = CompanyDefinition::placeholder();
    for message in &log.messages {
        let naive_utc_timestamp = NaiveDateTime::parse_from_str(&message.timestamp, "%Y-%m-%dT%H:%M:%S%.3fZ")
            .expect("failed to parse timestamp");
        let timestamp = Local.from_utc_datetime(&naive_utc_timestamp);
        println!("[{}] <{}> {}", timestamp, message.username, message.message);
        increment_rides_by_spec(
            &mut db_client,
            bim_database_opt.as_ref(),
            company,
            &placeholder_company,
            &message.username,
            timestamp,
            &message.message,
            true,
            false,
            false,
        ).await
            .expect("failed to increment ride");
    }
}
