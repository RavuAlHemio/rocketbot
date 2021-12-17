use std::env;
use std::ffi::OsString;
use std::fs::File;

use chrono::{Local, NaiveDateTime, TimeZone};
use rocketbot_plugin_bim::increment_rides_by_spec;
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
    if args.len() != 4 {
        eprintln!("Usage: populate_bim_rides DBCONNSTRING COMPANY MESSAGES");
        std::process::exit(1);
    }

    let mut log: Log = {
        let file = File::open(&args[3])
            .expect("failed to open file");
        serde_json::from_reader(file)
            .expect("failed to load log")
    };
    log.messages.sort_unstable_by_key(|e| e.timestamp.clone());

    let company = args[2].to_str().expect("company name is not valid UTF-8");

    let conn_string = args[1].to_str().expect("connection string is not valid UTF-8");
    let (mut db_client, db_conn) = tokio_postgres::connect(conn_string, NoTls).await
        .expect("failed to connect to Postgres server");
    tokio::spawn(async move {
        if let Err(e) = db_conn.await {
            eprintln!("database connection error: {}", e);
        }
    });
    for message in &log.messages {
        let naive_utc_timestamp = NaiveDateTime::parse_from_str(&message.timestamp, "%Y-%m-%dT%H:%M:%S%.3fZ")
            .expect("failed to parse timestamp");
        let timestamp = Local.from_utc_datetime(&naive_utc_timestamp);
        println!("[{}] <{}> {}", timestamp, message.username, message.message);
        increment_rides_by_spec(
            &mut db_client,
            None,
            company,
            &message.username,
            timestamp,
            &message.message,
            true,
        ).await
            .expect("failed to increment ride");
    }
}
