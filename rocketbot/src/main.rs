mod config;
mod errors;
mod jsonage;
mod plugins;
mod socketry;
mod string_utils;


use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use tokio::time::sleep;

use crate::config::{CONFIG_FILE_NAME, load_config};
use crate::errors::GeneralError;
use crate::socketry::connect;


async fn run() -> Result<(), GeneralError> {
    env_logger::init();

    // get config path and load config
    let args_os: Vec<OsString> = std::env::args_os().collect();
    let config_path = match args_os.get(1) {
        Some(cp) => PathBuf::from(cp),
        None => PathBuf::from("config.json"),
    };
    CONFIG_FILE_NAME.set(config_path).expect("config path already set");
    load_config().await?;

    // connect to the server
    let _connection = connect().await;

    // wait for something interesting to happen
    loop {
        sleep(Duration::from_secs(9001)).await;
    }
}

fn main() {
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            run().await
        });

    std::process::exit(
        match result {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{}", e);
                1
            },
        }
    )
}
