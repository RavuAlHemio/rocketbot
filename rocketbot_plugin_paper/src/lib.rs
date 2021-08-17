use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::ops::Deref;
use std::sync::Weak;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, One, Zero};
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
const SI_PREFIX_MAX: Lazy<BigDecimal> = Lazy::new(|| BigDecimal::from_i32(5000).unwrap());
const SI_PREFIX_MIN: Lazy<BigDecimal> = Lazy::new(|| BigDecimal::from_i32(5).unwrap());


fn twopow(mut power: BigInt) -> BigDecimal {
    let mut dec = BigDecimal::one();
    let zero = BigInt::zero();

    let bd1024 = BigDecimal::from(1024);
    let bi10 = BigInt::from(10);
    let bim10 = -&bi10;

    while power > bi10 {
        dec = dec / &bd1024;
        power -= &bi10;
    }
    while power > zero {
        dec = dec.double();
        power -= 1;
    }

    while power < bim10 {
        dec *= &bd1024;
        power += &bi10;
    }
    while power < zero {
        dec = dec.half();
        power += 1;
    }

    dec
}


fn paper_size(series: &str, order: &BigInt) -> Option<(BigDecimal, BigDecimal)> {
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

        twopow(-order)
    } else if series_upper == "B" {
        // Border's area is the geometric mean between Aorder's area and A(order-1)'s area
        // A0 = 1 m^2, 2A0 = 2 m^2
        // => B0 = sqrt(1 m^2 * 2 m^2) = sqrt(2 m^4) = sqrt(2) m^2

        // more generally:
        // Border = sqrt(2^(-order) m^2 * 2 * 2^(-order) m^2) = sqrt(2^(2*(-order)+1))

        let b_pow: BigInt = (-order) * 2 + 1;
        twopow(b_pow).sqrt()?
    } else if series_upper == "C" {
        // Corder's area is the geometric mean between Aorder's area and Border's area
        // A0 = 1 m^2, B0 = sqrt(2) m^2
        // => C0 = sqrt(1 m^2 * sqrt(2) m^2) = sqrt(sqrt(2) m^4) = sqrt(sqrt(2)) m^2

        // more generally:
        // Corder = sqrt(sqrt(2^(-order)) * sqrt(2^(2*norder+1)))

        let b_pow: BigInt = (-order) * 2 + 1;

        let a_area = twopow(-order);
        let b_area = twopow(b_pow).sqrt()?;

        (a_area * b_area).sqrt()?
    } else {
        panic!("unknown ISO 216 series {:?}", series_upper);
    };

    let sqrt_2 = BigDecimal::from(2).sqrt().unwrap();

    let long_m = (&sqrt_2 * area_m2).sqrt()?;
    let short_m = &long_m / &sqrt_2;

    Some((long_m, short_m))
}

fn si_prefix(mut value: BigDecimal) -> (&'static str, BigDecimal) {
    let mut index_with_offset = SI_THOUSANDS_OFFSET;
    let max_index: isize = isize::try_from(SI_THOUSANDS.len()).unwrap() - 1;

    let thousand = BigDecimal::from(1000);

    if value == BigDecimal::zero() {
        return (SI_THOUSANDS[0], value);
    }

    while &value > SI_PREFIX_MAX.deref() && index_with_offset < max_index {
        value = value / &thousand;
        index_with_offset += 1;
    }
    while &value < SI_PREFIX_MIN.deref() && index_with_offset > 0 {
        value *= &thousand;
        index_with_offset -= 1;
    }

    let index_with_offset_usize: usize = index_with_offset.try_into().unwrap();
    (SI_THOUSANDS[index_with_offset_usize], value)
}


pub struct PaperPlugin {
    interface: Weak<dyn RocketBotInterface>,
    max_index: BigInt,
}
#[async_trait]
impl RocketBotPlugin for PaperPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let max_index_str = config["max_index"].as_str()
            .expect("max_index missing or not a string");
        let max_index: BigInt = max_index_str.parse()
            .expect("failed to parse max_index");

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
            max_index,
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

        let index: BigInt = match index_trimmed.parse() {
            Ok(i) => i,
            Err(_e) => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{} Failed to parse index.", channel_message.message.sender.username),
                ).await;
                return;
            },
        };

        if index > self.max_index || index < -&self.max_index {
            interface.send_channel_message(
                &channel_message.channel.name,
                &format!("@{} Index too large.", channel_message.message.sender.username),
            ).await;
            return;
        }

        let (long_m, short_m) = match paper_size(series, &index) {
            Some(lmsm) => lmsm,
            None => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{} Value out of bounds. :(", channel_message.message.sender.username),
                ).await;
                return;
            }
        };
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

    fn test_paper_size(series: &str, order_i: i64) {
        let order = BigInt::from(order_i);
        match paper_size(series, &order) {
            None => {
                panic!("{}{} is not computable", series, order);
            },
            Some((long_side, short_side)) => {
                // ensure the SI prefixes can be calculated
                si_prefix(long_side);
                si_prefix(short_side);
            },
        }
    }

    fn test_paper_sizes(order_i: i64) {
        test_paper_size("A", order_i);
        test_paper_size("B", order_i);
        test_paper_size("C", order_i);
    }

    #[test]
    fn test_zero_si_prefix() {
        let (zero_pfx, zero_val) = si_prefix(BigDecimal::zero());
        assert_eq!("y", zero_pfx);
        assert_eq!(BigDecimal::zero(), zero_val);
    }

    #[test]
    fn test_si_prefixes() {
        let (pfx, val) = si_prefix(BigDecimal::from(9000));
        assert_eq!("k", pfx);
        assert_eq!(BigDecimal::from(9), val);

        let (pfx, val) = si_prefix("0.009".parse().unwrap());
        assert_eq!("m", pfx);
        assert_eq!(BigDecimal::from(9), val);

        let (pfx, val) = si_prefix(BigDecimal::from(9_000_000_000_i64));
        assert_eq!("G", pfx);
        assert_eq!(BigDecimal::from(9), val);
    }

    #[test]
    fn test_ten_each_way_concrete() {
        for order_i32 in -10..=10 {
            test_paper_sizes(order_i32);
        }
    }

    #[test]
    fn test_extremes() {
        test_paper_sizes(1755);
        test_paper_sizes(1769);
        test_paper_sizes(1791);
        test_paper_sizes(1794);
        test_paper_sizes(99999);
    }
}
