use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use rocketbot_interface::{send_channel_message, write_expect};
use rocketbot_interface::commands::CommandInstance;
use rocketbot_interface::model::ChannelMessage;
use tracing::error;

use crate::{BimPlugin, Config, connect_ride_db};


impl BimPlugin {
    pub(crate) async fn channel_command_bimcompanies(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;

        if config_guard.company_to_definition.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "There are no companies.",
            ).await;
            return;
        }

        let mut country = command.rest.trim();
        if country == "?" {
            // list countries
            let mut countries = BTreeSet::new();
            for company_def in config_guard.company_to_definition.values() {
                countries.insert(format!(":flag_{}: (`{}`)", company_def.country, company_def.country));
            }

            let mut response = "We know of companies in the following countries: ".to_owned();
            let mut first_op = true;
            for country in countries {
                if first_op {
                    first_op = false;
                } else {
                    response.push_str(", ");
                }
                response.push_str(&country);
            }

            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &response,
            ).await;
            return;
        } else if country.len() == 0 {
            // country of the default operator
            let default_operator = config_guard.default_company.as_str();
            let op_def = match config_guard.company_to_definition.get(default_operator) {
                Some(od) => od,
                None => {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "Default company does not have a definition?! This is odd, please bug the administrator(s).",
                    ).await;
                    return;
                }
            };
            country = op_def.country.as_str();
        }

        let mut company_to_name: BTreeMap<&String, &String> = BTreeMap::new();
        for (company_id, company_def) in &config_guard.company_to_definition {
            if company_def.country != country {
                continue;
            }
            company_to_name
                .insert(company_id, &company_def.name);
        }

        if company_to_name.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "We know of no companies in that country...",
            ).await;
            return;
        }

        let mut response_str = format!("The following companies exist in :flag_{}: :", country);
        for (company_abbr, name) in company_to_name {
            write_expect!(&mut response_str, "\n* `{}` ({})", company_abbr, name);
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_str,
        ).await;
    }

    async fn count_rides_with_company(config: &Config, company_key: &str) -> Option<i64> {
        let db_conn = connect_ride_db(config)
            .await.ok()?; // error already output
        let row_opt = db_conn.query_opt("SELECT CAST(COUNT(*) AS bigint) FROM bim.rides WHERE company = $1", &[&company_key])
            .await.ok()?;
        let ride_count: i64 = match row_opt {
            Some(row) => row.get(0),
            None => 0,
        };
        Some(ride_count)
    }

    pub(crate) async fn channel_command_bimcompany(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;

        if config_guard.company_to_definition.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "There are no companies.",
            ).await;
            return;
        }

        let company_key = command.rest.trim();
        if company_key.len() == 0 {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                "You must specify a company.",
            ).await;
            return;
        }

        let company = match config_guard.company_to_definition.get(company_key) {
            Some(c) => c,
            None => {
                error!("cannot find company {:?}", company_key);
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "I know of no such company.",
                ).await;
                return;
            },
        };

        let ride_count_opt = Self::count_rides_with_company(&config_guard, company_key).await;
        let ride_count_message = match ride_count_opt {
            Some(0) => format!("no registered rides with this company yet"),
            Some(n) => format!("{} registered rides with this company", n),
            None => format!("failed to obtain ride count"),
        };

        let company_message = format!(
            concat!(
                "*{}* (`{}`)\n",
                "country: {} :flag_{}:\n",
                "{}",
                "{}",
            ),
            company.name, company_key,
            company.country.to_uppercase(), company.country.to_lowercase(),
            if company.bim_database_path.is_some() { "vehicle database available\n" } else { "" },
            ride_count_message,
        );

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &company_message,
        ).await;
    }
}
