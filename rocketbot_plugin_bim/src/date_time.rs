use std::fmt;

use chrono::{Datelike, DateTime, TimeZone, Weekday};


#[inline]
pub fn weekday_abbr2(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Mo",
        Weekday::Tue => "Tu",
        Weekday::Wed => "We",
        Weekday::Thu => "Th",
        Weekday::Fri => "Fr",
        Weekday::Sat => "Sa",
        Weekday::Sun => "Su",
    }
}

pub fn canonical_date_format<W: fmt::Write, Tz: TimeZone>(mut writer: W, date_time: &DateTime<Tz>, on_at: bool, seconds: bool) -> fmt::Result
        where Tz::Offset: fmt::Display {
    let dow = weekday_abbr2(date_time.weekday());
    let date_formatted = date_time.format("%Y-%m-%d");
    let time_formatted = if seconds {
        date_time.format("%H:%M:%S")
    } else {
        date_time.format("%H:%M")
    };
    if on_at {
        write!(writer, "on {} {} at {}", dow, date_formatted, time_formatted)
    } else {
        write!(writer, "{} {} {}", dow, date_formatted, time_formatted)
    }
}

pub fn canonical_date_format_relative<W: fmt::Write, Tz: TimeZone, Tz2: TimeZone>(mut writer: W, date_time: &DateTime<Tz>, relative_to: &DateTime<Tz2>, on_at: bool, seconds: bool) -> fmt::Result
        where Tz::Offset: fmt::Display, Tz2::Offset: fmt::Display {
    let night_owl_date = crate::get_night_owl_date(date_time);
    let night_owl_relative = crate::get_night_owl_date(relative_to);
    if night_owl_date == night_owl_relative {
        // only output time
        let time_formatted = if seconds {
            date_time.format("%H:%M:%S")
        } else {
            date_time.format("%H:%M")
        };
        if on_at {
            write!(writer, "at {}", time_formatted)
        } else {
            write!(writer, "{}", time_formatted)
        }
    } else {
        canonical_date_format(writer, date_time, on_at, seconds)
    }
}
