use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Cursor, Write};

use rocketbot_bim_common::ride_table::RideTableData;
use rocketbot_plugin_bim::table_draw::draw_ride_table;
use rocketbot_render_text::map_to_png;
use serde_json;


fn main() {
    const MARGIN: u32 = 8;

    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 3 || args[1] == "--help" {
        eprintln!("Usage: test_draw_table TABLEDATA OUTPNG");
        return;
    }

    let data: RideTableData = {
        let f = File::open(&args[1])
            .expect("failed to open data file");
        serde_json::from_reader(f)
            .expect("failed to read data file")
    };

    // draw image
    let table_canvas = draw_ride_table(&data);
    let mut png_buf = Vec::new();
    {
        let cursor = Cursor::new(&mut png_buf);
        map_to_png(cursor, &table_canvas, MARGIN, MARGIN, MARGIN, MARGIN, &[])
            .expect("failed to write PNG data");
    }

    {
        let mut f = File::create(&args[2])
            .expect("failed to open output PNG");
        f.write_all(&png_buf)
            .expect("failed to write output PNG");
    }
}
