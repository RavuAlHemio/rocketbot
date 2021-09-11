pub mod datamatrix;


use std::fmt;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Attachment, ChannelMessage, OutgoingMessageWithAttachment};

use crate::datamatrix::datamatrix_string_to_png;


#[derive(Debug)]
pub enum BarcodeError {
    DataMatrixEncoding(::datamatrix::data::DataEncodingError),
    SizeConversion(&'static str, usize, &'static str, std::num::TryFromIntError),
    PngEncoding(png::EncodingError),
}
impl fmt::Display for BarcodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataMatrixEncoding(e)
                => write!(f, "Data Matrix encoding error: {:?}", e),
            Self::SizeConversion(dimension, value, target_type, e)
                => write!(f, "failed to convert {} ({}) to {}: {}", dimension, value, target_type, e),
            Self::PngEncoding(e)
                => write!(f, "PNG encoding error: {}", e),
        }
    }
}
impl std::error::Error for BarcodeError {
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

        let png = match datamatrix_string_to_png(&command.rest) {
            Ok(p) => p,
            Err(e) => {
                error!("error rendering Data Matrix barcode for {:?}: {}", command.rest, e);
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
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "datamatrix" {
            Some(include_str!("../help/datamatrix.md").to_owned())
        } else {
            None
        }
    }
}
