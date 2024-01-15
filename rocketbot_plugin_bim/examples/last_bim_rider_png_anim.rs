use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use clap::Parser;
use futures_util::{pin_mut, TryStreamExt};
use rocketbot_bim_common::VehicleInfo;
use rocketbot_string::NatSortedString;
use serde::{Deserialize, Serialize};


#[derive(Parser)]
struct Opts {
    #[arg(default_value = "last_bim_rider_png_anim.json")]
    pub config_path: PathBuf,
}


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub db_conn_string: String,
    pub vehicle_db_path: String,
    pub company: String,
    pub palette: Vec<u8>,
    pub output_png_path: String,
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct LastRiderChange {
    pub vehicle: NatSortedString,
    pub rider: NatSortedString,
}


async fn get_last_rider_changes(db_conn_string: &str, company: &str, vehicle_number_to_pos: &BTreeMap<NatSortedString, usize>) -> Vec<LastRiderChange> {
    let (client, connection) = tokio_postgres::connect(db_conn_string, tokio_postgres::NoTls)
        .await.expect("failed to connect to Postgres");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let row_iterator = client.query_raw(
        "
            SELECT
                rav.vehicle_number,
                rav.rider_username
            FROM
                bim.rides_and_vehicles rav
            WHERE
                rav.coupling_mode = 'R'
                AND rav.company = $1
            ORDER BY
                rav.\"timestamp\",
                rav.id,
                rav.spec_position,
                rav.fixed_coupling_position
        ",
        &[&company],
    ).await.expect("failed to query database");

    let mut ret = Vec::new();
    {
        pin_mut!(row_iterator);
        while let Some(row) = row_iterator.try_next().await.expect("failed to obtain next row") {
            let vehicle_number: String = row.get(0);
            let rider_username: String = row.get(1);

            let vehicle = NatSortedString::from_string(vehicle_number);
            if !vehicle_number_to_pos.contains_key(&vehicle) {
                // skip
                continue;
            }

            let rider = NatSortedString::from_string(rider_username);

            ret.push(LastRiderChange {
                vehicle,
                rider,
            });
        }
    }
    ret
}


fn load_vehicle_number_to_pos(path: &Path) -> BTreeMap<NatSortedString, usize> {
    let vehicles: Vec<VehicleInfo> = {
        let f = File::open(path)
            .expect("failed to open vehicle database");
        ciborium::from_reader(f)
            .expect("failed to load vehicle database")
    };

    let vehicle_numbers: BTreeSet<NatSortedString> = vehicles.into_iter()
        .map(|v| v.number)
        .collect();

    vehicle_numbers.into_iter()
        .enumerate()
        .map(|(i, vn)| (vn, i))
        .collect()
}


#[tokio::main]
async fn main() {
    let opts = Opts::parse();

    // load config
    let config: Config = {
        let f = File::open(&opts.config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };
    if config.palette.len() % 3 != 0 {
        panic!("palette in config must have a number of elements divisible by 3 (R, G, B)");
    }

    // load vehicle database
    let vehicle_db_path = Path::new(&config.vehicle_db_path);
    let vehicle_number_to_pos = load_vehicle_number_to_pos(vehicle_db_path);

    // load vehicles changing hands
    let last_rider_changes = get_last_rider_changes(&config.db_conn_string, &config.company, &vehicle_number_to_pos).await;

    // find all riders
    let all_riders: BTreeSet<&NatSortedString> = last_rider_changes
        .iter()
        .map(|lrc| &lrc.rider)
        .collect();
    if all_riders.len() > config.palette.len() / 3 {
        panic!("not enough palette colors ({}) for {} riders", config.palette.len() / 3, all_riders.len());
    }
    if all_riders.len() > 255 {
        panic!("too many riders for an indexed PNG image");
    }
    let rider_to_palette_index: BTreeMap<&NatSortedString, u8> = all_riders
        .iter()
        .enumerate()
        .map(|(i, r)| (*r, (i + 1).try_into().unwrap()))
        .collect();

    // grow image
    let mut side_length_usize: usize = 1;
    loop {
        let image_pixels = side_length_usize.checked_mul(side_length_usize).unwrap();
        if image_pixels >= vehicle_number_to_pos.len() {
            break;
        }
        side_length_usize = side_length_usize.checked_add(1).unwrap();
    }
    let side_length: u32 = side_length_usize.try_into().expect("image size does not fit into u32");

    let mut full_palette = Vec::with_capacity(3 + config.palette.len());
    full_palette.push(0x00);
    full_palette.push(0x00);
    full_palette.push(0x00);
    full_palette.extend(config.palette.iter().map(|b| *b));
    /*
    let palette: [u8; 39] = [
        0x00, 0x00, 0x00,
        0x59, 0x2C, 0x24,
        0x65, 0x32, 0x3B,
        0x67, 0x3E, 0x55,
        0x5B, 0x4F, 0x6D,
        0x42, 0x60, 0x7B,
        0x22, 0x71, 0x7C,
        0x19, 0x7E, 0x71,
        0x3E, 0x89, 0x5C,
        0x69, 0x90, 0x46,
        0x98, 0x92, 0x38,
        0xC6, 0x8F, 0x3F,
        0xEE, 0x8A, 0x5C,
    ];
    */

    // 0 = opaque, 255 = transparent
    // if shorter than palette: all other colors assumed opaque
    let transparency: [u8; 1] = [0x00];

    let num_frames: u32 = (last_rider_changes.len() + 1).try_into().unwrap();

    let mut pixelfield = vec![0u8; side_length_usize*side_length_usize];

    let mut png_bytes = Vec::new();

    {
        let png_cursor = Cursor::new(&mut png_bytes);
        let mut apng = png::Encoder::new(png_cursor, side_length, side_length);
        apng.set_depth(png::BitDepth::Eight);
        apng.set_color(png::ColorType::Indexed);
        apng.set_palette(&full_palette);
        apng.set_trns(&transparency[..]);
        apng.set_animated(num_frames, 0)
            .expect("failed to set animation");
        let mut writer = apng.write_header()
            .expect("failed to write PNG header");

        // initial (empty) pixel field
        writer.write_image_data(&pixelfield)
            .expect("failed to write initial pixel field");

        for last_rider_change in &last_rider_changes {
            let pixelfield_index = vehicle_number_to_pos.get(&last_rider_change.vehicle)
                .expect("cannot find pixel field index for vehicle");
            let palette_index = rider_to_palette_index.get(&last_rider_change.rider)
                .expect("cannot find palette index for rider");
            pixelfield[*pixelfield_index] = *palette_index;

            writer.write_image_data(&pixelfield)
                .expect("failed to write initial pixel field");
        }

        writer.finish()
            .expect("failed to finalize PNG");
    }

    // write out PNG
    std::fs::write(&config.output_png_path, &png_bytes)
        .expect("failed to write out PNG");
}
