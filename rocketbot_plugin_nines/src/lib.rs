use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Weak;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use log::error;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;


static NINES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "(?:",
        "(?P<nine_count>[0-9]+)",
        "|",
        "(?P<digit_count>[0-9]+)\\s+(?P<digit>[0-9])s",
        "|",
        "(?P<percentage>[0-9]+(?:[.][0-9]+)?)%",
    ")",
    "$",
)).unwrap());
static SECONDS_PER_DAY: Lazy<BigDecimal> = Lazy::new(|| BigDecimal::from(60*60*24));
static SECONDS_PER_WEEK: Lazy<BigDecimal> = Lazy::new(|| SECONDS_PER_DAY.deref() * BigDecimal::from(7));
static SECONDS_PER_MONTH: Lazy<BigDecimal> = Lazy::new(|| SECONDS_PER_DAY.deref() * BigDecimal::from(30));
static SECONDS_PER_YEAR: Lazy<BigDecimal> = Lazy::new(|| SECONDS_PER_DAY.deref() * BigDecimal::from(365));


fn seconds_to_human_unit(seconds: &BigDecimal) -> String {
    let mut decimal_number = seconds.clone();

    if decimal_number < BigDecimal::from(2*60) {
        // less than two minutes
        return format!("{:.2} seconds", decimal_number);
    }

    decimal_number = decimal_number / BigDecimal::from(60);

    if decimal_number < BigDecimal::from(2*60) {
        // less than two hours
        return format!("{:.2} minutes", decimal_number);
    }

    decimal_number = decimal_number / BigDecimal::from(60);

    if decimal_number < BigDecimal::from(2*24) {
        // less than two days
        return format!("{:.2} hours", decimal_number);
    }

    decimal_number = decimal_number / BigDecimal::from(24);

    if decimal_number < BigDecimal::from(2*7) {
        // less than two weeks
        return format!("{:.2} days", decimal_number);
    }

    decimal_number = decimal_number / BigDecimal::from(7);

    format!("{:.2} weeks", decimal_number)
}


pub struct NinesPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for NinesPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(&CommandDefinition::new(
            "nines".to_owned(),
            "nines".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}nines NUMBER|NUMBER NUMBERs|NUMBER%".to_owned(),
            "Calculates allowed downtime for an uptime expressed as a number of nines.".to_owned(),
        )).await;

        NinesPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "nines".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        if command.name != "nines" {
            return;
        }
        let stripped_nines = command.rest.trim();

        let caps = match NINES_RE.captures(stripped_nines) {
            Some(caps) => caps,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to interpret the value...",
                ).await;
                return;
            },
        };

        let percentage = if let Some(percentage_str) = caps.name("percentage") {
            // direct percentage input
            match BigDecimal::from_str(percentage_str.as_str()) {
                Ok(p) => p,
                Err(e) => {
                    error!("failed to parse nines percentage {:?}: {}", percentage_str, e);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "What kind of a percentage is that?!",
                    ).await;
                    return;
                },
            }
        } else {
            // number of nines or other digits
            let (digit_count_str, digit_str) = if let Some(nine_count_str) = caps.name("nine_count") {
                (nine_count_str.as_str(), "9")
            } else if let Some(digit_count_str) = caps.name("digit_count") {
                let digit_str = caps
                    .name("digit").expect("group \"digit\" failed but \"digit_count\" succeeded")
                    .as_str();
                (digit_count_str.as_str(), digit_str)
            } else {
                unreachable!("unexpected nines argument format");
            };

            let digit_count: usize = match digit_count_str.parse() {
                Ok(cs) => cs,
                Err(e) => {
                    error!("failed to parse nines digit count {:?}: {}", digit_count_str, e);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "What kind of a count is that?!",
                    ).await;
                    return;
                }
            };

            let mut digits = String::with_capacity((digit_count+1).max(3));
            for i in 0..digit_count {
                digits.push_str(digit_str);
                if i == 1 {
                    // add decimal point
                    digits.push('.');
                }
            }
            if digits.len() < 2 {
                // right-pad with zeroes (one nine = 90%)
                digits.push('0');
            }

            // return as percentage
            match BigDecimal::from_str(&digits) {
                Ok(p) => p,
                Err(e) => {
                    error!("failed to parse assembled value {:?} from {:?} {:?}s: {}", digits, digit_count_str, digit_str, e);
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "What kind of a value is that?!",
                    ).await;
                    return;
                },
            }
        };

        // convert percentage to downtime factor
        let downtime_factor: BigDecimal = BigDecimal::from(1) - (&percentage / 100);
        let downtime_per_day = &downtime_factor * SECONDS_PER_DAY.deref();
        let downtime_per_week = &downtime_factor * SECONDS_PER_WEEK.deref();
        let downtime_per_month = &downtime_factor * SECONDS_PER_MONTH.deref();
        let downtime_per_year = &downtime_factor * SECONDS_PER_YEAR.deref();

        let message = format!(
            "{}% is a downtime of {} per day or {} per week or {} per month or {} per year",
            percentage, seconds_to_human_unit(&downtime_per_day),
            seconds_to_human_unit(&downtime_per_week), seconds_to_human_unit(&downtime_per_month),
            seconds_to_human_unit(&downtime_per_year),
        );
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &message,
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "nines" {
            Some(include_str!("../help/nines.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // not much to update
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_regex_nine_count() {
        let caps = super::NINES_RE.captures("12").unwrap();
        assert_eq!("12", caps.name("nine_count").unwrap().as_str());
    }

    #[test]
    fn test_regex_digit_count() {
        let caps = super::NINES_RE.captures("12 5s").unwrap();
        assert_eq!("12", caps.name("digit_count").unwrap().as_str());
        assert_eq!("5", caps.name("digit").unwrap().as_str());
    }

    #[test]
    fn test_regex_percentage_count() {
        let caps = super::NINES_RE.captures("12%").unwrap();
        assert_eq!("12", caps.name("percentage").unwrap().as_str());
    }
}
