use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::process::ExitCode;

use ciborium;
use rocketbot_bim_common::VehicleInfo;


fn print_usage() {
    eprintln!("Usage: vehicle_db_tool SOURCE_JSON TARGET_CBOR");
    eprintln!("       vehicle_db_tool --to-json SOURCE_CBOR TARGET_JSON");
}


fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() < 3 || args.len() > 4 {
        print_usage();
        return ExitCode::FAILURE;
    }

    let (to_json, source, target) = if args[1] == "--to-json" {
        if args.len() != 4 {
            print_usage();
            return ExitCode::FAILURE;
        }
        (true, &args[2], &args[3])
    } else {
        (false, &args[1], &args[2])
    };

    let database: Vec<VehicleInfo> = {
        let f = File::open(source)
            .expect("failed to open source file");
        if to_json {
            // from CBOR
            ciborium::from_reader(f)
                .expect("failed to read source CBOR file")
        } else {
            serde_json::from_reader(f)
                .expect("failed to read source JSON file")
        }
    };

    {
        let f = File::create(target)
            .expect("failed to create target file");
        if to_json {
            serde_json::to_writer_pretty(f, &database)
                .expect("failed to write target JSON file");
        } else {
            ciborium::into_writer(&database, f)
                .expect("failed to write target CBOR file");
        }
    }

    ExitCode::SUCCESS
}
