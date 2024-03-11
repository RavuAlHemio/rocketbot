//! Table image rendering for bim rides.
//!
//! Generally, the following structure is rendered:
//!
//! ```plain
//! ┌─────────┬─────────┬───────────────────────────┬───────────────────────────────────┬─────┐
//! │ vehicle │         │ ravu.al.hemio             │ other                             | Σ   |
//! ├─────────┼─────────┼───────────────────────────┼───────────────────────────────────┼─────┤
//! │ 4096    │ same    │  1x (09:16/25)            │  1x (paulchen 24.02.2023 22:03/5) |  2x |
//! │         │ coupled │ 12x (25.02.2023 22:03/25) │  9x (Steve    23.02.2023 22:00/5) | 21x |
//! │         │ Σ       │ 13x                       │ 10x                               | 23x |
//! ├─────────┼─────────┼───────────────────────────┼───────────────────────────────────┼─────┤
//! │ 1496    │ same    │  1x (09:16/25)            │  1x (paulchen 24.02.2023 22:03/5) |  2x |
//! │         │ coupled │ 12x (25.02.2023 22:03/25) │ 12x (Steve    23.02.2023 22:00/5) | 24x |
//! │         │ Σ       │ 13x                       │ 13x                               | 26x |
//! └─────────┴─────────┴───────────────────────────┴───────────────────────────────────┴─────┘
//! ```
//!
//! The headers and data are actually arranged in the following columns for improved visual
//! alignment:
//!
//! ```plain
//!   ╭──0──╮   ╭──1──╮   ╭──────────2+3───────────╮   ╭─────────────4+5+6──────────────╮   ╭7─╮
//! │ vehicle │         │ ravu.al.hemio              │ other                              | Σ    |
//! │ 4096    │ same    │ 421x (09:16/25)            │ 421x (paulchen 24.02.2023 22:03/5) | 842x |
//!   ╰──0──╯   ╰──1──╯   ╰2r╯╰─────────3──────────╯   ╰4r╯╰───5────╯╰────────6─────────╯   ╰7r╯
//! ```
//!
//! (Columns marked with `r` are right-aligned, the others are left-aligned.)


use std::collections::HashMap;
use std::fmt::Display;
use std::iter::once;

use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use rocketbot_bim_common::{CouplingMode, LastRider};
use rocketbot_render_text::{
    DEFAULT_FONT_DATA, DEFAULT_ITALIC_FONT_DATA, DEFAULT_SIZE_PX, map_to_dimensions, TextRenderer,
};


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RideTableData {
    /// The number of the ride.
    pub ride_id: i64,

    /// The line on which the vehicles have been ridden.
    pub line: Option<String>,

    /// The name of the rider. Used as a column header.
    pub rider_username: String,

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

    /// Returns the timestamp of this ride. Provided for structural equivalence with [`UserRide`].
    #[inline]
    pub fn timestamp(&self) -> &DateTime<Local> { &self.timestamp }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct UserRide {
    /// The username of the rider who rode this vehicle.
    pub rider_username: String,

    /// All the other ride data.
    pub ride: Ride,
}
impl UserRide {
    /// Returns the timestamp of this ride. Provided for structural equivalence with [`Ride`].
    #[inline]
    pub fn timestamp(&self) -> &DateTime<Local> { &self.ride.timestamp }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct HighlightedRide {
    pub timestamp: DateTime<Local>,
    pub is_other: bool,
    pub is_coupled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RideTableVehicle {
    /// The number of this vehicle.
    pub vehicle_number: String,

    /// The type of this vehicle, if known.
    pub vehicle_type: Option<String>,

    /// The number of times this vehicle (exactly) was ridden by the rider registering the ride.
    #[serde(default)]
    pub my_same_count: i64,

    /// The last time this vehicle (exactly) was ridden by the rider registering the ride.
    pub my_same_last: Option<Ride>,

    /// The number of times a vehicle coupled with this one was ridden by the rider registering the
    /// ride.
    #[serde(default)]
    pub my_coupled_count: i64,

    /// The timestamp and line of the last time a vehicle coupled with this one was ridden by the
    /// rider registering the ride.
    pub my_coupled_last: Option<Ride>,

    /// The number of times this vehicle (exactly) was ridden by a different rider.
    #[serde(default)]
    pub other_same_count: i64,

    /// The rider's username and timestamp of the last time this vehicle (exactly) was ridden by a
    /// different rider.
    pub other_same_last: Option<UserRide>,

    /// The number of times a vehicle coupled with this one was ridden by the rider registering the
    /// ride.
    #[serde(default)]
    pub other_coupled_count: i64,

    /// The rider's username and timestamp of the last time a vehicle coupled with this one was
    /// ridden by a different rider.
    pub other_coupled_last: Option<UserRide>,

    /// Whether to highlight coupled rides if they are the latest.
    ///
    /// If false, a coupled ride is not highlighted even if it is the latest.
    pub highlight_coupled_rides: bool,

    /// The coupling mode of this vehicle.
    pub coupling_mode: CouplingMode,
}

macro_rules! define_is_most_recent {
    ($name:ident, $expect_coupled:expr, $expect_other:expr) => {
        #[inline]
        pub fn $name(&self) -> bool {
            let highlighted_rides = self.highlighted_rides_most_recent_first();
            highlighted_rides.get(0)
                .map(|r| r.is_coupled == $expect_coupled && r.is_other == $expect_other)
                .unwrap_or(false)
        }
    };
}

impl RideTableVehicle {
    /// Returns all the rides worth highlighting.
    ///
    /// A ride is considered worth highlighting if it is a same-vehicle ride, or if it is a
    /// coupled-vehicle ride and `highlight_coupled_rides` is true.
    ///
    /// The rides are returned sorted in descending order by timestamp, i.e. latest first.
    fn highlighted_rides_most_recent_first(&self) -> SmallVec<[HighlightedRide; 4]> {
        let mut ret = SmallVec::new();
        if let Some(my_same_last) = self.my_same_last.as_ref() {
            ret.push(HighlightedRide {
                timestamp: my_same_last.timestamp,
                is_coupled: false,
                is_other: false,
            });
        }
        if let Some(other_same_last) = self.other_same_last.as_ref() {
            ret.push(HighlightedRide {
                timestamp: other_same_last.ride.timestamp,
                is_coupled: false,
                is_other: true,
            });
        }
        if self.highlight_coupled_rides {
            if let Some(my_coupled_last) = self.my_coupled_last.as_ref() {
                ret.push(HighlightedRide {
                    timestamp: my_coupled_last.timestamp,
                    is_coupled: true,
                    is_other: false,
                });
            }
            if let Some(other_coupled_last) = self.other_coupled_last.as_ref() {
                ret.push(HighlightedRide {
                    timestamp: other_coupled_last.ride.timestamp,
                    is_coupled: true,
                    is_other: true,
                });
            }
        }
        ret.sort_unstable();
        ret.reverse();
        ret
    }

    define_is_most_recent!(is_my_same_most_recent, false, false);
    define_is_most_recent!(is_my_coupled_most_recent, false, true);
    define_is_most_recent!(is_other_same_most_recent, true, false);
    define_is_most_recent!(is_other_coupled_most_recent, true, true);

    /// Whether this vehicle has, in terms of highlighting, never been ridden before.
    ///
    /// See the documentation `highlighted_rides_most_recent_first` for an explanation of
    /// highlighting rules.
    #[inline]
    pub fn is_first_highlighted_ride_overall(&self) -> bool {
        self.highlighted_rides_most_recent_first().len() == 0
    }

    /// Whether this vehicle has previously belonged to the same rider in terms of highlighting.
    ///
    /// See the documentation `highlighted_rides_most_recent_first` for an explanation of
    /// highlighting rules.
    #[inline]
    pub fn belongs_to_rider_highlighted(&self) -> bool {
        self.highlighted_rides_most_recent_first().get(0)
            .map(|hr| !hr.is_other)
            .unwrap_or(false)
    }

    /// Whether this vehicle has changed hands in terms of highlighting.
    ///
    /// See the documentation `highlighted_rides_most_recent_first` for an explanation of
    /// highlighting rules.
    #[inline]
    pub fn has_changed_hands_highlighted(&self) -> bool {
        self.highlighted_rides_most_recent_first().get(0)
            .map(|hr| hr.is_other)
            .unwrap_or(false)
    }

    /// Returns the last highlighted rider of this vehicle.
    pub fn last_highlighted_rider(&self) -> LastRider<'_> {
        let last_rides_by_category = self.highlighted_rides_most_recent_first();

        if let Some(last_highlighted_ride) = last_rides_by_category.get(0) {
            if last_highlighted_ride.is_other {
                if last_highlighted_ride.is_coupled {
                    LastRider::SomebodyElse(self.other_coupled_last.as_ref().unwrap().rider_username.as_str())
                } else {
                    LastRider::SomebodyElse(self.other_same_last.as_ref().unwrap().rider_username.as_str())
                }
            } else {
                LastRider::Me
            }
        } else {
            LastRider::Nobody
        }
    }

    /// The latest highlighted ride of this vehicle by the rider registering the ride, if any.
    ///
    /// If `highlight_coupled_rides` is true, picks the later ride of `my_same_last` and
    /// `my_coupled_last`. If `highlight_coupled_rides` is false, returns `my_same_last`.
    pub fn my_highlighted_last(&self) -> Option<&Ride> {
        if self.highlight_coupled_rides {
            match (self.my_same_last.as_ref(), self.my_coupled_last.as_ref()) {
                (Some(s), Some(c)) => Some(if c.timestamp() > s.timestamp() { c } else { s }),
                (Some(s), None) => Some(s),
                (None, Some(c)) => Some(c),
                (None, None) => None,
            }
        } else {
            self.my_same_last.as_ref()
        }
    }
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

#[allow(unused_assignments)] // improve consistency, simplify extensibility
pub fn draw_ride_table(
    table: &RideTableData,
) -> HashMap<(u32, u32), u8> {
    let renderer = TextRenderer::new(DEFAULT_FONT_DATA, DEFAULT_SIZE_PX)
        .expect("failed to load default font");
    let italic_renderer = TextRenderer::new(DEFAULT_ITALIC_FONT_DATA, DEFAULT_SIZE_PX)
        .expect("failed to load italic font");

    let line_height = renderer.font_line_height();
    const HORIZONTAL_MARGIN: u32 = 8;
    const COLUMN_SPACING: u32 = 16;

    // render each required piece
    let ride_text = if let Some(line) = &table.line {
        renderer.render_text(&format!("ride {} (line {}):", table.ride_id, line))
    } else {
        renderer.render_text(&format!("ride {}:", table.ride_id))
    };
    let vehicle_heading = renderer.render_text("vehicle");
    let rider_username_heading = renderer.render_text(&table.rider_username);
    let other_heading = renderer.render_text("other");
    let same_tag = renderer.render_text("same");
    let coupled_tag = renderer.render_text("coupled");
    let sum_heading_and_tag = renderer.render_text("\u{3A3}");

    let mut vehicle_numbers = Vec::with_capacity(table.vehicles.len());
    let mut vehicle_types = Vec::with_capacity(table.vehicles.len());
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
    let mut same_sums = Vec::with_capacity(table.vehicles.len());
    let mut coupled_sums = Vec::with_capacity(table.vehicles.len());
    let mut my_sums = Vec::with_capacity(table.vehicles.len());
    let mut other_sums = Vec::with_capacity(table.vehicles.len());
    let mut total_sums = Vec::with_capacity(table.vehicles.len());
    let mut vehicle_has_coupled = Vec::with_capacity(table.vehicles.len());

    for vehicle in &table.vehicles {
        if vehicle.coupling_mode == CouplingMode::FixedCoupling {
            continue;
        }

        vehicle_numbers.push(renderer.render_text(&vehicle.vehicle_number));

        if let Some(vt) = &vehicle.vehicle_type {
            vehicle_types.push(renderer.render_text(&format!("({})", vt)));
        } else {
            vehicle_types.push(HashMap::new());
        }

        my_same_counts.push(renderer.render_text(&format!("{}\u{D7}", vehicle.my_same_count)));
        if let Some(my_same_last) = &vehicle.my_same_last {
            let render_me = format!(" ({})", my_same_last.stringify(table.relative_time));
            let rendered = if vehicle.is_my_same_most_recent() {
                italic_renderer.render_text(&render_me)
            } else {
                renderer.render_text(&render_me)
            };
            my_same_rides.push(rendered);
        } else {
            my_same_rides.push(HashMap::new());
        }

        my_coupled_counts.push(renderer.render_text(&format!("{}\u{D7}", vehicle.my_coupled_count)));
        if let Some(my_coupled_last) = &vehicle.my_coupled_last {
            let render_me = format!(" ({})", my_coupled_last.stringify(table.relative_time));
            let rendered = if vehicle.is_my_coupled_most_recent() {
                italic_renderer.render_text(&render_me)
            } else {
                renderer.render_text(&render_me)
            };
            my_coupled_rides.push(rendered);
        } else {
            my_coupled_rides.push(HashMap::new());
        }

        other_same_counts.push(renderer.render_text(&format!("{}\u{D7}", vehicle.other_same_count)));
        if let Some(other_same_last) = &vehicle.other_same_last {
            let render_me_name = format!(" ({}", other_same_last.rider_username);
            let render_me_ride = format!(" {})", other_same_last.ride.stringify(table.relative_time));
            let (rendered_name, rendered_ride) = if vehicle.is_other_same_most_recent() {
                (
                    italic_renderer.render_text(&render_me_name),
                    italic_renderer.render_text(&render_me_ride),
                )
            } else {
                (
                    renderer.render_text(&render_me_name),
                    renderer.render_text(&render_me_ride),
                )
            };
            other_same_names.push(rendered_name);
            other_same_rides.push(rendered_ride);
        } else {
            other_same_names.push(HashMap::new());
            other_same_rides.push(HashMap::new());
        }

        other_coupled_counts.push(renderer.render_text(&format!("{}\u{D7}", vehicle.other_coupled_count)));
        if let Some(other_coupled_last) = &vehicle.other_coupled_last {
            let render_me_name = format!(" ({}", other_coupled_last.rider_username);
            let render_me_ride = format!(" {})", other_coupled_last.ride.stringify(table.relative_time));
            let (rendered_name, rendered_ride) = if vehicle.is_other_coupled_most_recent() {
                (
                    italic_renderer.render_text(&render_me_name),
                    italic_renderer.render_text(&render_me_ride),
                )
            } else {
                (
                    renderer.render_text(&render_me_name),
                    renderer.render_text(&render_me_ride),
                )
            };
            other_coupled_names.push(rendered_name);
            other_coupled_rides.push(rendered_ride);
        } else {
            other_coupled_names.push(HashMap::new());
            other_coupled_rides.push(HashMap::new());
        }

        same_sums.push(renderer.render_text(&format!("{}\u{D7}", vehicle.my_same_count + vehicle.other_same_count)));
        coupled_sums.push(renderer.render_text(&format!("{}\u{D7}", vehicle.my_coupled_count + vehicle.other_coupled_count)));
        my_sums.push(renderer.render_text(&format!("{}\u{D7}", vehicle.my_same_count + vehicle.my_coupled_count)));
        other_sums.push(renderer.render_text(&format!("{}\u{D7}", vehicle.other_same_count + vehicle.other_coupled_count)));
        total_sums.push(renderer.render_text(&format!(
            "{}\u{D7}",
            vehicle.my_same_count + vehicle.my_coupled_count + vehicle.other_same_count + vehicle.other_coupled_count,
        )));

        let has_coupled =
            vehicle.my_coupled_count > 0
            || vehicle.my_coupled_last.is_some()
            || vehicle.other_coupled_count > 0
            || vehicle.other_coupled_last.is_some()
        ;
        vehicle_has_coupled.push(has_coupled);
    }

    assert_eq!(vehicle_types.len(), vehicle_numbers.len());
    assert_eq!(my_same_counts.len(), vehicle_numbers.len());
    assert_eq!(my_same_rides.len(), vehicle_numbers.len());
    assert_eq!(my_coupled_counts.len(), vehicle_numbers.len());
    assert_eq!(my_coupled_rides.len(), vehicle_numbers.len());
    assert_eq!(other_same_counts.len(), vehicle_numbers.len());
    assert_eq!(other_same_names.len(), vehicle_numbers.len());
    assert_eq!(other_same_rides.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_counts.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_names.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_rides.len(), vehicle_numbers.len());
    assert_eq!(same_sums.len(), vehicle_numbers.len());
    assert_eq!(coupled_sums.len(), vehicle_numbers.len());
    assert_eq!(my_sums.len(), vehicle_numbers.len());
    assert_eq!(other_sums.len(), vehicle_numbers.len());
    assert_eq!(total_sums.len(), vehicle_numbers.len());

    // calculate table widths
    let vehicle_number_width = calculate_width(
        vehicle_numbers.iter()
            .chain(&vehicle_types)
            .chain(once(&vehicle_heading))
    );
    let same_coupled_width = calculate_width(
        once(&same_tag)
            .chain(once(&coupled_tag))
            .chain(once(&sum_heading_and_tag))
    );
    let my_count_width = calculate_width(
        my_same_counts.iter()
            .chain(&my_coupled_counts)
            .chain(&my_sums)
    );
    let my_ride_width = calculate_width(
        my_same_rides.iter()
            .chain(&my_coupled_rides)
    );
    let other_count_width = calculate_width(
        other_same_counts.iter()
            .chain(&other_coupled_counts)
    );
    let other_name_width = calculate_width(
        other_same_names
            .iter()
            .chain(&other_coupled_names)
            .chain(&other_sums)
    );
    let other_ride_width = calculate_width(
        other_same_rides
            .iter()
            .chain(&other_coupled_rides)
    );
    let sum_column_width = calculate_width(
        once(&sum_heading_and_tag)
            .chain(&same_sums)
            .chain(&coupled_sums)
            .chain(&total_sums)
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
        + other_columns_width + COLUMN_SPACING
        + sum_column_width
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
    x_cursor += other_columns_width + COLUMN_SPACING;
    place_on_canvas(&mut canvas, &sum_heading_and_tag, x_cursor, y_cursor);

    for i in 0..vehicle_numbers.len() {
        draw_line(&mut canvas, &mut y_cursor, line_height, full_table_width);

        x_cursor = HORIZONTAL_MARGIN;

        // "same" row
        place_on_canvas(&mut canvas, &vehicle_numbers[i], x_cursor, y_cursor);
        x_cursor += vehicle_number_width + COLUMN_SPACING;
        place_on_canvas(&mut canvas, &same_tag, x_cursor, y_cursor);
        x_cursor += same_coupled_width + COLUMN_SPACING;
        {
            let mut sub_x_cursor = x_cursor;

            // calculation for right-alignment:
            let this_my_count_width = calculate_width(once(&my_same_counts[i]));
            place_on_canvas(&mut canvas, &my_same_counts[i], sub_x_cursor + my_count_width - this_my_count_width, y_cursor);
            sub_x_cursor += my_count_width;
            place_on_canvas(&mut canvas, &my_same_rides[i], sub_x_cursor, y_cursor);
            sub_x_cursor += my_ride_width;

            x_cursor += rider_columns_width + COLUMN_SPACING;
        }
        {
            let mut sub_x_cursor = x_cursor;

            // calculation for right-alignment:
            let this_other_count_width = calculate_width(once(&other_same_counts[i]));
            place_on_canvas(&mut canvas, &other_same_counts[i], sub_x_cursor + other_count_width - this_other_count_width, y_cursor);
            sub_x_cursor += other_count_width;
            place_on_canvas(&mut canvas, &other_same_names[i], sub_x_cursor, y_cursor);
            sub_x_cursor += other_name_width;
            place_on_canvas(&mut canvas, &other_same_rides[i], sub_x_cursor, y_cursor);
            sub_x_cursor += other_ride_width;

            x_cursor += other_columns_width + COLUMN_SPACING;
        }
        // calculation for right-alignment:
        let this_sum_width = calculate_width(once(&same_sums[i]));
        place_on_canvas(&mut canvas, &same_sums[i], x_cursor + sum_column_width - this_sum_width, y_cursor);

        if vehicle_has_coupled[i] {
            y_cursor += line_height;
            x_cursor = HORIZONTAL_MARGIN;

            // "coupled" row
            // vehicle type instead of vehicle number
            place_on_canvas(&mut canvas, &vehicle_types[i], x_cursor, y_cursor);
            x_cursor += vehicle_number_width + COLUMN_SPACING;
            place_on_canvas(&mut canvas, &coupled_tag, x_cursor, y_cursor);
            x_cursor += same_coupled_width + COLUMN_SPACING;
            {
                let mut sub_x_cursor = x_cursor;

                // calculation for right-alignment:
                let this_my_count_width = calculate_width(once(&my_coupled_counts[i]));
                place_on_canvas(&mut canvas, &my_coupled_counts[i], sub_x_cursor + my_count_width - this_my_count_width, y_cursor);
                sub_x_cursor += my_count_width;
                place_on_canvas(&mut canvas, &my_coupled_rides[i], sub_x_cursor, y_cursor);
                sub_x_cursor += my_ride_width;

                x_cursor += rider_columns_width + COLUMN_SPACING;
            }
            {
                let mut sub_x_cursor = x_cursor;

                // calculation for right-alignment:
                let this_other_count_width = calculate_width(once(&other_coupled_counts[i]));
                place_on_canvas(&mut canvas, &other_coupled_counts[i], sub_x_cursor + other_count_width - this_other_count_width, y_cursor);
                sub_x_cursor += other_count_width;
                place_on_canvas(&mut canvas, &other_coupled_names[i], sub_x_cursor, y_cursor);
                sub_x_cursor += other_name_width;
                place_on_canvas(&mut canvas, &other_coupled_rides[i], sub_x_cursor, y_cursor);
                sub_x_cursor += other_ride_width;

                x_cursor += other_columns_width + COLUMN_SPACING;
            }
            // calculation for right-alignment:
            let this_sum_width = calculate_width(once(&coupled_sums[i]));
            place_on_canvas(&mut canvas, &coupled_sums[i], x_cursor + sum_column_width - this_sum_width, y_cursor);

            y_cursor += line_height;
            x_cursor = HORIZONTAL_MARGIN;

            // sum row
            // no vehicle number here
            x_cursor += vehicle_number_width + COLUMN_SPACING;
            place_on_canvas(&mut canvas, &sum_heading_and_tag, x_cursor, y_cursor);
            x_cursor += same_coupled_width + COLUMN_SPACING;
            // calculation for right-alignment:
            let this_my_count_width = calculate_width(once(&my_sums[i]));
            place_on_canvas(&mut canvas, &my_sums[i], x_cursor + my_count_width - this_my_count_width, y_cursor);
            // skip over all the rider columns
            x_cursor += rider_columns_width + COLUMN_SPACING;
            // calculation for right-alignment:
            let this_other_count_width = calculate_width(once(&other_sums[i]));
            place_on_canvas(&mut canvas, &other_sums[i], x_cursor + other_count_width - this_other_count_width, y_cursor);
            // skip over all the other-rider columns
            x_cursor += other_columns_width + COLUMN_SPACING;
            // calculation for right-alignment:
            let this_sum_width = calculate_width(once(&total_sums[i]));
            place_on_canvas(&mut canvas, &total_sums[i], x_cursor + sum_column_width - this_sum_width, y_cursor);
        } else if vehicle_types[i].len() > 0 {
            // add vehicle type below
            y_cursor += line_height;
            x_cursor = HORIZONTAL_MARGIN;

            place_on_canvas(&mut canvas, &vehicle_types[i], x_cursor, y_cursor);
        }
    }

    canvas
}
