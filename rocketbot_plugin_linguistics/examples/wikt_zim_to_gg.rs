//! Wiktionary `zim` export to German gender loader
//!
//! 1. Obtain a current `zim` export of English Wiktionary from
//! https://dumps.wikimedia.org/other/kiwix/zim/wiktionary/ or a mirror.
//!
//! 2. Create a TOML configuration file:
//! ```toml
//! zim_path = "..."
//! db_conn_string = "..."
//! ```
//!
//! 3. Run this tool, passing the path to the TOML configuration file.


use std::env::args_os;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use rocketbot_plugin_linguistics::GenderFlags;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use tokio_postgres::{NoTls, Statement, Transaction};
use toml;
use zim::Zim;


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub zim_path: PathBuf,
    pub db_conn_string: String,
    #[serde(default)] pub empty_first: bool,
    #[serde(default)] pub commit_after_empty_count: usize,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Page {
    pub title: String,
    pub flags: GenderFlags,
}

async fn prepare_insert_stmt(xact: &Transaction<'_>) -> Statement {
    xact.prepare(
        "
            INSERT INTO linguistics.german_genders
                (   word
                ,   gender_flags
                ) VALUES
                (   $1
                ,   $2
                )
                ON CONFLICT
                    (   word
                    )   DO UPDATE
                    SET gender_flags
                        = german_genders.gender_flags
                        | excluded.gender_flags
        ",
    )
        .await.expect("failed to prepare insertion statement")
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

    let mut xact = conn.transaction()
        .await.expect("failed to start database transaction");

    let mut insert_stmt = prepare_insert_stmt(&xact).await;
    if config.empty_first {
        xact.execute("DELETE FROM linguistics.german_genders", &[])
            .await.expect("failed to execute deletion statement");
    }

    let german_headword_sel = Selector::parse("strong.headword[lang=de]")
        .expect("failed to compile German headword selector");
    let italic_sel = Selector::parse("i")
        .expect("failed to compile italic selector");
    let gender_abbr_sel = Selector::parse("span.gender abbr")
        .expect("failed to compile gender abbr selector");

    let zim_info = Zim::new(&config.zim_path)
        .expect("failed to parse .zim file headers");
    let article_list = zim_info.article_list_by_title()
        .expect("failed to obtain article list by title")
        .expect("article list by title missing");

    let article_count = article_list.len()
        .expect("failed to obtain number of articles");

    let mut empty_count = 0;
    let mut change_count = 0;
    for article_index in 0..article_count {
        let url_index = match article_list.get(article_index) {
            Ok(Some(ui)) => ui,
            Ok(None) => panic!("no article at index {}", article_index),
            Err(e) => panic!("failed to obtain article at index {}: {}", article_index, e),
        };
        let article_entry = match zim_info.get_by_url_index(url_index) {
            Ok(ae) => ae,
            Err(e) => panic!("failed to obtain article entry (article index {}, URL index {}): {}", article_index, url_index, e),
        };
        let article_content = match zim_info.entry_content(&article_entry) {
            Ok(Some(ac)) => ac,
            Ok(None) => continue, // probably just a redirect
            Err(e) => panic!("failed to obtain article content (namespace {:?}, URL {:?}): {}", article_entry.namespace, article_entry.url, e),
        };
        let article_bytes = match article_content.to_vec() {
            Ok(ab) => ab,
            Err(e) => panic!("failed to obtain article content as bytes (namespace {:?}, URL {:?}): {}", article_entry.namespace, article_entry.url, e),
        };
        let article_string = match String::from_utf8(article_bytes) {
            Ok(s) => s,
            Err(_) => panic!("article content (namespace {:?}, URL {:?}) is not valid UTF-8", article_entry.namespace, article_entry.url),
        };

        let html = Html::parse_document(&article_string);
        for headword in html.select(&german_headword_sel) {
            let headword_text: String = headword
                .text().collect();

            let parent_node = headword.parent().unwrap();
            let parent = ElementRef::wrap(parent_node).unwrap();

            let mut gender_flags = GenderFlags::empty();

            let mut is_proper = false;
            let mut is_plurale_tantum = false;
            let mut is_singulare_tantum = false;
            for italic in parent.select(&italic_sel) {
                let italic_text: String = italic.text().collect();
                if italic_text == "proper noun" {
                    is_proper = true;
                    break;
                } else if italic_text == "plural only" {
                    is_plurale_tantum = true;
                } else if italic_text == "no plural" {
                    is_singulare_tantum = true;
                }
            }
            if is_proper {
                // don't consider genders of proper nouns
                continue;
            }

            if is_plurale_tantum {
                gender_flags |= GenderFlags::PLURALE_TANTUM;
            }
            if is_singulare_tantum {
                gender_flags |= GenderFlags::SINGULARE_TANTUM;
            }

            for gender_abbr_elem in parent.select(&gender_abbr_sel) {
                let gender_abbr: String = gender_abbr_elem
                    .text().collect();
                if gender_abbr == "m" {
                    gender_flags |= GenderFlags::MASCULINE;
                } else if gender_abbr == "f" {
                    gender_flags |= GenderFlags::FEMININE;
                } else if gender_abbr == "n" {
                    gender_flags |= GenderFlags::NEUTER;
                }
            }

            println!("{:?}: {:?}", headword_text, gender_flags);
            if gender_flags.is_empty() {
                empty_count += 1;
                if config.commit_after_empty_count > 0 && empty_count >= config.commit_after_empty_count {
                    // commit!
                    if change_count > 0 {
                        println!("(committing)");
                        xact.commit().await.expect("committing transaction failed");
                        xact = conn.transaction().await.expect("failed to start new transaction");
                        insert_stmt = prepare_insert_stmt(&xact).await;
                    } else {
                        println!("(nothing changed)");
                    }
                    empty_count = 0;
                    change_count = 0;
                }
                continue;
            } else {
                empty_count = 0;
                change_count += 1;
            }
            xact.execute(&insert_stmt, &[
                &headword_text,
                &gender_flags.bits(),
            ])
                .await.expect("failed to insert database row");
        }
    }

    xact.commit().await.expect("committing transaction failed");
    ExitCode::SUCCESS
}
