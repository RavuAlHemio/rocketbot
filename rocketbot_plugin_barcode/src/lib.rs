pub mod bitmap;
pub mod datamatrix;
pub mod qr;
pub mod vaxcert;


use std::convert::TryFrom;
use std::fmt;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use log::{debug, error};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::commands::{
    CommandDefinitionBuilder, CommandInstance, CommandValueType,
};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachment};

use crate::bitmap::{BitmapError, BitmapRenderOptions};
use crate::datamatrix::datamatrix_string_to_bitmap;
use crate::qr::qr_string_to_bitmap;
use crate::vaxcert::{encode_vax, normalize_name, VaxInfo};


#[derive(Debug)]
pub enum BarcodeError {
    DataMatrixEncoding(::datamatrix::data::DataEncodingError),
    QrEncoding(qrcode::types::QrError),
    Bitmap(BitmapError),
}
impl fmt::Display for BarcodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataMatrixEncoding(e)
                => write!(f, "Data Matrix encoding error: {:?}", e),
            Self::QrEncoding(e)
                => write!(f, "QR encoding error: {:?}", e),
            Self::Bitmap(e)
                => write!(f, "{}", e),
        }
    }
}
impl std::error::Error for BarcodeError {
}


static CERT_ID_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const CERT_ID_LENGTH: usize = 25;


fn generate_cert_id(country: &str) -> String {
    let mut rng = StdRng::from_entropy();
    let alphabet: Vec<char> = CERT_ID_ALPHABET.chars().collect();

    let mut letters = String::with_capacity(CERT_ID_LENGTH);
    for _ in 0..CERT_ID_LENGTH {
        let index = rng.gen_range(0..alphabet.len());
        letters.push(alphabet[index]);
    }

    format!("URN:UVCI:01:{}:{}#C", country, letters)
}


pub struct BarcodePlugin {
    interface: Weak<dyn RocketBotInterface>,
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
            missing_options.push(format!("{}f/{}first-name", sop, lop));
        }

        let last_name_opt = command.options.get("l")
            .or_else(|| command.options.get("last-name"))
            .map(|v| v.as_str().unwrap().to_owned());
        if last_name_opt.is_none() {
            missing_options.push(format!("{}l/{}last-name", sop, lop));
        }

        let birthdate_str_opt = command.options.get("b")
            .or_else(|| command.options.get("birthdate"))
            .map(|v| v.as_str().unwrap().to_owned());
        if birthdate_str_opt.is_none() {
            missing_options.push(format!("{}b/{}birthdate", sop, lop));
        }

        let issue_date_str_opt = command.options.get("d")
            .or_else(|| command.options.get("issue-date"))
            .map(|v| v.as_str().unwrap().to_owned());
        if issue_date_str_opt.is_none() {
            missing_options.push(format!("{}d/{}issue-date", sop, lop));
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
            Ok(bd) => Utc.from_utc_date(&bd),
            Err(_) => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    "Failed to parse birthdate as YYYY-MM-DD.",
                ).await;
                return;
            },
        };
        let issue_date = match NaiveDate::parse_from_str(&issue_date_str, "%Y-%m-%d") {
            Ok(bd) => Utc.from_utc_datetime(&bd.and_hms(13, 37, 23)),
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
                Ok(bd) => Utc.from_utc_datetime(&bd.and_hms(0, 0, 0)),
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
            (issue_date + Duration::days(334)).date().and_hms(0, 0, 0)
        };

        // assemble the cert data
        let vax_info = VaxInfo {
            issued: issue_date,
            expires: valid_date,
            issuer,
            country_code: country,
            dose_number,
            total_doses,
            date_of_birth: birthdate,
            cert_id,
            surname: last_name,
            surname_normalized: norm_last_name,
            given_name: first_name,
            given_name_normalized: norm_first_name,
        };

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
}
#[async_trait]
impl RocketBotPlugin for BarcodePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "datamatrix".to_owned(),
                "barcode".to_owned(),
                "{cpfx}datamatrix TEXT".to_owned(),
                "Encodes the given text into a Data Matrix barcode.".to_owned(),
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "vaxcert".to_owned(),
                "barcode".to_owned(),
                "{cpfx}vaxcert OPTIONS".to_owned(),
                "Generates a vaccination certificate QR code.".to_owned(),
            )
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
        }
    }

    async fn plugin_name(&self) -> String {
        "barcode".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "datamatrix" {
            self.handle_datamatrix(channel_message, command).await;
        } else if command.name == "vaxcert" {
            self.handle_vaxcert(channel_message, command).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "datamatrix" {
            Some(include_str!("../help/datamatrix.md").to_owned())
        } else if command_name == "vaxcert" {
                Some(include_str!("../help/vaxcert.md").to_owned())
        } else {
            None
        }
    }
}
