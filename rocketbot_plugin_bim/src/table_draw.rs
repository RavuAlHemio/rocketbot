//! Table image rendering for bim rides.
//!
//! Generally, the following structure is rendered:
//!
//! ```plain
//! ┌─────────┬─────────┬───────────────────────────┬───────────────────────────────────┐
//! │ vehicle │         │ ravu.al.hemio             │ other                             |
//! ├─────────┼─────────┼───────────────────────────┼───────────────────────────────────┤
//! │ 4096    │ same    │  1x (09:16/25)            │  1x (paulchen 24.02.2023 22:03/5) |
//! │         │ coupled │ 12x (25.02.2023 22:03/25) │ 12x (Steve    23.02.2023 22:00/5) |
//! ├─────────┼─────────┼───────────────────────────┼───────────────────────────────────┤
//! │ 1496    │ same    │  1x (09:16/25)            │  1x (paulchen 24.02.2023 22:03/5) |
//! │         │ coupled │ 12x (25.02.2023 22:03/25) │ 12x (Steve    23.02.2023 22:00/5) |
//! └─────────┴─────────┴───────────────────────────┴───────────────────────────────────┘
//! ```
//!
//! The data is actually arranged in the following columns for improved visual alignment:
//!
//! ```plain
//! │ 4096    │ same    │ 421x (09:16/25)            │ 421x (paulchen 24.02.2023 22:03/5) |
//!   ╰──0──╯   ╰──1──╯   ╰2r╯╰─────────3──────────╯   ╰4r╯╰───5────╯╰────────6─────────╯
//! ```
//!
//! (Columns marked with `r` are right-aligned, the others are left-aligned.)


use std::collections::HashMap;
use std::fmt::Display;
use std::iter::once;

use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};

use rocketbot_render_text::{font_line_height, map_to_dimensions, render_text};


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RideTableData {
    /// The number of the ride.
    pub ride_id: i64,

    /// The line on which the vehicles have been ridden.
    pub line: Option<String>,

    /// The name of the rider. Used as a column header.
    pub rider_name: String,

    /// The vehicles ridden.
    pub vehicles: Vec<RideTableVehicle>,

    /// The time relative to which to show timestamps. This shortens timestamps to only the time if
    /// the ride happened within the past 24 hours. `None` causes the full timestamp to always be
    /// shown.
    pub relative_time: Option<DateTime<Local>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Ride {
    /// The timestamp at which the vehicle was ridden.
    pub timestamp: DateTime<Local>,

    /// The line in which the vehicle was ridden.
    pub line: Option<String>,
}
impl Ride {
    pub fn stringify<Tz: TimeZone>(&self, anchor_timestamp: Option<DateTime<Tz>>) -> String {
        if let Some(line) = &self.line {
            format!("{}/{}", format_timestamp(self.timestamp, anchor_timestamp), line)
        } else {
            format_timestamp(self.timestamp, anchor_timestamp)
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct UserRide {
    /// The username of the rider who rode this vehicle.
    pub rider_username: String,

    /// All the other ride data.
    pub ride: Ride,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RideTableVehicle {
    /// The number of this vehicle.
    pub vehicle_number: String,

    /// The number of times this vehicle (exactly) was ridden by the rider registering the ride.
    pub my_same_count: i64,

    /// The last time this vehicle (exactly) was ridden by the rider registering the ride.
    pub my_same_last: Option<Ride>,

    /// The number of times a vehicle coupled with this one was ridden by the rider registering the
    /// ride.
    pub my_coupled_count: i64,

    /// The timestamp and line of the last time a vehicle coupled with this one was ridden by the
    /// rider registering the ride.
    pub my_coupled_last: Option<Ride>,

    /// The number of times this vehicle (exactly) was ridden by a different rider.
    pub other_same_count: i64,

    /// The rider's username and timestamp of the last time this vehicle (exactly) was ridden by a
    /// different rider.
    pub other_same_last: Option<UserRide>,

    /// The number of times a vehicle coupled with this one was ridden by the rider registering the
    /// ride.
    pub other_coupled_count: i64,

    /// The rider's username and timestamp of the last time a vehicle coupled with this one was
    /// ridden by a different rider.
    pub other_coupled_last: Option<UserRide>,
}


/// Formats a timestamp, possibly relative to another timestamp.
///
/// If an anchor timestamp is provided and the timestamp has happened less than 24h before the
/// anchor timestamp, the timestamp is formatted as time-only. Otherwise, it is formatted as date
/// and time.
fn format_timestamp<Tz1: TimeZone, Tz2: TimeZone>(
    timestamp: DateTime<Tz1>,
    anchor_timestamp: Option<DateTime<Tz2>>,
) -> String
    where
        Tz1::Offset: Display {
    if let Some(anchor) = anchor_timestamp {
        if anchor >= timestamp && anchor.signed_duration_since(timestamp.clone()).num_hours() < 24 {
            return timestamp.format("%H:%M").to_string();
        }
    }
    timestamp.format("%d.%m.%Y %H:%M").to_string()
}


/// Calculate the width of a column containing the given images.
fn calculate_width<'a, I: IntoIterator<Item = &'a HashMap<(u32, u32), u8>>>(images: I) -> u32 {
    images.into_iter()
        .map(|m| map_to_dimensions(m))
        .map(|(w, _h)| w)
        .max()
        .unwrap_or(0)
}


/// Place an image on a canvas. Blends by intensity.
fn place_on_canvas(canvas: &mut HashMap<(u32, u32), u8>, image: &HashMap<(u32, u32), u8>, left: u32, top: u32) {
    for ((x, y), b) in image {
        let pixel_ref = canvas
            .entry((left + *x, top + *y))
            .or_insert(0);
        *pixel_ref = pixel_ref.saturating_add(*b);
    }
}

/// Draw a horizontal line on the canvas.
fn draw_line(
    canvas: &mut HashMap<(u32, u32), u8>,
    y_cursor: &mut u32,
    line_height: u32,
    full_table_width: u32,
) {
    const LINE_ABOVE: u32 = 6;
    const LINE_BELOW: u32 = 2;
    const LINE_THICKNESS: u32 = 1;

    *y_cursor += line_height;

    // draw a line
    *y_cursor += LINE_ABOVE;
    let mut x_cursor = 0;
    for _ in 0..LINE_THICKNESS {
        while x_cursor < full_table_width {
            canvas.insert((x_cursor, *y_cursor), u8::MAX);
            x_cursor += 1;
        }
        *y_cursor += 1;
    }
    *y_cursor += LINE_BELOW;
}

pub fn draw_ride_table(
    table: &RideTableData,
) -> HashMap<(u32, u32), u8> {
    let line_height = font_line_height();
    const HORIZONTAL_MARGIN: u32 = 8;
    const COLUMN_SPACING: u32 = 16;

    // render each required piece
    let ride_text = if let Some(line) = &table.line {
        render_text(&format!("ride {} (line {}):", table.ride_id, line))
    } else {
        render_text(&format!("ride {}:", table.ride_id))
    };
    let vehicle_heading = render_text("vehicle");
    let rider_username_heading = render_text(&table.rider_name);
    let other_heading = render_text("other");
    let same_tag = render_text("same");
    let coupled_tag = render_text("coupled");

    let mut vehicle_numbers = Vec::with_capacity(table.vehicles.len());
    let mut my_same_counts = Vec::with_capacity(table.vehicles.len());
    let mut my_same_rides = Vec::with_capacity(table.vehicles.len());
    let mut my_coupled_counts = Vec::with_capacity(table.vehicles.len());
    let mut my_coupled_rides = Vec::with_capacity(table.vehicles.len());
    let mut other_same_counts = Vec::with_capacity(table.vehicles.len());
    let mut other_same_names = Vec::with_capacity(table.vehicles.len());
    let mut other_same_rides = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_counts = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_names = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_rides = Vec::with_capacity(table.vehicles.len());

    for vehicle in &table.vehicles {
        vehicle_numbers.push(render_text(&vehicle.vehicle_number));

        my_same_counts.push(render_text(&format!("{}\u{D7}", vehicle.my_same_count)));
        if let Some(my_same_last) = &vehicle.my_same_last {
            my_same_rides.push(render_text(&format!(" ({})", my_same_last.stringify(table.relative_time))));
        } else {
            my_same_rides.push(HashMap::new());
        }

        my_coupled_counts.push(render_text(&format!("{}\u{D7}", vehicle.my_coupled_count)));
        if let Some(my_coupled_last) = &vehicle.my_coupled_last {
            my_coupled_rides.push(render_text(&format!(" ({})", my_coupled_last.stringify(table.relative_time))));
        } else {
            my_coupled_rides.push(HashMap::new());
        }

        other_same_counts.push(render_text(&format!("{}\u{D7}", vehicle.other_same_count)));
        if let Some(other_same_last) = &vehicle.other_same_last {
            other_same_names.push(render_text(&format!(" ({}", other_same_last.rider_username)));
            other_same_rides.push(render_text(&format!(" {})", other_same_last.ride.stringify(table.relative_time))));
        } else {
            other_same_names.push(HashMap::new());
            other_same_rides.push(HashMap::new());
        }

        other_coupled_counts.push(render_text(&format!("{}\u{D7}", vehicle.other_coupled_count)));
        if let Some(other_coupled_last) = &vehicle.other_coupled_last {
            other_coupled_names.push(render_text(&format!(" ({}", other_coupled_last.rider_username)));
            other_coupled_rides.push(render_text(&format!(" {})", other_coupled_last.ride.stringify(table.relative_time))));
        } else {
            other_coupled_names.push(HashMap::new());
            other_coupled_rides.push(HashMap::new());
        }
    }

    assert_eq!(vehicle_numbers.len(), table.vehicles.len());
    assert_eq!(my_same_counts.len(), table.vehicles.len());
    assert_eq!(my_same_rides.len(), table.vehicles.len());
    assert_eq!(my_coupled_counts.len(), table.vehicles.len());
    assert_eq!(my_coupled_rides.len(), table.vehicles.len());
    assert_eq!(other_same_counts.len(), table.vehicles.len());
    assert_eq!(other_same_names.len(), table.vehicles.len());
    assert_eq!(other_same_rides.len(), table.vehicles.len());
    assert_eq!(other_coupled_counts.len(), table.vehicles.len());
    assert_eq!(other_coupled_names.len(), table.vehicles.len());
    assert_eq!(other_coupled_rides.len(), table.vehicles.len());

    // calculate table widths
    let vehicle_number_width = calculate_width(
        vehicle_numbers
            .iter()
            .chain(once(&vehicle_heading))
    );
    let same_coupled_width = calculate_width(
        once(&same_tag)
            .chain(once(&coupled_tag))
    );
    let my_count_width = calculate_width(
        my_same_counts
            .iter()
            .chain(&my_coupled_counts)
    );
    let my_ride_width = calculate_width(
        my_same_rides
            .iter()
            .chain(&my_coupled_rides)
    );
    let other_count_width = calculate_width(
        other_same_counts
            .iter()
            .chain(&other_coupled_counts)
    );
    let other_name_width = calculate_width(
        other_same_names
            .iter()
            .chain(&other_coupled_names)
    );
    let other_ride_width = calculate_width(
        other_same_rides
            .iter()
            .chain(&other_coupled_rides)
    );
    let rider_columns_width = calculate_width(once(&rider_username_heading))
        .max(my_count_width + my_ride_width);
    let other_columns_width = calculate_width(once(&other_heading))
        .max(other_count_width + other_name_width + other_ride_width);
    let full_table_width =
        HORIZONTAL_MARGIN
        + vehicle_number_width + COLUMN_SPACING
        + same_coupled_width + COLUMN_SPACING
        + rider_columns_width + COLUMN_SPACING
        + other_columns_width
        + HORIZONTAL_MARGIN;

    // and now, placement
    let mut canvas = HashMap::new();
    let mut x_cursor = HORIZONTAL_MARGIN;
    let mut y_cursor = 0;

    // ride ID:
    place_on_canvas(&mut canvas, &ride_text, x_cursor, y_cursor);

    draw_line(&mut canvas, &mut y_cursor, line_height, full_table_width);

    x_cursor = HORIZONTAL_MARGIN;

    // headings
    place_on_canvas(&mut canvas, &vehicle_heading, x_cursor, y_cursor);
    x_cursor += vehicle_number_width + COLUMN_SPACING;
    x_cursor += same_coupled_width + COLUMN_SPACING;
    place_on_canvas(&mut canvas, &rider_username_heading, x_cursor, y_cursor);
    x_cursor += rider_columns_width + COLUMN_SPACING;
    place_on_canvas(&mut canvas, &other_heading, x_cursor, y_cursor);

    for i in 0..table.vehicles.len() {
        draw_line(&mut canvas, &mut y_cursor, line_height, full_table_width);

        x_cursor = HORIZONTAL_MARGIN;

        // "same" row
        place_on_canvas(&mut canvas, &vehicle_numbers[i], x_cursor, y_cursor);
        x_cursor += vehicle_number_width + COLUMN_SPACING;
        place_on_canvas(&mut canvas, &same_tag, x_cursor, y_cursor);
        x_cursor += same_coupled_width + COLUMN_SPACING;
        // calculation for right-alignment:
        let this_my_count_width = calculate_width(once(&my_same_counts[i]));
        place_on_canvas(&mut canvas, &my_same_counts[i], x_cursor + my_count_width - this_my_count_width, y_cursor);
        x_cursor += my_count_width;
        place_on_canvas(&mut canvas, &my_same_rides[i], x_cursor, y_cursor);
        x_cursor += my_ride_width + COLUMN_SPACING;
        // calculation for right-alignment:
        let this_other_count_width = calculate_width(once(&other_same_counts[i]));
        place_on_canvas(&mut canvas, &other_same_counts[i], x_cursor + other_count_width - this_other_count_width, y_cursor);
        x_cursor += other_count_width;
        place_on_canvas(&mut canvas, &other_same_names[i], x_cursor, y_cursor);
        x_cursor += other_name_width;
        place_on_canvas(&mut canvas, &other_same_rides[i], x_cursor, y_cursor);

        y_cursor += line_height;
        x_cursor = HORIZONTAL_MARGIN;

        // "other" row
        // no vehicle number here
        x_cursor += vehicle_number_width + COLUMN_SPACING;
        place_on_canvas(&mut canvas, &coupled_tag, x_cursor, y_cursor);
        x_cursor += same_coupled_width + COLUMN_SPACING;
        // calculation for right-alignment:
        let this_my_count_width = calculate_width(once(&my_coupled_counts[i]));
        place_on_canvas(&mut canvas, &my_coupled_counts[i], x_cursor + my_count_width - this_my_count_width, y_cursor);
        x_cursor += my_count_width;
        place_on_canvas(&mut canvas, &my_coupled_rides[i], x_cursor, y_cursor);
        x_cursor += my_ride_width + COLUMN_SPACING;
        // calculation for right-alignment:
        let this_other_count_width = calculate_width(once(&other_coupled_counts[i]));
        place_on_canvas(&mut canvas, &other_coupled_counts[i], x_cursor + other_count_width - this_other_count_width, y_cursor);
        x_cursor += other_count_width;
        place_on_canvas(&mut canvas, &other_coupled_names[i], x_cursor, y_cursor);
        x_cursor += other_name_width;
        place_on_canvas(&mut canvas, &other_coupled_rides[i], x_cursor, y_cursor);
    }

    canvas
}
