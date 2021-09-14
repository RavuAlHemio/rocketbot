use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;

use rocketbot_makepdf::model::PdfBinaryDataDescription;
use serde_json::to_value;


fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 2 {
        eprintln!("Usage: binarify FILENAME");
        std::process::exit(1);
    }

    let bs = {
        let mut f = File::open(&args[1])
            .expect("failed to open file");
        let mut bs = Vec::new();
        f.read_to_end(&mut bs)
            .expect("failed to read file");
        bs
    };

    let bdd = PdfBinaryDataDescription(bs);
    let val = to_value(bdd)
        .expect("failed to serialize value");
    if let Some(v) = val.as_str() {
        println!("{}", v);
    } else {
        panic!("resulting JSON value is not a string!");
    }
}
