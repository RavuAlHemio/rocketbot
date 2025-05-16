pub mod ean13;
pub mod vaxcert;


use std::fs::File;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_barcode::bitmap::{BitmapRenderOptions, LinearBitmap};
use rocketbot_barcode::datamatrix::datamatrix_string_to_bitmap;
use rocketbot_barcode::qr::qr_string_to_bitmap;
use rocketbot_interface::{send_channel_message, ResultExtensions};
use rocketbot_interface::commands::{
    CommandDefinitionBuilder, CommandInstance, CommandValueType,
};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachment};
use rocketbot_interface::sync::RwLock;
use tracing::{debug, error};

use crate::ean13::Digit;
use crate::vaxcert::{encode_vax, make_vax_pdf, normalize_name, PdfSettings, VaxInfo};


static CERT_ID_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const CERT_ID_LENGTH: usize = 26;


fn generate_cert_id(country: &str) -> String {
    let mut rng = StdRng::from_entropy();
    let alphabet: Vec<char> = CERT_ID_ALPHABET.chars().collect();

    let mut letters = String::with_capacity(CERT_ID_LENGTH);
    for _ in 0..CERT_ID_LENGTH {
        let index = rng.gen_range(0..alphabet.len());
        letters.push(alphabet[index]);
    }

    format!("URN:UVCI:01:{}:{}#{}", country, &letters[0..CERT_ID_LENGTH-1], &letters[CERT_ID_LENGTH-1..])
}


#[derive(Clone, Debug, PartialEq)]
struct Config {
    vax_pdf_settings: Option<PdfSettings>,
}


pub struct BarcodePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl BarcodePlugin {
    async fn handle_datamatrix(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let bitmap = match datamatrix_string_to_bitmap(&command.rest) {
            Ok(b) => b,
            Err(e) => {
                error!("error rendering Data Matrix barcode for {:?}: {}", command.rest, e);
                return;
            },
        };
        let png = match bitmap.render(&BitmapRenderOptions::new()).to_png() {
            Ok(p) => p,
            Err(e) => {
                error!("error converting Data Matrix bitmap for {:?} to PNG: {}", command.rest, e);
                return;
            },
        };

        // send it as a response
        interface.send_channel_message_with_attachment(
            &channel_message.channel.name,
            OutgoingMessageWithAttachment::new(
                Attachment::new(
                    png,
                    "datamatrix.png".to_owned(),
                    "image/png".to_owned(),
                    None,
                ),
                None,
                None,
            )
        ).await;
    }

    async fn handle_ean13(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let digit_string = command.rest.replace(" ", "");
        if digit_string.chars().any(|c| c < '0' || c > '9') {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "The number may only consist of digits from 0 to 9.",
            ).await;
            return;
        }
        if digit_string.len() < 12 || digit_string.len() > 13 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "You must supply 12 or 13 digits (i.e. an EAN-13 with or without a check digit) to encode an EAN-13 barcode.",
            ).await;
            return;
        }

        let mut digits = [Digit::default(); 13];
        for (digit, digit_char) in digits.iter_mut().zip(digit_string.chars()) {
            let digit_u8 = ((digit_char as u32) - ('0' as u32)).try_into().unwrap();
            *digit = Digit::try_from_u8(digit_u8).unwrap();
        }

        if digit_string.len() == 12 {
            // calculate check digit
            let mut initial_digits = [Digit::default(); 12];
            initial_digits.copy_from_slice(&digits[0..12]);
            let check_digit = crate::ean13::calculate_check_digit(initial_digits);
            digits[12] = check_digit;
        }

        let bars = crate::ean13::encode_ean_13(digits);
        let barcode = LinearBitmap::new(bars.to_vec());
        let bitmap = barcode.to_bitmap(32);
        let png = match bitmap.render(&BitmapRenderOptions::new()).to_png() {
            Ok(p) => p,
            Err(e) => {
                error!("error converting EAN-13 bitmap for {:?} to PNG: {}", command.rest, e);
                return;
            },
        };

        // send it as a response
        interface.send_channel_message_with_attachment(
            &channel_message.channel.name,
            OutgoingMessageWithAttachment::new(
                Attachment::new(
                    png,
                    "ean13.png".to_owned(),
                    "image/png".to_owned(),
                    None,
                ),
                None,
                None,
            )
        ).await;
    }

    async fn handle_vaxcert(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let cmd_config = interface.get_command_configuration().await;
        let sop = &cmd_config.short_option_prefix;
        let lop = &cmd_config.long_option_prefix;

        // alrighty then, command-line parsing time
        let mut missing_options = Vec::new();

        let first_name_opt = command.options.get("f")
            .or_else(|| command.options.get("first-name"))
            .map(|v| v.as_str().unwrap().to_owned());
        if first_name_opt.is_none() {
            missing_options.push(format!("`{}f`/`{}first-name`", sop, lop));
        }

        let last_name_opt = command.options.get("l")
            .or_else(|| command.options.get("last-name"))
            .map(|v| v.as_str().unwrap().to_owned());
        if last_name_opt.is_none() {
            missing_options.push(format!("`{}l`/`{}last-name`", sop, lop));
        }

        let birthdate_str_opt = command.options.get("b")
            .or_else(|| command.options.get("birthdate"))
            .map(|v| v.as_str().unwrap().to_owned());
        if birthdate_str_opt.is_none() {
            missing_options.push(format!("`{}b`/`{}birthdate`", sop, lop));
        }

        let issue_date_str_opt = command.options.get("d")
            .or_else(|| command.options.get("issue-date"))
            .map(|v| v.as_str().unwrap().to_owned());
        if issue_date_str_opt.is_none() {
            missing_options.push(format!("`{}d`/`{}issue-date`", sop, lop));
        }

        if missing_options.len() > 0 {
            let mut output_message = "The following required options are missing:\n".to_owned();
            output_message.push_str(&missing_options.join("\n"));
            interface.send_channel_message(
                &channel_message.channel.name,
                &output_message,
            ).await;
            return;
        }

        let first_name = first_name_opt.unwrap();
        let last_name = last_name_opt.unwrap();
        let birthdate_str = birthdate_str_opt.unwrap();
        let issue_date_str = issue_date_str_opt.unwrap();

        let country = command.options.get("c")
            .or_else(|| command.options.get("country"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| "AT".to_owned());

        let country_name_de = command.options.get("C")
            .or_else(|| command.options.get("country-name-de"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| "\u{D6}sterreich".to_owned());

        let country_name_en = command.options.get("E")
            .or_else(|| command.options.get("country-name-en"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| "Austria".to_owned());

        let issuer = command.options.get("I")
            .or_else(|| command.options.get("issuer"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| "Ministry of Health".to_owned());

        let dose_number = command.options.get("n")
            .or_else(|| command.options.get("dose-number"))
            .map(|v| usize::try_from(v.as_i64().unwrap()).ok())
            .flatten()
            .unwrap_or(2);

        let total_doses = command.options.get("N")
            .or_else(|| command.options.get("total-doses"))
            .map(|v| usize::try_from(v.as_i64().unwrap()).ok())
            .flatten()
            .unwrap_or(2);

        let cert_id = command.options.get("i")
            .or_else(|| command.options.get("cert-id"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| generate_cert_id(&country));

        let norm_first_name = command.options.get("F")
            .or_else(|| command.options.get("normalized-first-name"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| normalize_name(&first_name));

        let norm_last_name = command.options.get("L")
            .or_else(|| command.options.get("normalized-last-name"))
            .map(|v| v.as_str().unwrap().to_owned())
            .unwrap_or_else(|| normalize_name(&last_name));

        let valid_date_str_opt = command.options.get("v")
            .or_else(|| command.options.get("valid-date"))
            .map(|v| v.as_str().unwrap().to_owned());

        // attempt to parse the dates
        let birthdate = match NaiveDate::parse_from_str(&birthdate_str, "%Y-%m-%d") {
            Ok(bd) => bd,
            Err(_) => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    "Failed to parse birthdate as YYYY-MM-DD.",
                ).await;
                return;
            },
        };
        let issue_date = match NaiveDate::parse_from_str(&issue_date_str, "%Y-%m-%d") {
            Ok(bd) => Utc.from_utc_datetime(&bd.and_hms_opt(13, 37, 23).unwrap()),
            Err(_) => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    "Failed to parse issue date as YYYY-MM-DD.",
                ).await;
                return;
            },
        };
        let valid_date = if let Some(valid_date_str) = valid_date_str_opt {
            match NaiveDate::parse_from_str(&valid_date_str, "%Y-%m-%d") {
                Ok(bd) => Utc.from_utc_datetime(&bd.and_hms_opt(0, 0, 0).unwrap()),
                Err(_) => {
                    interface.send_channel_message(
                        &channel_message.channel.name,
                        "Failed to parse validity end date as YYYY-MM-DD.",
                    ).await;
                    return;
                },
            }
        } else {
            // add 334 days to issuance date and round down to midnight
            Utc.from_utc_datetime(
                &(issue_date + Duration::days(334))
                    .date_naive()
                    .and_hms_opt(0, 0, 0).unwrap()
            )
        };

        // assemble the cert data
        let vax_info = VaxInfo {
            issued: issue_date,
            expires: valid_date,
            issuer,
            country_code: country,
            country_name_de,
            country_name_en,
            dose_number,
            total_doses,
            date_of_birth: birthdate,
            cert_id,
            surname: last_name,
            surname_normalized: norm_last_name,
            given_name: first_name,
            given_name_normalized: norm_first_name,
        };

        if command.flags.contains("p") || command.flags.contains("pdf") {
            let config_guard = self.config.read().await;
            let pdf_settings = match &config_guard.vax_pdf_settings {
                Some(ps) => ps,
                None => {
                    interface.send_channel_message(
                        &channel_message.channel.name,
                        "Cannot render PDF \u{2013} no template configured.",
                    ).await;
                    return;
                },
            };

            let pdf_data = make_vax_pdf(&vax_info, pdf_settings);
            interface.send_channel_message_with_attachment(
                &channel_message.channel.name,
                OutgoingMessageWithAttachment::new(
                    Attachment::new(
                        pdf_data,
                        "vaxcert.pdf".to_owned(),
                        "application/pdf".to_owned(),
                        None,
                    ),
                    None,
                    None,
                )
            ).await;
            return;
        }

        // generate QR data
        let vax_qr_data = encode_vax(&vax_info);
        debug!("vax QR data is {}", vax_qr_data);

        let bitmap = match qr_string_to_bitmap(&vax_qr_data) {
            Ok(b) => b,
            Err(e) => {
                error!("error rendering QR barcode for vax data: {}", e);
                return;
            },
        };
        let png = match bitmap.render(&BitmapRenderOptions::new()).to_png() {
            Ok(p) => p,
            Err(e) => {
                error!("error converting QR bitmap for vax data to PNG: {}", e);
                return;
            },
        };

        // send it as a response
        interface.send_channel_message_with_attachment(
            &channel_message.channel.name,
            OutgoingMessageWithAttachment::new(
                Attachment::new(
                    png,
                    "vaxcert.png".to_owned(),
                    "image/png".to_owned(),
                    None,
                ),
                None,
                None,
            )
        ).await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let vax_pdf_settings = if config["vax_pdf_settings"].is_null() {
            None
        } else {
            // deserialize
            let vax_pdf_file_name = config["vax_pdf_settings"]
                .as_str().ok_or("vax_pdf_settings not a string")?;
            let vax_pdf_file = File::open(vax_pdf_file_name)
                .or_msg("failed to open vax_pdf_settings file")?;
            let pdf_settings = serde_json::from_reader(vax_pdf_file)
                .or_msg("failed to deserialize vax_pdf_settings")?;
            Some(pdf_settings)
        };

        Ok(Config {
            vax_pdf_settings,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for BarcodePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "BarcodePlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "datamatrix",
                "barcode",
                "{cpfx}datamatrix TEXT",
                "Encodes the given text into a Data Matrix barcode.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "ean13",
                "barcode",
                "{cpfx}ean13 NUMBER",
                "Encodes the given number into an EAN-13 barcode.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "vaxcert",
                "barcode",
                "{cpfx}vaxcert OPTIONS",
                "Generates a vaccination certificate QR code.",
            )
                // flags
                .add_flag("p")
                .add_flag("pdf")
                // required options
                .add_option("f", CommandValueType::String)
                .add_option("first-name", CommandValueType::String)
                .add_option("l", CommandValueType::String)
                .add_option("last-name", CommandValueType::String)
                .add_option("b", CommandValueType::String)
                .add_option("birthdate", CommandValueType::String)
                .add_option("d", CommandValueType::String)
                .add_option("issue-date", CommandValueType::String)
                // optional options
                .add_option("c", CommandValueType::String)
                .add_option("country", CommandValueType::String)
                .add_option("C", CommandValueType::String)
                .add_option("country-name-de", CommandValueType::String)
                .add_option("E", CommandValueType::String)
                .add_option("country-name-en", CommandValueType::String)
                .add_option("I", CommandValueType::String)
                .add_option("issuer", CommandValueType::String)
                .add_option("n", CommandValueType::Integer)
                .add_option("dose-number", CommandValueType::Integer)
                .add_option("N", CommandValueType::Integer)
                .add_option("total-doses", CommandValueType::Integer)
                .add_option("i", CommandValueType::String)
                .add_option("cert-id", CommandValueType::String)
                .add_option("F", CommandValueType::String)
                .add_option("normalized-first-name", CommandValueType::String)
                .add_option("L", CommandValueType::String)
                .add_option("normalized-last-name", CommandValueType::String)
                .add_option("v", CommandValueType::String)
                .add_option("valid-date", CommandValueType::String)
                .build()
        ).await;

        BarcodePlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "barcode".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "datamatrix" {
            self.handle_datamatrix(channel_message, command).await;
        } else if command.name == "ean13" {
            self.handle_ean13(channel_message, command).await;
        } else if command.name == "vaxcert" {
            self.handle_vaxcert(channel_message, command).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "datamatrix" {
            Some(include_str!("../help/datamatrix.md").to_owned())
        } else if command_name == "ean13" {
            Some(include_str!("../help/ean13.md").to_owned())
        } else if command_name == "vaxcert" {
            Some(include_str!("../help/vaxcert.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;
                *config_guard = c;
                true
            },
            Err(e) => {
                error!("failed to reload configuration: {}", e);
                false
            },
        }
    }
}
