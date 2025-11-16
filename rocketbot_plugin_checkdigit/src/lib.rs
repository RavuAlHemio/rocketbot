use std::borrow::Cow;
use std::fmt::{self, Write};
use std::sync::Weak;

use async_trait::async_trait;
use num_bigint::BigUint;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Error {
    InvalidInputFormat {
        expected_format: Cow<'static, str>,
    },
    ForbiddenInputValue {
        theoretical_check_digit: Option<String>,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInputFormat { expected_format }
                => write!(f, "invalid input format; expected {:?}", expected_format),
            Self::ForbiddenInputValue { theoretical_check_digit }
                => if let Some(tcd) = theoretical_check_digit {
                    write!(f, "value not allowed for this format; check digit is theoretically {:?}", tcd)
                } else {
                    write!(f, "value not allowed for this format")
                },
        }
    }
}
impl std::error::Error for Error {
}


fn is_only_ascii_digits_and_placeholders(check_digit_input: &str) -> bool {
    !check_digit_input.chars()
        .filter(|c| *c != '#')
        .any(|c|
            (c.is_numeric() && !c.is_ascii_digit())
            || c.is_alphabetic()
            || c.is_control()
        )
}

fn is_only_ascii_digits_uppercase_and_placeholders(check_digit_input: &str) -> bool {
    !check_digit_input.chars()
        .filter(|c| *c != '#')
        .any(|c|
            (c.is_numeric() && !c.is_ascii_digit())
            || (c.is_alphabetic() && !c.is_ascii_uppercase())
            || c.is_control()
        )
}

fn extract_ascii_digits_and_placeholders(check_digit_input: &str) -> Vec<u8> {
    check_digit_input.chars()
        .filter(|c| *c == '#' || c.is_ascii_digit())
        .map(|c|
            if c == '#' {
                0xFF
            } else {
                u8::try_from(u32::from(c) - u32::from('0')).unwrap()
            }
        )
        .collect()
}

fn extract_ascii_digits_uppercase_and_placeholders(check_digit_input: &str) -> Vec<u8> {
    check_digit_input.chars()
        .filter(|c| *c == '#' || c.is_ascii_digit() || c.is_ascii_uppercase())
        .map(|c|
            if c == '#' {
                0xFF
            } else if c.is_ascii_uppercase() {
                // 10-35
                u8::try_from(u32::from(c) + 10 - u32::from('A')).unwrap()
            } else {
                // 0-9
                debug_assert!(c.is_ascii_digit());
                u8::try_from(u32::from(c) - u32::from('0')).unwrap()
            }
        )
        .collect()
}


fn check_digit_luhn(check_digit_input: &str) -> Result<String, Error> {
    const EXPECTED_FORMAT_LUHN: &str = "0...#";

    // 1. assign the digits indexes from right to left starting at 0
    // 2. total_sum = 0
    // 3. for each digit
    //   a. value = if the index is divisible by 2, 2*digit, otherwise digit
    //   b. digit_sum = sum of the decimal digits of value
    //   c. total_sum = total_sum + digit_sum
    // 4. check_digit = (10 - (total_sum mod 10)) mod 10

    if !is_only_ascii_digits_and_placeholders(check_digit_input) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_LUHN),
        });
    }

    let mut digits = extract_ascii_digits_and_placeholders(check_digit_input);
    if digits.last() == Some(&0xFF) {
        digits.pop();
    }
    if digits.len() == 0 || digits.contains(&0xFF) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_LUHN),
        });
    }
    digits.reverse();

    let mut multiply_by_two = true;
    let mut digit_sum = 0u8;
    for digit in digits {
        let multiplied = if multiply_by_two {
            digit * 2
        } else {
            digit
        };

        // 0 -> 0, 1 -> 1, 2 -> 2, ..., 9 -> 9, 10 -> 1, 11 -> 2, ..., 18 -> 9
        let value = match multiplied {
            0..=9 => multiplied,
            10..=18 => multiplied - 9,
            _ => unreachable!(),
        };
        digit_sum = (digit_sum + value) % 10;

        multiply_by_two = !multiply_by_two;
    }

    let check_digit = (10 - digit_sum) % 10;
    Ok(format!("{}", check_digit))
}


fn check_digit_atsvnr(check_digit_input: &str) -> Result<String, Error> {
    const EXPECTED_FORMAT_ATSVNR: &str = "000# 00 00 00";
    const ATSVNR_FACTORS: [u8; 9] = [3, 7, 9, 5, 8, 4, 2, 1, 6];

    // 1. multiply the digits left-to-right with each digit of: 379584216
    // 2. sum up the products
    // 3. calculate the sum modulo 11
    // 4. if the result is 10, the input is invalid

    if !is_only_ascii_digits_and_placeholders(check_digit_input) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_ATSVNR),
        });
    }

    let mut digits = extract_ascii_digits_and_placeholders(check_digit_input);
    if digits.len() < 9 || digits.len() > 10 {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_ATSVNR),
        });
    }
    if digits[3] == 0xFF {
        digits.remove(3);
    }
    if digits.len() != 9 || digits.contains(&0xFF) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_ATSVNR),
        });
    }

    let product_sum: u16 = digits
        .iter()
        .zip(ATSVNR_FACTORS.iter())
        .map(|(s, f)| u16::from(s * f))
        .sum();
    let check_digit = product_sum % 11;
    if check_digit == 10 {
        Err(Error::ForbiddenInputValue { theoretical_check_digit: None })
    } else {
        Ok(format!("{}", check_digit))
    }
}


fn check_digit_czrodc(check_digit_input: &str) -> Result<String, Error> {
    const EXPECTED_FORMAT_CZRODC: &str = "000000/000#";

    // calculate the whole number mod 11
    // if the result is 10, the number is theoretically invalid, but some exist in the wild with check digit 0

    if !is_only_ascii_digits_and_placeholders(check_digit_input) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_CZRODC),
        });
    }

    let mut digits = extract_ascii_digits_and_placeholders(check_digit_input);
    if digits.len() < 9 || digits.len() > 10 {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_CZRODC),
        });
    }
    if digits.last() == Some(&0xFF) {
        digits.pop();
    }
    if digits.len() != 9 || digits.contains(&0xFF) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_CZRODC),
        });
    }

    let mut full_number = 0u32;
    for digit in digits {
        full_number *= 10;
        full_number += u32::from(digit);
    }
    let check_digit = full_number % 11;
    if check_digit == 10 {
        Err(Error::ForbiddenInputValue { theoretical_check_digit: Some("0".to_owned()) })
    } else {
        Ok(format!("{}", check_digit))
    }
}


fn check_digit_iban(check_digit_input: &str) -> Result<String, Error> {
    const EXPECTED_FORMAT_IBAN: &str = "ZZ## OOOO...";

    // 1. replace check digits with 00
    // 2. move initial 4 characters (country code and check digit placeholder) to the end
    // 3. replace each letter with a two-digit number: A = 10, B = 11, ..., Z = 35
    // 4. interpret the whole string of digits as a decimal number
    // 5. calculate this number mod 97
    // 6. subtract the remainder from 98
    // 7. pad to two digits if necessary

    if !is_only_ascii_digits_uppercase_and_placeholders(check_digit_input) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_IBAN),
        });
    }

    // this operation already converts letters to number values 10-35
    let mut digits = extract_ascii_digits_uppercase_and_placeholders(check_digit_input);
    if digits.len() < 4 {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_IBAN),
        });
    }
    if digits[0..2].contains(&0xFF) || digits[4..].contains(&0xFF) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_IBAN),
        });
    }
    if digits[2] != 0xFF || digits[3] != 0xFF {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_IBAN),
        });
    }

    // replace check digits with 00
    digits[2] = 0;
    digits[3] = 0;

    // move initial 4 characters to the end
    digits.push(digits[0]);
    digits.push(digits[1]);
    digits.push(digits[2]);
    digits.push(digits[3]);
    digits.drain(0..4);

    // collect string
    let mut number_calc_string = String::new();
    debug_assert!(!digits.contains(&0xFF));
    for digit in &digits {
        write!(&mut number_calc_string, "{}", digit).unwrap();
    }

    // interpret string as number
    let number_calc: BigUint = number_calc_string.parse().unwrap();
    let ninety_seven = BigUint::from(97u8);
    let modulo: u8 = (number_calc % ninety_seven).try_into().unwrap();
    let check_number = 98 - modulo;

    Ok(format!("{:02}", check_number))
}

fn check_digit_ean(check_digit_input: &str) -> Result<String, Error> {
    const EXPECTED_FORMAT_EAN: &str = "0...#";

    // 1. assign the digits indexes from right to left starting at 0
    // 2. total_sum = 0
    // 3. for each digit
    //   a. value = if the index is divisible by 2, 3*digit, otherwise digit
    //   b. total_sum = total_sum + value
    // 4. check_digit = (10 - (total_sum mod 10)) mod 10

    if !is_only_ascii_digits_and_placeholders(check_digit_input) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_EAN),
        });
    }

    let mut digits = extract_ascii_digits_and_placeholders(check_digit_input);
    if digits.last() == Some(&0xFF) {
        digits.pop();
    }
    if digits.len() == 0 || digits.contains(&0xFF) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_EAN),
        });
    }
    digits.reverse();

    let mut multiply_by_three = true;
    let mut total_sum = 0u8;
    for digit in digits {
        let multiplied = if multiply_by_three {
            digit * 3
        } else {
            digit
        };

        total_sum = (total_sum + multiplied) % 10;

        multiply_by_three = !multiply_by_three;
    }

    let check_digit = (10 - total_sum) % 10;
    Ok(format!("{}", check_digit))
}

fn check_digit_isbn10(check_digit_input: &str) -> Result<String, Error> {
    const EXPECTED_FORMAT_ISBN10: &str = "000000000#";

    // 1. assign the non-check digits weights from right to left starting at 2
    // 2. total_sum = 0
    // 3. for each digit
    //   a. value = digit * digit_weight
    //   b. total_sum = total_sum + value
    // 4. check_digit = (11 - (total_sum mod 11)) mod 11

    if !is_only_ascii_digits_and_placeholders(check_digit_input) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_ISBN10),
        });
    }

    let mut digits = extract_ascii_digits_and_placeholders(check_digit_input);
    if digits.last() == Some(&0xFF) {
        digits.pop();
    }
    if digits.len() == 0 || digits.contains(&0xFF) {
        return Err(Error::InvalidInputFormat {
            expected_format: Cow::Borrowed(EXPECTED_FORMAT_ISBN10),
        });
    }
    digits.reverse();

    let mut total_sum = 0u8;
    let mut weight = 2;
    for digit in digits {
        let value = digit * weight;
        weight += 1;
        total_sum = (total_sum + value) % 11;
    }

    let check_digit = (11 - total_sum) % 11;
    if check_digit == 10 {
        Ok("X".to_owned())
    } else {
        Ok(format!("{}", check_digit))
    }
}


pub struct CheckDigitPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
impl CheckDigitPlugin {
    async fn channel_command_checkdigit(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.args.len() != 1 {
            return;
        }
        let cd_type = &command.args[0];

        let check_digit_input = command.rest.trim();

        let check_digit = match cd_type.as_str() {
            "luhn"|"creditcard"|"cc"|"uic" => {
                check_digit_luhn(check_digit_input)
            },
            "atsvnr" => {
                check_digit_atsvnr(check_digit_input)
            },
            "czrodc" => {
                check_digit_czrodc(check_digit_input)
            },
            "iban" => {
                check_digit_iban(check_digit_input)
            },
            "ean"|"isbn13" => {
                check_digit_ean(check_digit_input)
            },
            "isbn10" => {
                check_digit_isbn10(check_digit_input)
            },
            _ => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Unknown check digit type.",
                ).await;
                return;
            }
        };

        match check_digit {
            Ok(cd) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &cd,
                ).await;
            },
            Err(Error::InvalidInputFormat { expected_format }) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Invalid format for this type of check digit; expected: `{}`", expected_format),
                ).await;
            },
            Err(Error::ForbiddenInputValue { theoretical_check_digit }) => {
                match theoretical_check_digit {
                    Some(tcd) => {
                        send_channel_message!(
                            interface,
                            &channel_message.channel.name,
                            &format!("Invalid value for this type of check digit, but in theory, the check digit would be: {}", tcd),
                        ).await;
                    },
                    None => {
                        send_channel_message!(
                            interface,
                            &channel_message.channel.name,
                            "Invalid value for this type of check digit.",
                        ).await;
                    },
                }
            },
        }
    }
}
#[async_trait]
impl RocketBotPlugin for CheckDigitPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "checkdigit",
                "checkdigit",
                "{cpfx}checkdigit TYPE VALUE",
                "Calculates a check digit of the given type for the given value.",
            )
                .arg_count(1)
                .build()
        ).await;

        Self {
            interface,
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "checkdigit" {
            self.channel_command_checkdigit(channel_message, command).await
        }
    }

    async fn plugin_name(&self) -> String {
        "checkdigit".to_owned()
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "checkdigit" {
            Some(include_str!("../help/checkdigit.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // not much to reload
        true
    }
}


#[cfg(test)]
mod tests {
    use super::{
        check_digit_atsvnr, check_digit_czrodc, check_digit_ean, check_digit_iban,
        check_digit_isbn10, check_digit_luhn,
        Error,
    };

    #[test]
    fn test_check_digit_luhn() {
        // VISA test number
        assert_eq!(&check_digit_luhn("411111111111111").unwrap(), "1");
        assert_eq!(&check_digit_luhn("411111111111111#").unwrap(), "1");

        // MasterCard test number
        assert_eq!(&check_digit_luhn("555555555555444").unwrap(), "4");
        assert_eq!(&check_digit_luhn("555555555555444#").unwrap(), "4");

        // UIC: first carriage of first ÖBB Railjet
        assert_eq!(&check_digit_luhn("73 81 84-90 101-").unwrap(), "0");
        assert_eq!(&check_digit_luhn("73 81 84-90 101-#").unwrap(), "0");
    }

    #[test]
    fn test_check_digit_atsvnr() {
        // example from Austrian social security website
        assert_eq!(&check_digit_atsvnr("782# 28 07 55").unwrap(), "9");
        assert_eq!(&check_digit_atsvnr("782#280755").unwrap(), "9");
    }

    #[test]
    fn test_check_digit_czrodc() {
        // manually generated examples
        assert_eq!(&check_digit_czrodc("010101/001#").unwrap(), "9");
        assert_eq!(&check_digit_czrodc("010101/001").unwrap(), "9");
        assert_eq!(&check_digit_czrodc("010101001#").unwrap(), "9");
        assert_eq!(&check_digit_czrodc("010101001").unwrap(), "9");

        assert_eq!(&check_digit_czrodc("101010/001#").unwrap(), "4");
        assert_eq!(&check_digit_czrodc("101010/001").unwrap(), "4");
        assert_eq!(&check_digit_czrodc("101010001#").unwrap(), "4");
        assert_eq!(&check_digit_czrodc("101010001").unwrap(), "4");

        assert_eq!(check_digit_czrodc("101010/007#").unwrap_err(), Error::ForbiddenInputValue { theoretical_check_digit: Some("0".to_owned()) });
        assert_eq!(check_digit_czrodc("101010/007").unwrap_err(), Error::ForbiddenInputValue { theoretical_check_digit: Some("0".to_owned()) });
        assert_eq!(check_digit_czrodc("101010007#").unwrap_err(), Error::ForbiddenInputValue { theoretical_check_digit: Some("0".to_owned()) });
        assert_eq!(check_digit_czrodc("101010007").unwrap_err(), Error::ForbiddenInputValue { theoretical_check_digit: Some("0".to_owned()) });
    }

    #[test]
    fn test_check_digit_iban() {
        // Wikipedia example
        assert_eq!(&check_digit_iban("GB## WEST 1234 5698 7654 32").unwrap(), "82");
        assert_eq!(&check_digit_iban("GB##WEST12345698765432").unwrap(), "82");

        // Austrian Anti-Fraud Office
        assert_eq!(&check_digit_iban("AT## 0100 0000 0550 4374").unwrap(), "09");
        assert_eq!(&check_digit_iban("AT##0100000005504374").unwrap(), "09");
    }

    #[test]
    fn test_check_digit_ean() {
        // Wikipedia examples
        assert_eq!(&check_digit_ean("4 006381 33393#").unwrap(), "1");
        assert_eq!(&check_digit_ean("400638133393#").unwrap(), "1");

        assert_eq!(&check_digit_ean("7351 353#").unwrap(), "7");
        assert_eq!(&check_digit_ean("7351353#").unwrap(), "7");
    }

    #[test]
    fn test_check_digit_isbn10() {
        // numerically first ISBNs
        assert_eq!(&check_digit_isbn10("000000001#").unwrap(), "9");
        assert_eq!(&check_digit_isbn10("000000002#").unwrap(), "7");
    }
}
