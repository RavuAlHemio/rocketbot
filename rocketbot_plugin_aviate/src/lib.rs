use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::{phrase_join, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_postgres::NoTls;
use tracing::error;


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    db_conn_string: String,
}


pub struct AviatePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl AviatePlugin {
    fn try_load_config(config: serde_json::Value) -> Option<Config> {
        match serde_json::from_value(config) {
            Ok(c) => Some(c),
            Err(e) => {
                error!("failed to interpret configuration: {}", e);
                None
            },
        }
    }

    async fn channel_command_fly(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.rest.trim().len() > 0 {
            // invalid input
            return;
        }

        let config_guard = self.config.read().await;
        let conn = match connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let from_airport_iata = command.args[0].to_uppercase();
        let to_airport_iata = command.args[1].to_uppercase();

        let rows_res = conn.query(
            "
                SELECT
                    r.airline_iata_code,
                    al.name,
                    req.equipment_code
                FROM aviate.routes r
                INNER JOIN aviate.airlines al
                    ON al.iata_code = r.airline_iata_code
                LEFT OUTER JOIN aviate.route_equipment req
                    ON req.route_id = r.id
                WHERE
                    r.from_airport_iata_code = $1
                    AND r.to_airport_iata_code = $2
            ",
            &[&from_airport_iata, &to_airport_iata],
        ).await;
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query routes: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to execute route query. :disappointed:",
                ).await;
                return;
            },
        };
        if rows.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "No route found.",
            ).await;
            return;
        }

        let mut iata_code_to_airline_and_equipment: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
        for row in rows {
            let iata_code: String = row.get(0);
            let name: String = row.get(1);
            let equipment_code: Option<String> = row.get(2);

            let (_, equipment) = iata_code_to_airline_and_equipment
                .entry(iata_code)
                .or_insert_with(|| (name, BTreeSet::new()));
            if let Some(ec) = equipment_code {
                equipment.insert(ec);
            }
        }

        let mut lines = Vec::new();
        for (iata_code, (airline, equipment)) in iata_code_to_airline_and_equipment {
            let mut line = format!("* `{}` ({})", iata_code, airline);
            if equipment.len() > 0 {
                line.push_str(" using ");
                let equipment_vec: Vec<&String> = equipment.iter().collect();
                let equipment_string = phrase_join(&equipment_vec, ", ", " or ");
                line.push_str(&equipment_string);
            }
            lines.push(line);
        }

        let body = lines.join("\n");
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &body,
        ).await;
    }

    async fn channel_command_flyfrom_flyto(&self, channel_message: &ChannelMessage, command: &CommandInstance, fly_from: bool) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let my_airport_iata = command.rest.trim().to_uppercase();
        if my_airport_iata.len() == 0 {
            // invalid input
            return;
        }

        let config_guard = self.config.read().await;
        let conn = match connect_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        // does the airport even exist?
        let my_airport_res = conn.query(
            "
                SELECT name
                FROM aviate.airports
                WHERE iata_code = $1
            ",
            &[&my_airport_iata],
        ).await;
        let my_airport_name = match my_airport_res {
            Ok(r) if r.len() == 0 => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "I don't know that airport. :disappointed:",
                ).await;
                return;
            },
            Ok(r) => {
                let name: String = r[0].get(0);
                name
            },
            Err(e) => {
                error!("failed to query airport {:?}: {}", my_airport_iata, e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to execute airport query. :disappointed:",
                ).await;
                return;
            },
        };

        let route_query_str = if fly_from {
            "
                SELECT DISTINCT
                    r.to_airport_iata_code
                FROM aviate.routes r
                WHERE
                    r.from_airport_iata_code = $1
            "
        } else {
            "
                SELECT DISTINCT
                    r.from_airport_iata_code
                FROM aviate.routes r
                WHERE
                    r.to_airport_iata_code = $1
            "
        };
        let route_rows_res = conn
            .query(route_query_str, &[&my_airport_iata]).await;
        let route_rows = match route_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query routes: {}", e);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to execute route query. :disappointed:",
                ).await;
                return;
            },
        };
        if route_rows.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "No route found.",
            ).await;
            return;
        }

        let mut other_airport_iata_codes: BTreeSet<String> = BTreeSet::new();
        for row in route_rows {
            let other_airport_iata_code: String = row.get(0);
            other_airport_iata_codes.insert(other_airport_iata_code);
        }

        let relation_phrase = if fly_from {
            "reachable from"
        } else {
            "with flights to"
        };

        let mut body = format!("Airports {} `{}` ({}):", relation_phrase, my_airport_iata, my_airport_name);
        for other_code in &other_airport_iata_codes {
            write!(body, " {}", other_code).unwrap();
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &body,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for AviatePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = interface.upgrade()
            .expect("interface is gone?!");
        let config_object = Self::try_load_config(config)
            .expect("configuration loading failed");
        let config_lock = RwLock::new(
            "AviatePlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "fly",
                "aviate",
                "{cpfx}fly FROMIATA TOIATA",
                "Lists which airlines fly between two airports.",
            )
                .arg_count(2)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "flyfrom",
                "aviate",
                "{cpfx}flyfrom FROMIATA",
                "Lists which airports are reachable by flights from a specific airport.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "flyto",
                "aviate",
                "{cpfx}flyto FROMIATA",
                "Lists which airports offer flights to a specific airport.",
            )
                .build()
        ).await;

        AviatePlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "aviate".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "fly" {
            self.channel_command_fly(channel_message, command).await
        } else if command.name == "flyfrom" {
            self.channel_command_flyfrom_flyto(channel_message, command, false).await
        } else if command.name == "flyto" {
            self.channel_command_flyfrom_flyto(channel_message, command, true).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "fly" {
            Some(include_str!("../help/fly.md").to_owned())
        } else if command_name == "flyfrom" {
            Some(include_str!("../help/flyfrom.md").to_owned())
        } else if command_name == "flyto" {
            Some(include_str!("../help/flyto.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let config = match Self::try_load_config(new_config) {
            Some(c) => c,
            None => return false,
        };
        let mut config_guard = self.config.write().await;
        *config_guard = config;
        true
    }
}

async fn connect_db(config: &Config) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
    let (client, connection) = match tokio_postgres::connect(&config.db_conn_string, NoTls).await {
        Ok(cc) => cc,
        Err(e) => {
            error!("error connecting to database: {}", e);
            return Err(e);
        },
    };
    tokio::spawn(async move {
        connection.await
    });
    Ok(client)
}
