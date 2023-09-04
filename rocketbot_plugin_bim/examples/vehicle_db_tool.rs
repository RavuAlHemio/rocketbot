use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ciborium;
use clap::Parser;
use rocketbot_bim_common::VehicleInfo;


#[derive(Parser)]
enum Mode {
    #[command(about = "Convert a JSON vehicle database to CBOR.")]
    ToCbor(ToCborOptions),

    #[command(about = "Convert a CBOR vehicle database to JSON.")]
    ToJson(ToJsonOptions),
}

#[derive(Parser)]
struct ToCborOptions {
    #[arg(help = "The JSON vehicle database file to read.")]
    pub source_json: PathBuf,

    #[arg(help = "The CBOR vehicle database file to write.")]
    pub target_cbor: PathBuf,

    #[arg(long, help = "Output timing statistics when converting.")]
    pub benchmark: bool,
}

#[derive(Parser)]
struct ToJsonOptions {
    #[arg(help = "The CBOR vehicle database file to read.")]
    pub source_cbor: PathBuf,

    #[arg(help = "The JSON vehicle database file to write.")]
    pub target_json: PathBuf,

    #[arg(long, help = "Output timing statistics when converting.")]
    pub benchmark: bool,
}


fn read_json_vehicle_db(path: &Path) -> Vec<VehicleInfo> {
    if path.to_string_lossy() == "-" {
        let stdin = io::stdin().lock();
        serde_json::from_reader(stdin)
            .expect("failed to read source JSON from stdin")
    } else {
        let f = File::open(path)
            .expect("failed to open source JSON file");
        serde_json::from_reader(f)
            .expect("failed to read source JSON file")
    }
}


fn read_cbor_vehicle_db(path: &Path) -> Vec<VehicleInfo> {
    if path.to_string_lossy() == "-" {
        let stdin = io::stdin().lock();
        ciborium::from_reader(stdin)
            .expect("failed to read source CBOR from stdin")
    } else {
        let f = File::open(path)
            .expect("failed to open source CBOR file");
        ciborium::from_reader(f)
            .expect("failed to read source CBOR file")
    }
}


fn write_json_vehicle_db(path: &Path, db: &[VehicleInfo]) {
    if path.to_string_lossy() == "-" {
        let stdout = io::stdout().lock();
        serde_json::to_writer_pretty(stdout, db)
            .expect("failed to write target JSON to stdout")
    } else {
        let f = File::create(path)
            .expect("failed to create target JSON file");
        serde_json::to_writer_pretty(f, db)
            .expect("failed to write target JSON file")
    }
}


fn write_cbor_vehicle_db(path: &Path, db: &[VehicleInfo]) {
    if path.to_string_lossy() == "-" {
        let stdout = io::stdout().lock();
        ciborium::into_writer(db, stdout)
            .expect("failed to write target CBOR to stdout")
    } else {
        let f = File::create(path)
            .expect("failed to create target CBOR file");
        ciborium::into_writer(db, f)
            .expect("failed to write target CBOR file")
    }
}


fn main() {
    let mode = Mode::parse();

    match mode {
        Mode::ToCbor(opts) => {
            let read_start = Instant::now();
            let database = read_json_vehicle_db(&opts.source_json);
            let read_end_write_start = Instant::now();
            write_cbor_vehicle_db(&opts.target_cbor, &database);
            let write_end = Instant::now();
            if opts.benchmark {
                eprintln!(
                    "reading JSON took {}s, writing CBOR took {}s",
                    (read_end_write_start - read_start).as_secs_f64(),
                    (write_end - read_end_write_start).as_secs_f64(),
                );
            }
        },
        Mode::ToJson(opts) => {
            let read_start = Instant::now();
            let database = read_cbor_vehicle_db(&opts.source_cbor);
            let read_end_write_start = Instant::now();
            write_json_vehicle_db(&opts.target_json, &database);
            let write_end = Instant::now();
            if opts.benchmark {
                eprintln!(
                    "reading CBOR took {}s, writing JSON took {}s",
                    (read_end_write_start - read_start).as_secs_f64(),
                    (write_end - read_end_write_start).as_secs_f64(),
                );
            }
        },
    }
}
