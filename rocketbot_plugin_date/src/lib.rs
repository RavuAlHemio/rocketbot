use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Local, NaiveDate, Weekday};
use julian;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


const DATE_OUTPUT_FORMAT: &'static str = "%Y-%m-%d";
static DATE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "(?:",
        "(?P<ymdy>[0-9]{1,4})-(?P<ymdm>[0-9]{1,2})-(?P<ymdd>[0-9]{1,2})",
        "|",
        "(?P<dmyd>[0-9]{1,2})\\.(?P<dmym>[0-9]{1,2})\\.(?P<dmyy>[0-9]{1,4})",
        "|",
        "(?P<mdym>[0-9]{1,2})/(?P<mdyd>[0-9]{1,2})/(?P<mdyy>[0-9]{1,4})",
    ")",
    "$",
)).expect("failed to parse regex"));


/// Calculates the Gregorian date of Easter Sunday for the given year.
///
/// The result is returned as a month and day-of-month according to the Gregorian calendar.
fn gregorian_computus(year: i32) -> julian::Date {
    // Meeus/Jones/Butcher
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2*e + 2*i - h - k) % 7;
    let m = (a + 11*h + 22*l) / 451;
    let n = (h + l - 7*m + 114) / 31;
    let o = (h + l - 7*m + 114) % 31;

    let month = match n {
        1 => julian::Month::January,
        2 => julian::Month::February,
        3 => julian::Month::March,
        4 => julian::Month::April,
        5 => julian::Month::May,
        6 => julian::Month::June,
        7 => julian::Month::July,
        8 => julian::Month::August,
        9 => julian::Month::September,
        10 => julian::Month::October,
        11 => julian::Month::November,
        12 => julian::Month::December,
        _ => panic!("unexpected month {}", n),
    };
    let day = o + 1;
    julian::Calendar::GREGORIAN.at_ymd(
        year,
        month,
        day.try_into().expect("negative day-of-month?!"),
    ).expect("computed invalid Gregorian date")
}

/// Calculates the Julian date of Easter Sunday for the given year.
///
/// The result is returned as a month and day-of-month according to the Julian calendar. A
/// conversion to the Gregorian calendar is necessary to be able to perform anything useful with
/// the date.
fn julian_computus(year: i32) -> julian::Date {
    // Meeus
    let a = year % 4;
    let b = year % 7;
    let c = year % 19;
    let d = (19*c + 15) % 30;
    let e = (2*a + 4*b - d + 34) % 7;
    let f = d + e + 114;
    let g = f / 31;

    let month = match g {
        1 => julian::Month::January,
        2 => julian::Month::February,
        3 => julian::Month::March,
        4 => julian::Month::April,
        5 => julian::Month::May,
        6 => julian::Month::June,
        7 => julian::Month::July,
        8 => julian::Month::August,
        9 => julian::Month::September,
        10 => julian::Month::October,
        11 => julian::Month::November,
        12 => julian::Month::December,
        _ => panic!("unexpected month {}", g),
    };
    let day = (f % 31) + 1;
    julian::Calendar::JULIAN.at_ymd(
        year,
        month,
        day.try_into().expect("negative day-of-month?!"),
    ).expect("computed invalid Julian date")
}


pub struct DatePlugin {
    interface: Weak<dyn RocketBotInterface>,
}
impl DatePlugin {
    fn parse_date(date_str: &str) -> Option<NaiveDate> {
        let caps = DATE_REGEX.captures(date_str)?;

        let (year_match, month_match, day_match) = if let Some(year_match) = caps.name("ymdy") {
            let month_match = caps.name("ymdm").unwrap();
            let day_match = caps.name("ymdd").unwrap();
            (year_match, month_match, day_match)
        } else if let Some(day_match) = caps.name("dmyd") {
            let month_match = caps.name("dmym").unwrap();
            let year_match = caps.name("dmyy").unwrap();
            (year_match, month_match, day_match)
        } else if let Some(month_match) = caps.name("mdym") {
            let day_match = caps.name("mdyd").unwrap();
            let year_match = caps.name("mdyy").unwrap();
            (year_match, month_match, day_match)
        } else {
            panic!("unexpected variant");
        };

        let year: i32 = year_match.as_str().parse().expect("failed to parse year");
        let month: u32 = month_match.as_str().parse().expect("failed to parse month");
        let day: u32 = day_match.as_str().parse().expect("failed to parse day");
        Some(NaiveDate::from_ymd_opt(year, month, day).unwrap())
    }

    async fn handle_days(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let date = match Self::parse_date(command.rest.trim()) {
            Some(d) => d,
            None => return,
        };

        let delta = date.signed_duration_since(Local::now().date_naive());
        let in_days = delta.num_days();
        let response_text = if in_days < 0 {
            let day_days = if in_days == -1 { "day" } else { "days" };
            format!("{} was {} {} ago", date.format(DATE_OUTPUT_FORMAT), -in_days, day_days)
        } else if in_days == 0 {
            format!("{} is today", date.format(DATE_OUTPUT_FORMAT))
        } else {
            let day_days = if in_days == 1 { "day" } else { "days" };
            format!("{} is in {} {}", date.format(DATE_OUTPUT_FORMAT), in_days, day_days)
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_text,
        ).await;
    }

    async fn handle_weekday(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let date = match Self::parse_date(command.rest.trim()) {
            Some(d) => d,
            None => return,
        };

        let weekday_name = match date.weekday() {
            Weekday::Mon => "Monday",
            Weekday::Tue => "Tuesday",
            Weekday::Wed => "Wednesday",
            Weekday::Thu => "Thursday",
            Weekday::Fri => "Friday",
            Weekday::Sat => "Saturday",
            Weekday::Sun => "Sunday",
        };

        let today = Local::now().date_naive();
        let verb = if date < today {
            "was"
        } else if date == today {
            "is"
        } else {
            "will be"
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("{} {} a {}", date.format(DATE_OUTPUT_FORMAT), verb, weekday_name),
        ).await;
    }

    async fn handle_easter(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let year_str = command.rest.trim();
        let year: i32 = if year_str.len() == 0 {
            Local::now().year()
        } else {
            match year_str.parse() {
                Ok(y) => y,
                Err(_) => {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "Is that a year?",
                    ).await;
                    return;
                },
            }
        };
        if year < 1 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "We can reasonably assume that there was no Easter before 1 AD...",
            ).await;
            return;
        }

        if command.flags.contains("j") || command.flags.contains("julian") {
            let julian_date = julian_computus(year);
            let gregorian_date = julian_date.convert_to(julian::Calendar::GREGORIAN);

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!(
                    "Easter Sunday {} according to the Julian calendar:\nJulian date: {:04}-{:02}-{:02}\nequal to Gregorian date: {:04}-{:02}-{:02}",
                    year,
                    julian_date.year(), julian_date.month().number(), julian_date.day(),
                    gregorian_date.year(), gregorian_date.month().number(), gregorian_date.day(),
                ),
            ).await;
        } else {
            let gregorian_date = gregorian_computus(year);

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!(
                    "Easter Sunday {} according to the Gregorian calendar:\nGregorian date: {:04}-{:02}-{:02}",
                    year,
                    gregorian_date.year(), gregorian_date.month().number(), gregorian_date.day(),
                ),
            ).await;
        }
    }
}
#[async_trait]
impl RocketBotPlugin for DatePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "weekday",
                "date",
                "{cpfx}weekday DATE",
                "Reports the weekday of the specified date.",
            )
                .build()
        ).await;

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "days",
                "date",
                "{cpfx}days DATE",
                "Reports the difference, in days, between today and the specified date.",
            )
                .build()
        ).await;

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "easter",
                "date",
                "{cpfx}easter [-j|--julian] [YEAR]",
                "Outputs the date of Easter (Easter Sunday) for the given year.",
            )
                .add_flag("j")
                .add_flag("julian")
                .build()
        ).await;

        Self {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "date".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "days" {
            self.handle_days(channel_message, command).await;
        } else if command.name == "weekday" {
            self.handle_weekday(channel_message, command).await;
        } else if command.name == "easter" {
            self.handle_easter(channel_message, command).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "days" {
            Some(include_str!("../help/days.md").to_owned())
        } else if command_name == "weekday" {
            Some(include_str!("../help/weekday.md").to_owned())
        } else if command_name == "easter" {
            Some(include_str!("../help/easter.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // no config to update
        true
    }
}


#[cfg(test)]
mod tests {
    use super::{gregorian_computus, julian_computus};

    #[test]
    fn test_gregorian_computus() {
        let gregorian = gregorian_computus(1961);
        assert_eq!(gregorian.year(), 1961);
        assert_eq!(gregorian.month(), julian::Month::April);
        assert_eq!(gregorian.day(), 2);

        let gregorian = gregorian_computus(2024);
        assert_eq!(gregorian.year(), 2024);
        assert_eq!(gregorian.month(), julian::Month::March);
        assert_eq!(gregorian.day(), 31);

        let gregorian = gregorian_computus(2025);
        assert_eq!(gregorian.year(), 2025);
        assert_eq!(gregorian.month(), julian::Month::April);
        assert_eq!(gregorian.day(), 20);
    }

    #[test]
    fn test_julian_computus() {
        let julian = julian_computus(2008);
        assert_eq!(julian.year(), 2008);
        assert_eq!(julian.month(), julian::Month::April);
        assert_eq!(julian.day(), 14);

        let julian = julian_computus(2009);
        assert_eq!(julian.year(), 2009);
        assert_eq!(julian.month(), julian::Month::April);
        assert_eq!(julian.day(), 6);

        let julian = julian_computus(2010);
        assert_eq!(julian.year(), 2010);
        assert_eq!(julian.month(), julian::Month::March);
        assert_eq!(julian.day(), 22);

        let julian = julian_computus(2011);
        assert_eq!(julian.year(), 2011);
        assert_eq!(julian.month(), julian::Month::April);
        assert_eq!(julian.day(), 11);

        let julian = julian_computus(2016);
        assert_eq!(julian.year(), 2016);
        assert_eq!(julian.month(), julian::Month::April);
        assert_eq!(julian.day(), 18);

        let julian = julian_computus(2024);
        assert_eq!(julian.year(), 2024);
        assert_eq!(julian.month(), julian::Month::April);
        assert_eq!(julian.day(), 22);

        let julian = julian_computus(2025);
        assert_eq!(julian.year(), 2025);
        assert_eq!(julian.month(), julian::Month::April);
        assert_eq!(julian.day(), 7);
    }
}
