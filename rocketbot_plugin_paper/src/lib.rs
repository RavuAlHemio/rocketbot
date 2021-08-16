use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::sync::Weak;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


static PAPER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^\\s*(?P<series>[ABCabc])\\s*(?P<index>-?\\s*[0-9]+(?:\\s*[0-9]+)*)\\s*$",
).expect("failed to compile regex"));

static SI_THOUSANDS: &[&str] = &[
    "y", "z", "a", "f", "p", "n", "\u{3BC}", "m", "", "k", "M", "G", "T", "P", "E", "Z", "Y",
];
const SI_THOUSANDS_OFFSET: isize = 8;
const SI_PREFIX_MAX: f64 = 5000.0;
const SI_PREFIX_MIN: f64 = 5.0;


fn paper_size(series: &str, order: f64) -> (f64, f64) {
    /* derivation of lengths for an area:
     * long/short = sqrt(2), long*short = area
     * long/short = sqrt(2), short = area/long
     * long/(area / long) = sqrt(2)
     * long*long / area = sqrt(2)
     * long^2 / area = sqrt(2)
     * long^2 = sqrt(2) * area
     * long = sqrt(sqrt(2) * area)
     */

     let series_upper = series.to_uppercase();
     let area_m2 = if series_upper == "A" {
        // A0: area is 1 m^2
        // Aorder = 2^(-order) m^2

        2.0_f64.powf(-order)
     } else if series_upper == "B" {
        // Border's area is the geometric mean between Aorder's area and A(order-1)'s area
        // A0 = 1 m^2, 2A0 = 2 m^2
        // => B0 = sqrt(1 m^2 * 2 m^2) = sqrt(2 m^4) = sqrt(2) m^2

        // more generally:
        // Border = sqrt(2^(-order) m^2 * 2 * 2^(-order) m^2) = sqrt(2^(2*(-order)+1))

        f64::sqrt(2.0_f64.powf(2.0 * (-order) + 1.0))
     } else if series_upper == "C" {
         // Corder's area is the geometric mean between Aorder's area and Border's area
         // A0 = 1 m^2, B0 = sqrt(2) m^2
         // => C0 = sqrt(1 m^2 * sqrt(2) m^2) = sqrt(sqrt(2) m^4) = sqrt(sqrt(2)) m^2

         // more generally:
         // Corder = sqrt(sqrt(2^(-order)) * sqrt(2^(2*norder+1)))

         f64::sqrt(f64::sqrt(2.0_f64.powf(-order)) * f64::sqrt(2.0_f64.powf(2.0 * (-order) + 1.0)))
     } else {
         panic!("unknown ISO 216 series {:?}", series_upper);
     };

     let long_m = f64::sqrt(f64::sqrt(2.0) * area_m2);
     let short_m = long_m / f64::sqrt(2.0);

     (long_m, short_m)
}

fn si_prefix(mut value: f64) -> (&'static str, f64) {
    let mut index: isize = 0;

    if value == 0.0 {
        return ("", value);
    }

    while value > SI_PREFIX_MAX {
        value /= 1000.0;
        index += 1;
    }
    while value < SI_PREFIX_MIN {
        value *= 1000.0;
        index -= 1;
    }

    let mut index_with_offset = index + SI_THOUSANDS_OFFSET;
    while index_with_offset < 0 {
        value /= 1000.0;
        index_with_offset += 1;
    }
    let prefix_count_isize: isize = SI_THOUSANDS.len().try_into().unwrap();
    while index_with_offset >= prefix_count_isize {
        value *= 1000.0;
        index_with_offset -= 1;
    }

    let index_with_offset_usize: usize = index_with_offset.try_into().unwrap();
    (SI_THOUSANDS[index_with_offset_usize], value)
}


pub struct PaperPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for PaperPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(&CommandDefinition::new(
            "paper".to_owned(),
            "paper".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}paper PAPER".to_owned(),
            "Displays the size of the given ISO 216-like paper.".to_owned(),
        )).await;

        PaperPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "paper".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "paper" {
            return;
        }

        let (series, index_str) = match PAPER_RE.captures(&command.rest) {
            Some(caps) => (caps.name("series").unwrap().as_str(), caps.name("index").unwrap().as_str()),
            None => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{} Failed to parse paper type.", channel_message.message.sender.username),
                ).await;
                return;
            },
        };

        let mut index_trimmed = String::with_capacity(index_str.len());
        for c in index_str.chars() {
            if c == '-' || c.is_ascii_digit() {
                index_trimmed.push(c);
            }
        }

        let index: f64 = match index_trimmed.parse() {
            Ok(i) => i,
            Err(_e) => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{} Failed to parse index.", channel_message.message.sender.username),
                ).await;
                return;
            },
        };

        let (long_m, short_m) = paper_size(series, index);
        let (long_pfx, long_val) = si_prefix(long_m);
        let (short_pfx, short_val) = si_prefix(short_m);

        interface.send_channel_message(
            &channel_message.channel.name,
            &format!(
                "@{} {}{}: {} {}m \u{D7} {} {}m",
                channel_message.message.sender.username,
                series, index,
                long_val, long_pfx, short_val, short_pfx,
            ),
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "paper" {
            Some(include_str!("../help/paper.md").to_owned())
        } else {
            None
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_si_prefix() {
        let (zero_pfx, zero_val) = si_prefix(0.0);
        assert_eq!("", zero_pfx);
        assert_eq!(0.0, zero_val);
    }
}
