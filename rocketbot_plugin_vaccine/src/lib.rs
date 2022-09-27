mod database;


use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use log::error;
use num_bigint::{BigInt, BigUint};
use num_traits::ToPrimitive;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;

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

fn format_stat(dose_text: &str, value: &BigUint, delta: &BigInt, percentage: f64, percent_points_delta: f64) -> String {
    let mut value_str = value.to_string();
    with_thou_sep(&mut value_str, 3, ',');

    let mut delta_str = delta.to_string();
    if delta_str.starts_with('-') {
        delta_str.remove(0);
        with_thou_sep(&mut delta_str, 3, ',');
        delta_str.insert(0, '-');
    } else {
        with_thou_sep(&mut delta_str, 3, ',');
        delta_str.insert(0, '+');
    }

    format!(
        "vaccine {}: {} ({:.2}%, {}, {:+.3}%pt)",
        dose_text, value_str, percentage, delta_str, percent_points_delta,
    )
}


#[derive(Clone, Debug, PartialEq)]
struct ExtractedVaccineStats {
    pub freshest_entries: Vec<(NaiveDate, VaccinationStats)>,
    pub freshest_date: NaiveDate,
    pub delta: VaccinationStats,
    pub dose_to_percent: HashMap<String, f64>,
    pub dose_to_delta_percent_points: HashMap<String, f64>,
    pub vax_cert: BigUint,
    pub vax_cert_delta: BigInt,
    pub vax_cert_percent: f64,
    pub vax_cert_delta_percent_points: f64,
    pub population: BigUint,
    pub state_name: String,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Config {
    default_target: String,
    doses_timeline_url: String,
    vax_certs_url: String,
    prev_vax_certs_url_format: String,
    max_age_h: i64,
}


pub struct VaccinePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    vaccine_database: RwLock<VaccineDatabase>,
}
impl VaccinePlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let default_target = config["default_target"]
            .as_str().ok_or("default_target missing or not a string")?
            .to_owned();
        let doses_timeline_url = config["doses_timeline_url"]
            .as_str().ok_or("doses_timeline_url missing or not a string")?
            .to_owned();
        let vax_certs_url = config["vax_certs_url"]
            .as_str().ok_or("vax_certs_url missing or not a string")?
            .to_owned();
        let prev_vax_certs_url_format = config["prev_vax_certs_url_format"]
            .as_str().ok_or("prev_vax_certs_url_format missing or not a string")?
            .to_owned();
        let max_age_h = config["max_age_h"]
            .as_i64().ok_or("max_age_h missing or not an i64")?;

        Ok(Config {
            default_target,
            doses_timeline_url,
            vax_certs_url,
            prev_vax_certs_url_format,
            max_age_h,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for VaccinePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        let vaccine_database = RwLock::new(
            "VaccinePlugin::vaccine_database",
            match VaccineDatabase::new_from_urls(&config_object.doses_timeline_url, &config_object.vax_certs_url, &config_object.prev_vax_certs_url_format).await {
                Ok(d) => d,
                Err(e) => {
                    panic!("initial database population failed: {}", e);
                },
            },
        );

        let config_lock = RwLock::new(
            "VaccinePlugin::config",
            config_object,
        );

        let vaccine_command = CommandDefinitionBuilder::new(
            "vaccine",
            "vaccine",
            "{cpfx}vaccine [STATE]",
            "Displays the number of vaccinated people in the given Austrian state or for all of Austria.",
        )
            .build();
        my_interface.register_channel_command(&vaccine_command).await;

        VaccinePlugin {
            interface,
            config: config_lock,
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

        let config_guard = self.config.read().await;

        let rest_trim = command.rest.trim();
        let name = if rest_trim.len() > 0 {
            rest_trim
        } else {
            &config_guard.default_target
        };
        let name_lower = name.to_lowercase();

        let update_delta = {
            let db_guard = self.vaccine_database
                .read().await;
            Utc::now() - db_guard.corona_timestamp
        };
        if update_delta.num_hours() > config_guard.max_age_h {
            match VaccineDatabase::new_from_urls(&config_guard.doses_timeline_url, &config_guard.vax_certs_url, &config_guard.prev_vax_certs_url_format).await {
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

        let evs = {
            let db_guard = self.vaccine_database
                .read().await;
            let state_id = match db_guard.cert_database.lower_name_to_state_id.get(&name_lower) {
                Some(s) => s,
                None => {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        &format!(
                            "@{} was ist das f\u{FC}r 1 Bundesland?",
                            channel_message.message.sender.username,
                        ),
                    ).await;
                    return;
                },
            };
            let state_name = db_guard.cert_database.state_id_to_name.get(state_id)
                .expect("no state name found by ID")
                .clone();

            let mut freshest_entries: Vec<(NaiveDate, VaccinationStats)> = db_guard.state_id_and_date_to_fields
                .iter()
                .filter(|((state, _date), _stats)| state == state_id)
                .map(|((_state, date), stats)| (date.clone(), stats.clone()))
                .collect();
            freshest_entries.sort_unstable_by_key(|(date, _stats)| *date);
            freshest_entries.reverse();
            while freshest_entries.len() > 2 {
                freshest_entries.remove(freshest_entries.len() - 1);
            }
            let pop = db_guard.cert_database.state_id_to_pop[state_id].clone();

            let freshest_date = freshest_entries[0].0.clone();

            let mut actual_entries: Vec<VaccinationStats> = freshest_entries.iter()
                .map(|(_date, stats)| (*stats).clone())
                .collect();
            if actual_entries.len() == 0 {
                // nothing to show
                return;
            } else if actual_entries.len() == 1 {
                let only_entry = &actual_entries[0];
                let dose_to_zero_count: HashMap<String, BigUint> = only_entry.dose_to_count.keys()
                    .map(|d| (d.clone(), BigUint::from(0u32)))
                    .collect();
                actual_entries.push(VaccinationStats {
                    dose_to_count: dose_to_zero_count,
                });
            }

            let zero = BigUint::from(0u32);
            let mut delta_dose_to_count = HashMap::new();
            for (dose_number, dose_count) in actual_entries[0].dose_to_count.iter() {
                let prev_count = actual_entries[1].dose_to_count.get(dose_number)
                    .unwrap_or(&zero);
                if prev_count <= dose_count {
                    delta_dose_to_count.insert(dose_number.clone(), dose_count - prev_count);
                }
            }
            let delta = VaccinationStats {
                dose_to_count: delta_dose_to_count,
            };

            let mut dose_to_percent = HashMap::new();
            let mut dose_to_delta_percent_points = HashMap::new();
            let hundred_thousand = BigUint::from(100_000u32);
            for (dose_number, dose_count) in actual_entries[0].dose_to_count.iter() {
                let prev_dose_count = actual_entries[1].dose_to_count.get(dose_number)
                    .unwrap_or(&zero);

                let (percent, prev_percent): (f64, f64) = if pop == zero {
                    (f64::INFINITY, f64::INFINITY)
                } else {
                    (
                        (dose_count * &hundred_thousand / &pop).to_f64().expect("BigUint to f64") / 1000.0,
                        (prev_dose_count * &hundred_thousand / &pop).to_f64().expect("BigUint to f64") / 1000.0,
                    )
                };
                dose_to_percent.insert(dose_number.clone(), percent);
                dose_to_delta_percent_points.insert(dose_number.clone(), percent - prev_percent);
            }

            let vax_cert = db_guard.cert_database.state_id_and_date_to_cert_count.iter()
                .filter_map(|((sid, _date), cert_count)| if sid == state_id { Some(cert_count.clone()) } else { None })
                .nth(0).expect("vax cert statistic missing");
            let vax_cert_prev = db_guard.prev_cert_database.state_id_and_date_to_cert_count.iter()
                .filter_map(|((sid, _date), cert_count)| if sid == state_id { Some(cert_count.clone()) } else { None })
                .nth(0).expect("previous vax cert statistic missing");
            let vax_cert_delta = BigInt::from(vax_cert.clone()) - BigInt::from(vax_cert_prev.clone());
            let (vax_cert_percent, vax_cert_prev_percent): (f64, f64) = if pop == zero {
                (f64::INFINITY, f64::INFINITY)
            } else {
                (
                    (&vax_cert * &hundred_thousand / &pop).to_f64().expect("BigUint to f64") / 1000.0,
                    (vax_cert_prev * &hundred_thousand / &pop).to_f64().expect("BigUint to f64") / 1000.0,
                )
            };
            let vax_cert_delta_percent_points = vax_cert_percent - vax_cert_prev_percent;

            ExtractedVaccineStats {
                freshest_entries,
                freshest_date,
                delta,
                dose_to_percent,
                dose_to_delta_percent_points,
                vax_cert,
                vax_cert_delta,
                vax_cert_percent,
                vax_cert_delta_percent_points,
                population: pop,
                state_name,
            }
        };

        let mut pop_string = evs.population.to_string();
        with_thou_sep(&mut pop_string, 3, ',');

        let mut response = String::new();
        response.push_str(&format!(
            "@{} {} ({}), population {}:",
            channel_message.message.sender.username,
            evs.state_name,
            evs.freshest_date.format("%Y-%m-%d"),
            pop_string,
        ));
        let mut dose_numbers: Vec<String> = evs.dose_to_percent.keys()
            .map(|k| k.clone())
            .collect();
        dose_numbers.sort_unstable();
        let dose_texts: Vec<String> = dose_numbers.iter()
            .map(|dose_number| format_stat(
                &dose_number.to_string(),
                &evs.freshest_entries[0].1.dose_to_count[dose_number],
                &BigInt::from(evs.delta.dose_to_count[dose_number].clone()),
                evs.dose_to_percent[dose_number],
                evs.dose_to_delta_percent_points[dose_number],
            ))
            .collect();
        for dose_text in &dose_texts {
            response.push('\n');
            response.push_str(dose_text);
        }
        let vax_cert_text = format_stat(
            "certificates",
            &evs.vax_cert,
            &evs.vax_cert_delta,
            evs.vax_cert_percent,
            evs.vax_cert_delta_percent_points,
        );
        response.push('\n');
        response.push_str(&vax_cert_text);

        send_channel_message!(
            interface,
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

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;
                *config_guard = c;
                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
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
