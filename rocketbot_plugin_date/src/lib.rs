use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Local, NaiveDate, Weekday};
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
        Some(NaiveDate::from_ymd(year, month, day))
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

        let delta = date.signed_duration_since(Local::today().naive_local());
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

        let today = Local::today().naive_local();
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
                "weekday".to_string(),
                "date".to_owned(),
                "{cpfx}weekday DATE".to_owned(),
                "Reports the weekday of the specified date.".to_owned(),
            )
                .build()
        ).await;

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "days".to_string(),
                "date".to_owned(),
                "{cpfx}days DATE".to_owned(),
                "Reports the difference, in days, between today and the specified date.".to_owned(),
            )
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
        }
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // no config to update
        true
    }
}
