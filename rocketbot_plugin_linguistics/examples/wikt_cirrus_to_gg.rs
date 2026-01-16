//! Wiktionary `cirrussearch` export to German gender loader
//!
//! 1. Obtain a current `enwiktionary-...-cirrussearch-content.json.gz` from
//! https://dumps.wikimedia.org/other/cirrussearch/ or a mirror.
//!
//! 2. Create a TOML configuration file:
//! ```toml
//! cirrus_json_gz_path = "..."
//! db_conn_string = "..."
//! ```
//!
//! 3. Run this tool, passing the path to the TOML configuration file.


use std::collections::BTreeSet;
use std::env::args_os;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use flate2::read::GzDecoder;
use rocketbot_plugin_linguistics::GenderFlags;
use serde::{Deserialize, Serialize};
use tokio_postgres::NoTls;
use toml;


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub cirrus_json_gz_path: PathBuf,
    pub db_conn_string: String,
    #[serde(default)] pub empty_first: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Page {
    pub title: String,
    #[serde(rename = "category")] pub categories: BTreeSet<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<OsString> = args_os().collect();
    if args.len() == 2 && args[1].to_string_lossy().starts_with("-") {
        eprintln!("Usage: {} [CONFIG.TOML]", args[0].display());
        return ExitCode::FAILURE;
    }
    let config_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("config.toml")
    };

    let config_string = std::fs::read_to_string(&config_path)
        .expect("failed to read config file");
    let config: Config = toml::from_str(&config_string)
        .expect("failed to parse config file");

    let (mut conn, internal_conn) = tokio_postgres::connect(&config.db_conn_string, NoTls)
        .await.expect("failed to connect to database");
    tokio::spawn(async move {
        if let Err(e) = internal_conn.await {
            eprintln!("connection error: {}", e);
        }
    });

    let gz_file = File::open(&config.cirrus_json_gz_path)
        .expect("failed to open gz file");
    let cirrus_file = GzDecoder::new(gz_file);
    let mut cirrus_buffy = BufReader::new(cirrus_file);

    let xact = conn.transaction()
        .await.expect("failed to start database transaction");
    let insert_stmt = xact.prepare(
        "
            INSERT INTO linguistics.german_genders
                (   word
                ,   gender_flags
                ) VALUES
                (   $1
                ,   $2
                )
        ",
    )
        .await.expect("failed to prepare insertion statement");
    if config.empty_first {
        xact.execute("DELETE FROM linguistics.german_genders", &[])
            .await.expect("failed to execute deletion statement");
    }

    let mut buf = Vec::new();
    loop {
        buf.clear();
        cirrus_buffy.read_until(b'\n', &mut buf)
            .expect("reading failed");
        if buf.len() == 0 {
            // EOF
            break;
        }

        let Ok(page): Result<Page, _> = serde_json::from_slice(&buf)
            else { continue };
        if
                !page.categories.contains("German nouns")
                && !page.categories.contains("German proper nouns")
                && !page.categories.contains("German singularia tantum")
                && !page.categories.contains("German pluralia tantum")
                && !page.categories.contains("German masculine nouns")
                && !page.categories.contains("German feminine nouns")
                && !page.categories.contains("German neuter nouns") {
            continue;
        }

        let mut gender_flags = GenderFlags::empty();
        if page.categories.contains("German masculine nouns") {
            gender_flags |= GenderFlags::MASCULINE;
        }
        if page.categories.contains("German feminine nouns") {
            gender_flags |= GenderFlags::FEMININE;
        }
        if page.categories.contains("German neuter nouns") {
            gender_flags |= GenderFlags::NEUTER;
        }
        if page.categories.contains("German singularia tantum") {
            gender_flags |= GenderFlags::SINGULARE_TANTUM;
        }
        if page.categories.contains("German pluralia tantum") {
            gender_flags |= GenderFlags::PLURALE_TANTUM;
        }
        if page.categories.contains("German male given names") {
            gender_flags |= GenderFlags::MALE_GIVEN;
        }
        if page.categories.contains("German female given names") {
            gender_flags |= GenderFlags::FEMALE_GIVEN;
        }
        if page.categories.contains("German unisex given names") {
            gender_flags |= GenderFlags::UNISEX_GIVEN;
        }

        xact.execute(&insert_stmt, &[
            &page.title,
            &gender_flags.bits(),
        ])
            .await.expect("failed to insert database row");
    }
    xact.commit().await.expect("committing transaction failed");
    ExitCode::SUCCESS
}
