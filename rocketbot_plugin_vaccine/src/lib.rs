mod database;


use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use json::JsonValue;
use log::error;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;

use crate::database::{VaccinationStats, VaccineDatabase};

fn with_thou_sep(int_str: &mut String, group_size: usize, sep: char) {
    if group_size == 0 {
        return;
    }

    let mut pos: usize = int_str.len();
    while pos > group_size {
        pos -= group_size;
        int_str.insert(pos, sep);
    }
}

fn format_stat(value: &BigUint, delta: Option<&BigUint>, percentage: Option<f64>, name: &str) -> String {
    let mut ret = String::new();
    let mut value_str = value.to_string();
    with_thou_sep(&mut value_str, 3, ',');
    ret.push_str(&value_str);

    if let Some(d) = delta {
        let mut delta_str = d.to_string();
        with_thou_sep(&mut delta_str, 3, ',');

        if let Some(p) = percentage {
            ret.push_str(&format!(" ({:.2}%, +{})", p, delta_str));
        } else {
            ret.push_str(&format!(" (+{})", delta_str));
        }
    } else if let Some(p) = percentage {
        ret.push_str(&format!(" ({:.2}%)", p));
    }
    ret.push(' ');
    ret.push_str(name);
    ret
}


pub struct VaccinePlugin {
    interface: Weak<dyn RocketBotInterface>,
    default_target: String,
    vaccine_csv_uri: String,

    vaccine_database: RwLock<VaccineDatabase>,
}
#[async_trait]
impl RocketBotPlugin for VaccinePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let default_target = config["default_target"]
            .as_str().expect("default_target missing or not a string")
            .to_owned();
        let vaccine_csv_uri = config["vaccine_csv_uri"]
            .as_str().expect("vaccine_csv_uri missing or not a string")
            .to_owned();

        let vaccine_database = RwLock::new(
            "VaccinePlugin::vaccine_database",
            match VaccineDatabase::new_from_url(&vaccine_csv_uri).await {
                Ok(d) => d,
                Err(e) => {
                    panic!("initial database population failed: {}", e);
                },
            },
        );

        let vaccine_command = CommandDefinition::new(
            "vaccine".to_owned(),
            "vaccine".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}vaccine [STATE]".to_owned(),
            "Displays the number of vaccinated people in the given Austrian state or for all of Austria.".to_owned(),
        );
        my_interface.register_channel_command(&vaccine_command).await;

        VaccinePlugin {
            interface,
            default_target,
            vaccine_csv_uri,
            vaccine_database,
        }
    }

    async fn plugin_name(&self) -> String {
        "vaccine".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "vaccine" {
            return;
        }

        let rest_trim = command.rest.trim();
        let name = if rest_trim.len() > 0 {
            rest_trim
        } else {
            &self.default_target
        };
        let name_lower = name.to_lowercase();

        let update_delta = {
            let db_guard = self.vaccine_database
                .read().await;
            Utc::now() - db_guard.corona_timestamp
        };
        if update_delta.num_days() > 0 {
            match VaccineDatabase::new_from_url(&self.vaccine_csv_uri).await {
                Ok(d) => {
                    let mut db_guard = self.vaccine_database
                        .write().await;
                    *db_guard = d;
                },
                Err(e) => {
                    error!("failed to obtain updated database: {}", e);
                },
            };
        }

        let (freshest_entries, delta, part_percent, full_percent, state_name) = {
            let db_guard = self.vaccine_database
                .read().await;
            let state_id = match db_guard.lower_name_to_state_id.get(&name_lower) {
                Some(s) => s,
                None => {
                    interface.send_channel_message(
                        &channel_message.channel.name,
                        &format!(
                            "@{} was ist das f\u{FC}r 1 Bundesland?",
                            channel_message.message.sender.username,
                        ),
                    ).await;
                    return;
                },
            };
            let state_name = db_guard.state_id_to_name.get(state_id)
                .expect("no state name found by ID")
                .clone();

            let mut freshest_entries: Vec<(NaiveDate, &VaccinationStats)> = db_guard.state_id_and_date_to_fields
                .iter()
                .filter(|((state, _date), _stats)| state == state_id)
                .map(|((_state, date), stats)| (date.clone(), stats))
                .collect();
            freshest_entries.sort_unstable_by_key(|(date, _stats)| *date);
            freshest_entries.reverse();
            while freshest_entries.len() > 2 {
                freshest_entries.remove(freshest_entries.len() - 1);
            }
            let pop = db_guard.state_id_to_pop[state_id].clone();

            let mut actual_entries: Vec<VaccinationStats> = freshest_entries.iter()
                .map(|(_date, stats)| (*stats).clone())
                .collect();
            while actual_entries.len() < 2 {
                actual_entries.push(VaccinationStats {
                    vaccinations: BigUint::from(0u32),
                    partially_immune: BigUint::from(0u32),
                    fully_immune: BigUint::from(0u32),
                });
            }

            let ten_thousand = BigUint::from(10000u32);
            let part_percent: f64 = (&actual_entries[0].partially_immune * &ten_thousand / &pop).to_f64().expect("BigUint to f64") / 100.0;
            let full_percent: f64 = (&actual_entries[0].fully_immune * &ten_thousand / &pop).to_f64().expect("BigUint to f64") / 100.0;

            let delta = actual_entries[0].clone() - actual_entries[1].clone();

            (actual_entries, delta, part_percent, full_percent, state_name)
        };

        let mut response = String::new();
        response.push_str(&format!("@{} {}: ", channel_message.message.sender.username, state_name));
        response.push_str(&format_stat(
            &freshest_entries[0].vaccinations,
            delta.as_ref().map(|d| &d.partially_immune),
            None,
            "vaccinations",
        ));
        response.push_str(" => ");
        response.push_str(&format_stat(
            &freshest_entries[0].partially_immune,
            delta.as_ref().map(|d| &d.partially_immune),
            Some(part_percent),
            "at least partially",
        ));
        response.push_str(", ");
        response.push_str(&format_stat(
            &freshest_entries[0].fully_immune,
            delta.as_ref().map(|d| &d.fully_immune),
            Some(full_percent),
            "fully immune",
        ));

        interface.send_channel_message(
            &channel_message.channel.name,
            &response,
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "vaccine" {
            Some(include_str!("../help/vaccine.md").to_owned())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn do_test(input: &str, group_size: usize, output: &str) {
        let mut input_string = input.to_owned();
        with_thou_sep(&mut input_string, group_size, ',');
        assert_eq!(&input_string, output);
    }

    #[test]
    fn thou_sep_zero() {
        do_test("123456789", 0, "123456789");
        do_test("12345678", 0, "12345678");
        do_test("1234567", 0, "1234567");
        do_test("123456", 0, "123456");
        do_test("1", 0, "1");
        do_test("", 0, "");
    }

    #[test]
    fn thou_sep_three() {
        do_test("123456789", 3, "123,456,789");
        do_test("12345678", 3, "12,345,678");
        do_test("1234567", 3, "1,234,567");
        do_test("123456", 3, "123,456");
        do_test("1", 3, "1");
        do_test("", 3, "");
    }

    #[test]
    fn thou_sep_two() {
        do_test("123456789", 2, "1,23,45,67,89");
        do_test("12345678", 2, "12,34,56,78");
        do_test("1234567", 2, "1,23,45,67");
        do_test("123456", 2, "12,34,56");
        do_test("1", 2, "1");
        do_test("", 2, "");
    }

    #[test]
    fn thou_sep_one() {
        do_test("123456789", 1, "1,2,3,4,5,6,7,8,9");
        do_test("12345678", 1, "1,2,3,4,5,6,7,8");
        do_test("1234567", 1, "1,2,3,4,5,6,7");
        do_test("123456", 1, "1,2,3,4,5,6");
        do_test("12", 1, "1,2");
        do_test("1", 1, "1");
        do_test("", 1, "");
    }
}