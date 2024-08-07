//! Table image rendering for bim rides.
//!
//! Generally, the following structure is rendered:
//!
//! ```plain
//! ┌─────────┬─────────┬──────────────────────────────┬──────────────────────────────────────┬────────┐
//! │ vehicle │         │ ravu.al.hemio                │ other                                | Σ      |
//! ├─────────┼─────────┼──────────────────────────────┼──────────────────────────────────────┼────────┤
//! │ 4096    │ same    │ 1|  2¦ (09:16/25)            │ 1|  2¦ (paulchen 24.02.2023 22:03/5) | 2|  4¦ |
//! │         │ coupled │ 1| 12¦ (25.02.2023 22:03/25) │ 1|  9¦ (Steve    23.02.2023 22:00/5) | 2| 21¦ |
//! │         │ Σ       │ 2| 13¦                       │ 2| 10¦                               | 4| 23¦ |
//! ├─────────┼─────────┼──────────────────────────────┼──────────────────────────────────────┼────────┤
//! │ 1496    │ same    │ 1|  2¦ (09:16/25)            │ 1|  2¦ (paulchen 24.02.2023 22:03/5) | 2|  4¦ |
//! │         │ coupled │ 1| 12¦ (25.02.2023 22:03/25) │ 1|  9¦ (Steve    23.02.2023 22:00/5) | 2| 21¦ |
//! │         │ Σ       │ 2| 13¦                       │ 2| 10¦                               | 4| 23¦ |
//! └─────────┴─────────┴──────────────────────────────┴──────────────────────────────────────┴────────┘
//! ```
//!
//! The headers and data are actually arranged in the following columns for improved visual
//! alignment:
//!
//! ```plain
//!   ╭──0──╮   ╭──1──╮   ╭─────────2+3+4──────────╮   ╭───────────────5+6+7+8───────────────╮   ╭──8+9──╮
//! │ vehicle │         │ ravu.al.hemio              │ other                                   | Σ         |
//! │ 4096    │ same    │ 421| 421¦ (09:16/25)       │ 421| 421¦ (paulchen 24.02.2023 22:03/5) | 842| 842¦ |
//!   ╰──0──╯   ╰──1──╯   ╰2r─╯╰3r╯╰───────4───────╯   ╰5r─╯╰6r╯╰───7─────╯╰────────8────────╯   ╰8r─╯╰9r╯
//! ```
//!
//! (Columns marked with `r` are right-aligned, the others are left-aligned.)


use std::collections::HashMap;
use std::iter::once;

use rocketbot_bim_common::CouplingMode;
use rocketbot_bim_common::ride_table::RideTableData;
use rocketbot_render_text::{
    DEFAULT_FONT_DATA, DEFAULT_ITALIC_FONT_DATA, DEFAULT_SIZE_PX, map_to_dimensions, TextRenderer,
};


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
    let font_space_width = renderer.render_text_with_width(" ").1 as u32;

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
    let mut my_same_streaks = Vec::with_capacity(table.vehicles.len());
    let mut my_same_counts = Vec::with_capacity(table.vehicles.len());
    let mut my_same_rides = Vec::with_capacity(table.vehicles.len());
    let mut my_coupled_streaks = Vec::with_capacity(table.vehicles.len());
    let mut my_coupled_counts = Vec::with_capacity(table.vehicles.len());
    let mut my_coupled_rides = Vec::with_capacity(table.vehicles.len());
    let mut other_same_streaks = Vec::with_capacity(table.vehicles.len());
    let mut other_same_counts = Vec::with_capacity(table.vehicles.len());
    let mut other_same_names = Vec::with_capacity(table.vehicles.len());
    let mut other_same_rides = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_streaks = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_counts = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_names = Vec::with_capacity(table.vehicles.len());
    let mut other_coupled_rides = Vec::with_capacity(table.vehicles.len());
    let mut same_streak_sums = Vec::with_capacity(table.vehicles.len());
    let mut same_sums = Vec::with_capacity(table.vehicles.len());
    let mut coupled_streak_sums = Vec::with_capacity(table.vehicles.len());
    let mut coupled_sums = Vec::with_capacity(table.vehicles.len());
    let mut my_streak_sums = Vec::with_capacity(table.vehicles.len());
    let mut my_sums = Vec::with_capacity(table.vehicles.len());
    let mut other_streak_sums = Vec::with_capacity(table.vehicles.len());
    let mut other_sums = Vec::with_capacity(table.vehicles.len());
    let mut total_streak_sums = Vec::with_capacity(table.vehicles.len());
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

        my_same_streaks.push(renderer.render_text(&format!("{}|", vehicle.my_same_count_streak)));
        my_same_counts.push(renderer.render_text(&format!("{}\u{A6}", vehicle.my_same_count)));
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

        my_coupled_streaks.push(renderer.render_text(&format!("{}|", vehicle.my_coupled_count_streak)));
        my_coupled_counts.push(renderer.render_text(&format!("{}\u{A6}", vehicle.my_coupled_count)));
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

        other_same_streaks.push(renderer.render_text(&format!("{}|", vehicle.other_same_count_streak)));
        other_same_counts.push(renderer.render_text(&format!("{}\u{A6}", vehicle.other_same_count)));
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

        other_coupled_streaks.push(renderer.render_text(&format!("{}| ", vehicle.other_coupled_count_streak)));
        other_coupled_counts.push(renderer.render_text(&format!("{}\u{A6}", vehicle.other_coupled_count)));
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

        same_streak_sums.push(renderer.render_text(&format!("{}| ", vehicle.my_same_count_streak + vehicle.other_same_count_streak)));
        same_sums.push(renderer.render_text(&format!("{}\u{A6}", vehicle.my_same_count + vehicle.other_same_count)));
        coupled_streak_sums.push(renderer.render_text(&format!("{}| ", vehicle.my_coupled_count_streak + vehicle.other_coupled_count_streak)));
        coupled_sums.push(renderer.render_text(&format!("{}\u{A6}", vehicle.my_coupled_count + vehicle.other_coupled_count)));
        my_streak_sums.push(renderer.render_text(&format!("{}| ", vehicle.my_same_count_streak + vehicle.my_coupled_count_streak)));
        my_sums.push(renderer.render_text(&format!("{}\u{A6}", vehicle.my_same_count + vehicle.my_coupled_count)));
        other_streak_sums.push(renderer.render_text(&format!("{}| ", vehicle.other_same_count_streak + vehicle.other_coupled_count_streak)));
        other_sums.push(renderer.render_text(&format!("{}\u{A6}", vehicle.other_same_count + vehicle.other_coupled_count)));
        total_streak_sums.push(renderer.render_text(&format!(
            "{}| ",
            vehicle.my_same_count_streak + vehicle.my_coupled_count_streak + vehicle.other_same_count_streak + vehicle.other_coupled_count_streak,
        )));
        total_sums.push(renderer.render_text(&format!(
            "{}\u{A6}",
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
    assert_eq!(my_same_streaks.len(), vehicle_numbers.len());
    assert_eq!(my_same_counts.len(), vehicle_numbers.len());
    assert_eq!(my_same_rides.len(), vehicle_numbers.len());
    assert_eq!(my_coupled_streaks.len(), vehicle_numbers.len());
    assert_eq!(my_coupled_counts.len(), vehicle_numbers.len());
    assert_eq!(my_coupled_rides.len(), vehicle_numbers.len());
    assert_eq!(other_same_streaks.len(), vehicle_numbers.len());
    assert_eq!(other_same_counts.len(), vehicle_numbers.len());
    assert_eq!(other_same_names.len(), vehicle_numbers.len());
    assert_eq!(other_same_rides.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_streaks.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_counts.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_names.len(), vehicle_numbers.len());
    assert_eq!(other_coupled_rides.len(), vehicle_numbers.len());
    assert_eq!(same_streak_sums.len(), vehicle_numbers.len());
    assert_eq!(same_sums.len(), vehicle_numbers.len());
    assert_eq!(coupled_streak_sums.len(), vehicle_numbers.len());
    assert_eq!(coupled_sums.len(), vehicle_numbers.len());
    assert_eq!(my_streak_sums.len(), vehicle_numbers.len());
    assert_eq!(my_sums.len(), vehicle_numbers.len());
    assert_eq!(other_streak_sums.len(), vehicle_numbers.len());
    assert_eq!(other_sums.len(), vehicle_numbers.len());
    assert_eq!(total_streak_sums.len(), vehicle_numbers.len());
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
    let my_streak_width = calculate_width(
        my_same_streaks.iter()
            .chain(&my_coupled_streaks)
            .chain(&my_streak_sums)
    );
    let my_count_width = calculate_width(
        my_same_counts.iter()
            .chain(&my_coupled_counts)
            .chain(&my_sums)
    ) + font_space_width;
    let my_ride_width = calculate_width(
        my_same_rides.iter()
            .chain(&my_coupled_rides)
    );
    let other_streak_width = calculate_width(
        other_same_streaks.iter()
            .chain(&other_coupled_streaks)
            .chain(&other_streak_sums)
    );
    let other_count_width = calculate_width(
        other_same_counts.iter()
            .chain(&other_coupled_counts)
            .chain(&other_sums)
    ) + font_space_width;
    let other_name_width = calculate_width(
        other_same_names.iter()
            .chain(&other_coupled_names)
    );
    let other_ride_width = calculate_width(
        other_same_rides.iter()
            .chain(&other_coupled_rides)
    );
    let sum_streak_width = calculate_width(
        same_streak_sums.iter()
            .chain(&coupled_streak_sums)
            .chain(&total_streak_sums)
    );
    let sum_count_width = calculate_width(
        same_sums.iter()
            .chain(&coupled_sums)
            .chain(&total_sums)
    ) + font_space_width;
    let rider_columns_width = calculate_width(once(&rider_username_heading))
        .max(my_streak_width + my_count_width + my_ride_width);
    let other_columns_width = calculate_width(once(&other_heading))
        .max(other_streak_width + other_count_width + other_name_width + other_ride_width);
    let sum_column_width = calculate_width(once(&sum_heading_and_tag))
        .max(sum_streak_width + sum_count_width);
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
            let this_my_streak_width = calculate_width(once(&my_same_streaks[i]));
            let this_my_count_width = calculate_width(once(&my_same_counts[i]));
            place_on_canvas(&mut canvas, &my_same_streaks[i], sub_x_cursor + my_streak_width - this_my_streak_width, y_cursor);
            sub_x_cursor += my_streak_width;
            place_on_canvas(&mut canvas, &my_same_counts[i], sub_x_cursor + my_count_width - this_my_count_width, y_cursor);
            sub_x_cursor += my_count_width;
            place_on_canvas(&mut canvas, &my_same_rides[i], sub_x_cursor, y_cursor);
            sub_x_cursor += my_ride_width;

            x_cursor += rider_columns_width + COLUMN_SPACING;
        }
        {
            let mut sub_x_cursor = x_cursor;

            // calculation for right-alignment:
            let this_other_streak_width = calculate_width(once(&other_same_streaks[i]));
            let this_other_count_width = calculate_width(once(&other_same_counts[i]));
            place_on_canvas(&mut canvas, &other_same_streaks[i], sub_x_cursor + other_streak_width - this_other_streak_width, y_cursor);
            sub_x_cursor += other_streak_width;
            place_on_canvas(&mut canvas, &other_same_counts[i], sub_x_cursor + other_count_width - this_other_count_width, y_cursor);
            sub_x_cursor += other_count_width;
            place_on_canvas(&mut canvas, &other_same_names[i], sub_x_cursor, y_cursor);
            sub_x_cursor += other_name_width;
            place_on_canvas(&mut canvas, &other_same_rides[i], sub_x_cursor, y_cursor);
            sub_x_cursor += other_ride_width;

            x_cursor += other_columns_width + COLUMN_SPACING;
        }
        // calculation for right-alignment:
        let this_streak_sum_width = calculate_width(once(&same_streak_sums[i]));
        let this_sum_width = calculate_width(once(&same_sums[i]));
        place_on_canvas(&mut canvas, &same_streak_sums[i], x_cursor + sum_streak_width - this_streak_sum_width, y_cursor);
        x_cursor += sum_streak_width;
        place_on_canvas(&mut canvas, &same_sums[i], x_cursor + sum_count_width - this_sum_width, y_cursor);

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
                let this_my_streak_width = calculate_width(once(&my_coupled_streaks[i]));
                let this_my_count_width = calculate_width(once(&my_coupled_counts[i]));
                place_on_canvas(&mut canvas, &my_coupled_streaks[i], sub_x_cursor + my_streak_width - this_my_streak_width, y_cursor);
                sub_x_cursor += my_streak_width;
                place_on_canvas(&mut canvas, &my_coupled_counts[i], sub_x_cursor + my_count_width - this_my_count_width, y_cursor);
                sub_x_cursor += my_count_width;
                place_on_canvas(&mut canvas, &my_coupled_rides[i], sub_x_cursor, y_cursor);
                sub_x_cursor += my_ride_width;

                x_cursor += rider_columns_width + COLUMN_SPACING;
            }
            {
                let mut sub_x_cursor = x_cursor;

                // calculation for right-alignment:
                let this_other_streak_width = calculate_width(once(&other_coupled_streaks[i]));
                let this_other_count_width = calculate_width(once(&other_coupled_counts[i]));
                place_on_canvas(&mut canvas, &other_coupled_streaks[i], sub_x_cursor + other_streak_width - this_other_streak_width, y_cursor);
                sub_x_cursor += other_streak_width;
                place_on_canvas(&mut canvas, &other_coupled_counts[i], sub_x_cursor + other_count_width - this_other_count_width, y_cursor);
                sub_x_cursor += other_count_width;
                place_on_canvas(&mut canvas, &other_coupled_names[i], sub_x_cursor, y_cursor);
                sub_x_cursor += other_name_width;
                place_on_canvas(&mut canvas, &other_coupled_rides[i], sub_x_cursor, y_cursor);
                sub_x_cursor += other_ride_width;

                x_cursor += other_columns_width + COLUMN_SPACING;
            }
            // calculation for right-alignment:
            let this_streak_sum_width = calculate_width(once(&coupled_streak_sums[i]));
            let this_sum_width = calculate_width(once(&coupled_sums[i]));
            place_on_canvas(&mut canvas, &coupled_streak_sums[i], x_cursor + sum_streak_width - this_streak_sum_width, y_cursor);
            x_cursor += sum_streak_width;
            place_on_canvas(&mut canvas, &coupled_sums[i], x_cursor + sum_count_width - this_sum_width, y_cursor);

            y_cursor += line_height;
            x_cursor = HORIZONTAL_MARGIN;

            // sum row
            // no vehicle number here
            x_cursor += vehicle_number_width + COLUMN_SPACING;
            place_on_canvas(&mut canvas, &sum_heading_and_tag, x_cursor, y_cursor);
            x_cursor += same_coupled_width + COLUMN_SPACING;
            {
                let mut sub_x_cursor = x_cursor;

                // calculation for right-alignment:
                let this_my_streak_width = calculate_width(once(&my_streak_sums[i]));
                let this_my_count_width = calculate_width(once(&my_sums[i]));
                place_on_canvas(&mut canvas, &my_streak_sums[i], sub_x_cursor + my_streak_width - this_my_streak_width, y_cursor);
                sub_x_cursor += my_streak_width;
                place_on_canvas(&mut canvas, &my_sums[i], sub_x_cursor + my_count_width - this_my_count_width, y_cursor);

                x_cursor += rider_columns_width + COLUMN_SPACING;
            }
            {
                let mut sub_x_cursor = x_cursor;

                // calculation for right-alignment:
                let this_other_streak_width = calculate_width(once(&other_streak_sums[i]));
                let this_other_count_width = calculate_width(once(&other_sums[i]));
                place_on_canvas(&mut canvas, &other_streak_sums[i], sub_x_cursor + other_streak_width - this_other_streak_width, y_cursor);
                sub_x_cursor += other_streak_width;
                place_on_canvas(&mut canvas, &other_sums[i], sub_x_cursor + other_count_width - this_other_count_width, y_cursor);

                x_cursor += other_columns_width + COLUMN_SPACING;
            }
            // calculation for right-alignment:
            let this_streak_sum_width = calculate_width(once(&total_streak_sums[i]));
            let this_sum_width = calculate_width(once(&total_sums[i]));
            place_on_canvas(&mut canvas, &total_streak_sums[i], x_cursor + sum_streak_width - this_streak_sum_width, y_cursor);
            x_cursor += sum_streak_width;
            place_on_canvas(&mut canvas, &total_sums[i], x_cursor + sum_count_width - this_sum_width, y_cursor);
        } else if vehicle_types[i].len() > 0 {
            // add vehicle type below
            y_cursor += line_height;
            x_cursor = HORIZONTAL_MARGIN;

            place_on_canvas(&mut canvas, &vehicle_types[i], x_cursor, y_cursor);
        }
    }

    canvas
}
