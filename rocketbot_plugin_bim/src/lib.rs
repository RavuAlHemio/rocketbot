use std::collections::HashMap;
use std::fmt::Write;
use std::fs::File;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde::{Deserialize, Serialize};
use serde_json;


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehicleInfo {
    pub number: u32,
    pub type_code: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub other_data: HashMap<String, String>,
}
impl VehicleInfo {
    pub fn new(number: u32, type_code: String) -> Self {
        Self {
            number,
            type_code,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: HashMap::new(),
        }
    }
}


pub struct BimPlugin {
    interface: Weak<dyn RocketBotInterface>,
    bim_database_path: String,
    company_mapping: HashMap<String, String>,
}
impl BimPlugin {
    fn load_database(&self) -> Option<HashMap<u32, VehicleInfo>> {
        let f = match File::open(&self.bim_database_path) {
            Ok(f) => f,
            Err(e) => {
                error!("failed to open bim database: {}", e);
                return None;
            },
        };
        let mut vehicles: Vec<VehicleInfo> = match serde_json::from_reader(f) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse bim database: {}", e);
                return None;
            },
        };
        let vehicle_hash_map: HashMap<u32, VehicleInfo> = vehicles.drain(..)
            .map(|vi| (vi.number, vi))
            .collect();
        Some(vehicle_hash_map)
    }
}
#[async_trait]
impl RocketBotPlugin for BimPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let bim_database_path = config["bim_database_path"]
            .as_str().expect("bim_database_path is not a string")
            .to_owned();

        let company_mapping = if config["company_mapping"].is_null() {
            HashMap::new()
        } else {
            let mut mapping = HashMap::new();
            for (k, v) in config["company_mapping"].entries().expect("company_mapping not an object") {
                let v_str = v.as_str().expect("company_mapping value not a string");
                mapping.insert(k.to_owned(), v_str.to_owned());
            }
            mapping
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "bim".to_owned(),
                "bim".to_owned(),
                "{cpfx}bim NUMBER".to_owned(),
                "Obtains information about the public transport vehicle with the given number.".to_owned(),
            )
                .build()
        ).await;

        Self {
            interface,
            bim_database_path,
            company_mapping,
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let number_str = command.rest.trim();
        let number: u32 = match number_str.parse() {
            Ok(n) => n,
            Err(e) => {
                error!("failed to parse {:?} as u32: {}", number_str, e);
                return;
            },
        };

        let database = match self.load_database() {
            Some(db) => db,
            None => {
                return;
            },
        };
        let vehicle = match database.get(&number) {
            Some(v) => v,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Vehicle {} not found.", number),
                ).await;
                return;
            },
        };

        let mut response = format!(
            "*{}*: type *{}*",
            vehicle.number, vehicle.type_code,
        );
        match (&vehicle.in_service_since, &vehicle.out_of_service_since) {
            (Some(service_from), Some(service_to)) => {
                write!(response, ", in service from {} to {}", service_from, service_to).expect("failed to write");
            },
            (Some(service_from), None) => {
                write!(response, ", in service since {}", service_from).expect("failed to write");
            },
            (None, Some(service_to)) => {
                write!(response, ", in service until {}", service_to).expect("failed to write");
            },
            (None, None) => {},
        };

        if let Some(manuf) = &vehicle.manufacturer {
            let full_manuf = self.company_mapping.get(manuf).unwrap_or(manuf);
            write!(response, "\n*hergestellt von* {}", full_manuf).expect("failed to write");
        }

        let mut other_props: Vec<(&str, &str)> = vehicle.other_data.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        other_props.sort_unstable();
        for (key, val) in other_props {
            write!(response, "\n*{}*: {}", key, val).expect("failed to write");
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn plugin_name(&self) -> String {
        "bim".to_owned()
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "bim" {
            Some(include_str!("../help/bim.md").to_owned())
        } else {
            None
        }
    }
}
