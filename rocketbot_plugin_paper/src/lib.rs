use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::fmt::Write;
use std::ops::Deref;
use std::sync::Weak;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, One, Zero};
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandBehaviors, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


static PAPER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^\\s*(?P<series>[A-Za-z])\\s*(?P<index>-?\\s*[0-9]+(?:\\s*[0-9]+)*)\\s*$",
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
        dec *= &bd1024;
        power -= &bi10;
    }
    while power > zero {
        dec = dec.double();
        power -= 1;
    }

    while power < bim10 {
        dec = dec / &bd1024;
        power += &bi10;
    }
    while power < zero {
        dec = dec.half();
        power += 1;
    }

    dec
}


macro_rules! paper_area_func_geom_order_order {
    ($func_name:ident, $order_parent_1:path, $order_parent_2:path) => {
        #[inline]
        fn $func_name(order: &BigInt) -> Option<BigDecimal> {
            ($order_parent_1(order)? * $order_parent_2(order)?).sqrt()
        }
    };
}

macro_rules! paper_area_func_geom_order_orderm1 {
    ($func_name:ident, $order_parent:path, $orderm1_parent:path) => {
        #[inline]
        fn $func_name(order: &BigInt) -> Option<BigDecimal> {
            let orderm1: BigInt = order - 1;
            ($order_parent(order)? * $orderm1_parent(&orderm1)?).sqrt()
        }
    };
}

// layer 0 (A is ISO 216)
fn paper_area_a(order: &BigInt) -> Option<BigDecimal> {
    Some(twopow(-order))
}

// layer 1 (B is ISO 216)
paper_area_func_geom_order_orderm1!(paper_area_b, paper_area_a, paper_area_a);

// layer 2 (C is ISO 216, D is the Swedish extension SIS 014711)
paper_area_func_geom_order_order!(paper_area_c, paper_area_a, paper_area_b);
paper_area_func_geom_order_orderm1!(paper_area_d, paper_area_b, paper_area_a);

// layer 3 (E, F and G are SIS 014711, H is a logical extension thereof)
paper_area_func_geom_order_order!(paper_area_e, paper_area_a, paper_area_c);
paper_area_func_geom_order_order!(paper_area_f, paper_area_c, paper_area_b);
paper_area_func_geom_order_order!(paper_area_g, paper_area_b, paper_area_d);
paper_area_func_geom_order_orderm1!(paper_area_h, paper_area_d, paper_area_a);

// layer 4 (I, J, K, L, M, N, O and P are further subdivisions below SIS 014711)
paper_area_func_geom_order_order!(paper_area_i, paper_area_a, paper_area_e);
paper_area_func_geom_order_order!(paper_area_j, paper_area_e, paper_area_c);
paper_area_func_geom_order_order!(paper_area_k, paper_area_c, paper_area_f);
paper_area_func_geom_order_order!(paper_area_l, paper_area_f, paper_area_b);
paper_area_func_geom_order_order!(paper_area_m, paper_area_b, paper_area_g);
paper_area_func_geom_order_order!(paper_area_n, paper_area_g, paper_area_d);
paper_area_func_geom_order_order!(paper_area_o, paper_area_d, paper_area_h);
paper_area_func_geom_order_orderm1!(paper_area_p, paper_area_h, paper_area_a);

// layer 5 (Q, R, S, T, U, V, W, X, Y and Z are further subdivisions)
// further sizes exist on this layer -- AA through AF -- but they are not generated as functions here
paper_area_func_geom_order_order!(paper_area_q, paper_area_a, paper_area_i);
paper_area_func_geom_order_order!(paper_area_r, paper_area_i, paper_area_e);
paper_area_func_geom_order_order!(paper_area_s, paper_area_e, paper_area_j);
paper_area_func_geom_order_order!(paper_area_t, paper_area_j, paper_area_c);
paper_area_func_geom_order_order!(paper_area_u, paper_area_c, paper_area_k);
paper_area_func_geom_order_order!(paper_area_v, paper_area_k, paper_area_f);
paper_area_func_geom_order_order!(paper_area_w, paper_area_f, paper_area_l);
paper_area_func_geom_order_order!(paper_area_x, paper_area_l, paper_area_b);
paper_area_func_geom_order_order!(paper_area_y, paper_area_b, paper_area_m);
paper_area_func_geom_order_order!(paper_area_z, paper_area_m, paper_area_g);

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
        paper_area_a(order)?
    } else if series_upper == "B" {
        paper_area_b(order)?
    } else if series_upper == "C" {
        paper_area_c(order)?
    } else if series_upper == "D" {
        paper_area_d(order)?
    } else if series_upper == "E" {
        paper_area_e(order)?
    } else if series_upper == "F" {
        paper_area_f(order)?
    } else if series_upper == "G" {
        paper_area_g(order)?
    } else if series_upper == "H" {
        paper_area_h(order)?
    } else if series_upper == "I" {
        paper_area_i(order)?
    } else if series_upper == "J" {
        paper_area_j(order)?
    } else if series_upper == "K" {
        paper_area_k(order)?
    } else if series_upper == "L" {
        paper_area_l(order)?
    } else if series_upper == "M" {
        paper_area_m(order)?
    } else if series_upper == "N" {
        paper_area_n(order)?
    } else if series_upper == "O" {
        paper_area_o(order)?
    } else if series_upper == "P" {
        paper_area_p(order)?
    } else if series_upper == "Q" {
        paper_area_q(order)?
    } else if series_upper == "R" {
        paper_area_r(order)?
    } else if series_upper == "S" {
        paper_area_s(order)?
    } else if series_upper == "T" {
        paper_area_t(order)?
    } else if series_upper == "U" {
        paper_area_u(order)?
    } else if series_upper == "V" {
        paper_area_v(order)?
    } else if series_upper == "W" {
        paper_area_w(order)?
    } else if series_upper == "X" {
        paper_area_x(order)?
    } else if series_upper == "Y" {
        paper_area_y(order)?
    } else if series_upper == "Z" {
        paper_area_z(order)?
    } else {
        panic!("unknown ISO 216 or extension series {:?}", series_upper);
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

fn to_scientific(dec: &BigDecimal) -> String {
    let (bi, exp) = dec.as_bigint_and_exponent();
    let mut bi_string = bi.to_string();

    let out_exp = if bi_string.len() > 1 {
        // insert the decimal point
        bi_string.insert(1, '.');

        // the exponent is the number of digits minus one (and don't forget the decimal point)
        (bi_string.len() - 2) as i64 - exp
    } else {
        (bi_string.len() - 1) as i64 - exp
    };

    // append the exponent
    write!(&mut bi_string, "e{}", out_exp).unwrap();

    bi_string
}

fn maybe_to_scientific(dec: &BigDecimal) -> String {
    let standard = dec.to_string();
    if standard.starts_with("0.00000") || standard.ends_with("000000") {
        to_scientific(&dec)
    } else {
        standard
    }
}


pub struct PaperPlugin {
    interface: Weak<dyn RocketBotInterface>,
    max_index: BigInt,
    output_precision: u64,
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
        let output_precision = config["output_precision"].as_u64()
            .expect("output_precision missing or not a u64");

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
            output_precision,
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
                send_channel_message!(
                    interface,
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
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("@{} Failed to parse index.", channel_message.message.sender.username),
                ).await;
                return;
            },
        };

        if index > self.max_index || index < -&self.max_index {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &format!("@{} Index too large.", channel_message.message.sender.username),
            ).await;
            return;
        }

        let (long_m, short_m) = match paper_size(series, &index) {
            Some(lmsm) => lmsm,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("@{} Value out of bounds. :(", channel_message.message.sender.username),
                ).await;
                return;
            }
        };
        let (long_pfx, long_val) = si_prefix(long_m);
        let (short_pfx, short_val) = si_prefix(short_m);

        let long_prec = long_val.with_prec(self.output_precision);
        let short_prec = short_val.with_prec(self.output_precision);
        let long_sci = maybe_to_scientific(&long_prec);
        let short_sci = maybe_to_scientific(&short_prec);

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!(
                "@{} {}{}: {} {}m \u{D7} {} {}m",
                channel_message.message.sender.username,
                series, index,
                long_sci, long_pfx, short_sci, short_pfx,
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
        let mut series = String::with_capacity(1);
        for c in 'A'..='Z' {
            series.clear();
            series.push(c);
            test_paper_size(&series, order_i);
        }
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

    fn test_single_precision(
        series: &str,
        order: i64,
        prec: u64,
        expect_long_pfx: &str,
        expect_long_str: &str,
        expect_short_pfx: &str,
        expect_short_str: &str,
    ) {
        let (long_side, short_side) = paper_size(series, &BigInt::from(order)).unwrap();
        let (long_pfx, long_val) = si_prefix(long_side);
        let (short_pfx, short_val) = si_prefix(short_side);

        assert_eq!(expect_long_pfx, long_pfx);
        assert_eq!(expect_short_pfx, short_pfx);

        let long_prec = long_val.with_prec(prec);
        let short_prec = short_val.with_prec(prec);

        assert_eq!(expect_long_str, long_prec.to_string());
        assert_eq!(expect_short_str, short_prec.to_string());
    }

    #[test]
    fn test_precision() {
        test_single_precision(
            "A", 4, 6,
            "m", "297.302",
            "m", "210.224",
        );
        test_single_precision(
            "A", 4242, 6,
            "y", "0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000389616",
            "y", "0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000275500",
        );
    }

    fn test_single_to_scientific(expected: &str, to_sci_decimal: &str) {
        let dec: BigDecimal = to_sci_decimal.parse().unwrap();
        assert_eq!(expected, to_scientific(&dec))
    }

    #[test]
    fn test_to_scientific() {
        test_single_to_scientific("1.2345e4", "12345");
        test_single_to_scientific("1.2345e1", "12.345");
        test_single_to_scientific("1.2345e-3", "0.0012345");
        test_single_to_scientific("1.2345e-69", "0.0000000000000000000000000000000000000000000000000000000000000000000012345");
    }
}
