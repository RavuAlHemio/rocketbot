use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufWriter, Write};

use rocketbot_makepdf::render_description;
use rocketbot_makepdf::model::PdfDescription;
use serde_json;


fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: makepdf DEFINITION.json OUTPUT.pdf");
        std::process::exit(1);
    }

    let defn: PdfDescription = {
        let defn_file = File::open(&args[1])
            .expect("failed to open definition file");
        serde_json::from_reader(defn_file)
            .expect("failed to parse definition file")
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
        let mut output_file = File::create(&args[2])
            .expect("failed to open output file");
        output_file.write_all(&pdf_bytes)
            .expect("failed to write to output file");
    }
}
