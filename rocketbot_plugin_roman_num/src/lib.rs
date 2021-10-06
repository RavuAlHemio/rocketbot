use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage};
use serde_json;


pub struct RomanNumPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
impl RomanNumPlugin {
    async fn handle_roman(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let arabic_digits = command.rest.trim();
        let roman_numerals = arabic2roman(arabic_digits)
            .unwrap_or_else(|| "?".to_owned());

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &roman_numerals,
        ).await;
    }

    async fn handle_unroman(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let roman_numerals = command.rest.trim();
        let arabic_digits = roman2arabic(roman_numerals)
            .unwrap_or_else(|| "?".to_owned());

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &arabic_digits,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for RomanNumPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "roman".to_owned(),
                "roman_num".to_owned(),
                "{cpfx}roman NUMBER".to_owned(),
                "Converts the given number, given in Arabic digits, into Roman numerals.".to_owned(),
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "unroman".to_owned(),
                "roman_num".to_owned(),
                "{cpfx}unroman ROMAN".to_owned(),
                "Converts the given number, given in Roman numerals, into Arabic digits.".to_owned(),
            )
                .build()
        ).await;

        Self {
            interface
        }
    }

    async fn plugin_name(&self) -> String {
        "roman_num".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "roman" {
            self.handle_roman(channel_message, command).await
        } else if command.name == "unroman" {
            self.handle_unroman(channel_message, command).await
        }
    }
}


fn roman2arabic(roman_numerals: &str) -> Option<String> {
    let letter_to_value = {
        let mut ltv: HashMap<char, u16> = HashMap::new();
        ltv.insert('I', 1);
        ltv.insert('V', 5);
        ltv.insert('X', 10);
        ltv.insert('L', 50);
        ltv.insert('C', 100);
        ltv.insert('D', 500);
        ltv.insert('M', 1000);
        ltv
    };

    // convert numerals to numbers
    let numbers = {
        let mut nums = Vec::with_capacity(roman_numerals.len());
        for c in roman_numerals.chars() {
            let num = match letter_to_value.get(&c) {
                Some(&v) => v,
                None => return None,
            };
            nums.push(num);
        }
        nums
    };

    // collect sequences of ascending numbers
    // then, sum up all the occurrences of the final number and subtract from it the sum of the other numbers
    // e.g. IIIIVMM => [1, 1, 1, 1, 5, 1000, 1000] => sum([1000, 1000]) - sum([1, 1, 1, 1, 5]) = 2000 - 9 = 1991
    let asc_sequences = {
        let mut seqs: Vec<Vec<u16>> = Vec::with_capacity(numbers.len());
        let mut cur_seq: Vec<u16> = Vec::with_capacity(numbers.len());

        for &n in &numbers {
            if cur_seq.last().map(|&l| n < l).unwrap_or(false) {
                // ascending sequence has ended
                seqs.push(cur_seq);
                cur_seq = Vec::with_capacity(numbers.len());
            }
            cur_seq.push(n);
        }

        // push last sequence
        if cur_seq.len() > 0 {
            seqs.push(cur_seq);
        }

        seqs
    };

    let mut sum: u16 = 0;
    for asc_seq in &asc_sequences {
        let last_value = *asc_seq.last().unwrap();
        let mut seq_sum = last_value;
        for &subtrahend in &asc_seq[0..asc_seq.len()-1] {
            if subtrahend == last_value {
                seq_sum = seq_sum.checked_add(subtrahend)?;
            } else {
                seq_sum = seq_sum.checked_sub(subtrahend)?;
            }
        }
        sum = sum.checked_add(seq_sum)?;
    }
    Some(sum.to_string())
}

fn arabic2roman(arabic_digits: &str) -> Option<String> {
    let numero: u16 = arabic_digits.parse().ok()?;
    if numero > 3999 {
        // assume number too large
        return None;
    }

    let pieces = &[
        (1000, "M"),
        (100, "CDM"),
        (10, "XLC"),
        (1, "IVX"),
    ];
    let mut ret = String::new();
    for (factor, letters) in *pieces {
        let value = (numero / factor) % 10;
        let letter_vec: Vec<char> = letters.chars().collect();

        if letter_vec.len() == 1 {
            // purely additive
            for _ in 0..value {
                ret.push(letter_vec[0]);
            }
            continue;
        }

        match value {
            0 => {},
            1|2|3 => {
                // I, II, III
                for _ in 0..value {
                    ret.push(letter_vec[0]);
                }
            },
            4 => {
                // IV
                ret.push(letter_vec[0]);
                ret.push(letter_vec[1]);
            },
            5|6|7|8 => {
                // V, VI, VII, VIII
                ret.push(letter_vec[1]);
                for _ in 0..(value-5) {
                    ret.push(letter_vec[0]);
                }
            },
            9 => {
                // IX
                ret.push(letter_vec[0]);
                ret.push(letter_vec[2]);
            },
            _other => unreachable!(),
        }
    }
    Some(ret)
}


#[cfg(test)]
mod tests {
    use super::{arabic2roman, roman2arabic};

    fn run_a2r(expected_roman: &str, arabic: &str) {
        let roman = arabic2roman(arabic);
        assert_eq!(&roman.unwrap(), expected_roman);
    }

    fn run_r2a(roman: &str, expected_arabic: &str) {
        let arabic = roman2arabic(roman);
        assert_eq!(&arabic.unwrap(), expected_arabic);
    }

    #[test]
    fn test_roman_to_arabic() {
        run_r2a("I", "1");
        run_r2a("II", "2");
        run_r2a("III", "3");
        run_r2a("IIII", "4");
        run_r2a("IV", "4");
        run_r2a("V", "5");
        run_r2a("VI", "6");
        run_r2a("VII", "7");
        run_r2a("VIII", "8");
        run_r2a("IX", "9");
        run_r2a("X", "10");
        run_r2a("XI", "11");
        run_r2a("XII", "12");
        run_r2a("L", "50");
        run_r2a("LXIX", "69");
        run_r2a("C", "100");
        run_r2a("D", "500");
        run_r2a("M", "1000");
        run_r2a("MCMLIV", "1954");

        // less official syntax
        run_r2a("IIIIVMM", "1991");
    }

    #[test]
    fn test_arabic_to_roman() {
        run_a2r("I", "1");
        run_a2r("II", "2");
        run_a2r("III", "3");
        run_a2r("IV", "4");
        run_a2r("V", "5");
        run_a2r("VI", "6");
        run_a2r("VII", "7");
        run_a2r("VIII", "8");
        run_a2r("IX", "9");
        run_a2r("X", "10");
        run_a2r("XI", "11");
        run_a2r("XII", "12");
        run_a2r("L", "50");
        run_a2r("LXIX", "69");
        run_a2r("C", "100");
        run_a2r("D", "500");
        run_a2r("M", "1000");
        run_a2r("MCMLIV", "1954");
    }
}
