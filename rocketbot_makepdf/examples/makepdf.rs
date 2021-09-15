use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufWriter, Write};

use rocketbot_makepdf::render_description;
use rocketbot_makepdf::model::PdfDescription;
use serde_json;


fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Usage: makepdf [--bd] DEFINITION.json OUTPUT.pdf");
        std::process::exit(1);
    }

    let mut read_base_description = false;
    let first_file_index = if args[1] == "--bd" {
        read_base_description = true;
        2
    } else {
        1
    };

    let defn: PdfDescription = {
        let defn_file = File::open(&args[first_file_index])
            .expect("failed to open definition file");
        let val: serde_json::Value = serde_json::from_reader(defn_file)
            .expect("failed to parse definition file");

        if read_base_description {
            serde_json::from_value(val["base_description"].clone())
                .expect("failed to deserialize base_description")
        } else {
            serde_json::from_value(val)
                .expect("failed to deserialize file")
        }
    };

    let rendered = render_description(&defn)
        .expect("failed to render definition");

    let mut pdf_bytes = Vec::new();
    {
        let mut bufferer = BufWriter::new(&mut pdf_bytes);
        rendered.save(&mut bufferer)
            .expect("saving PDF failed");
    }

    {
        let mut output_file = File::create(&args[first_file_index+1])
            .expect("failed to open output file");
        output_file.write_all(&pdf_bytes)
            .expect("failed to write to output file");
    }
}
