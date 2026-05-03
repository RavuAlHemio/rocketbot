use std::collections::HashMap;

use rocketbot_bim_common::VehicleNumber;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::CommandInstance;
use rocketbot_interface::model::ChannelMessage;
use tokio_postgres::types::ToSql;
use tracing::error;

use crate::{BimPlugin, connect_ride_db, LookbackRange};


impl BimPlugin {
    async fn do_topriders_all_vehicles(
        &self,
        channel_name: &str,
        company: Option<&str>,
        lookback_range: LookbackRange,
    ) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let mut query_criteria: Vec<String> = Vec::with_capacity(2);
        let mut query_params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(2);

        let company_holder;
        if let Some(c) = company {
            company_holder = c;
            query_criteria.push(format!("r.company = ${}", query_params.len() + 1));
            query_params.push(&company_holder);
        }

        let timestamp_holder;
        if let Some(lbts) = lookback_range.start_timestamp() {
            timestamp_holder = lbts;
            query_criteria.push(format!("r.\"timestamp\" >= ${}", query_params.len() + 1));
            query_params.push(&timestamp_holder);
        }

        let rides_query = format!(
            "
                SELECT r.rider_username, CAST(COUNT(*) AS bigint) ride_count
                FROM bim.rides r
                {} {}
                GROUP BY r.rider_username
            ",
            if query_criteria.len() > 0 { " WHERE " } else { "" },
            query_criteria.join(" AND "),
        );

        let ride_rows_res = ride_conn.query(
            &rides_query,
            &query_params,
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most active riders: {}", e);
                send_channel_message!(
                    interface,
                    &channel_name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut rider_to_ride_and_vehicle_count = HashMap::new();
        for row in ride_rows {
            let rider_username: String = row.get(0);
            let ride_count: i64 = row.get(1);

            let rider_ride_and_vehicle_count = rider_to_ride_and_vehicle_count
                .entry(rider_username.clone())
                .or_insert((0i64, 0i64));
            rider_ride_and_vehicle_count.0 += ride_count;
        }

        let vehicles_query = format!(
            "
                SELECT i.rider_username, CAST(COUNT(*) AS bigint) vehicle_count
                FROM (
                    SELECT DISTINCT r.rider_username, r.company, rv.vehicle_number
                    FROM bim.rides r
                    INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
                    WHERE rv.coupling_mode = 'R'
                    {} {}
                ) i
                GROUP BY i.rider_username
            ",
            if query_criteria.len() > 0 { " AND " } else { "" },
            query_criteria.join(" AND "),
        );
        let vehicle_rows_res = ride_conn.query(
            &vehicles_query,
            &query_params,
        ).await;
        let vehicle_rows = match vehicle_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most active riders with vehicles: {}", e);
                send_channel_message!(
                    interface,
                    &channel_name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        for row in vehicle_rows {
            let rider_username: String = row.get(0);
            let vehicle_count: i64 = row.get(1);

            let rider_ride_and_vehicle_count = rider_to_ride_and_vehicle_count
                .entry(rider_username.clone())
                .or_insert((0i64, 0i64));
            rider_ride_and_vehicle_count.1 += vehicle_count;
        }

        let mut rider_and_ride_and_vehicle_count: Vec<(String, i64, i64)> = rider_to_ride_and_vehicle_count
            .iter()
            .map(|(r, (rc, vc))| (r.clone(), *rc, *vc))
            .collect();
        rider_and_ride_and_vehicle_count.sort_unstable_by_key(|(r, rc, _vc)| (-*rc, r.clone()));
        let mut rider_strings: Vec<String> = rider_and_ride_and_vehicle_count.iter()
            .map(|(rider_name, ride_count, vehicle_count)| {
                let ride_text = if *ride_count == 1 {
                    "one ride".to_owned()
                } else {
                    format!("{} rides", ride_count)
                };
                let vehicle_text = if *vehicle_count == 1 {
                    "one vehicle".to_owned()
                } else {
                    format!("{} vehicles", vehicle_count)
                };
                let uniqueness_percentage = (*vehicle_count as f64) * 100.0 / (*ride_count as f64);

                format!("{} ({} in {}; {:.2}% unique)", rider_name, ride_text, vehicle_text, uniqueness_percentage)
            })
            .collect();
        let prefix = if rider_strings.len() < 6 {
            "Top riders:\n"
        } else {
            rider_strings.drain(5..);
            "Top 5 riders:\n"
        };
        let riders_string = rider_strings.join("\n");

        let response_string = if riders_string.len() == 0 {
            "No top riders.".to_owned()
        } else {
            format!("{}{}", prefix, riders_string)
        };

        send_channel_message!(
            interface,
            &channel_name,
            &response_string,
        ).await;
    }

    async fn do_topriders_vehicle(
        &self,
        channel_name: &str,
        company: Option<&str>,
        lookback_range: LookbackRange,
        vehicle_number: &VehicleNumber,
    ) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        // if no company is given, fall back to the default one
        // (since e.g. adding up the rides of all companies' vehicles numbered 3333
        // doesn't make much sense)
        let actual_company = if let Some(c) = company {
            c
        } else {
            &config_guard.default_company
        };

        let ride_conn = match connect_ride_db(&config_guard).await {
            Ok(c) => c,
            Err(_) => {
                send_channel_message!(
                    interface,
                    &channel_name,
                    "Failed to open database connection. :disappointed:",
                ).await;
                return;
            },
        };

        let mut query_criteria: Vec<String> = Vec::with_capacity(2);
        let mut query_params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(3);

        let vehicle_number_holder = vehicle_number.as_str();
        query_params.push(&vehicle_number_holder);

        query_params.push(&actual_company);

        let timestamp_holder;
        if let Some(lbts) = lookback_range.start_timestamp() {
            timestamp_holder = lbts;
            query_criteria.push(format!("r.\"timestamp\" >= ${}", query_params.len() + 1));
            query_params.push(&timestamp_holder);
        }

        let rides_query = format!(
            "
                SELECT r.rider_username, CAST(COUNT(*) AS bigint) ride_count
                FROM bim.rides r
                WHERE EXISTS (
                    SELECT 1
                    FROM bim.ride_vehicles rv
                    WHERE rv.ride_id = r.id
                    AND rv.coupling_mode = 'R'
                    AND rv.vehicle_number = $1
                )
                AND r.company = $2
                {} {}
                GROUP BY r.rider_username
            ",
            if query_criteria.len() > 0 { " AND " } else { "" },
            query_criteria.join(" AND "),
        );

        let ride_rows_res = ride_conn.query(
            &rides_query,
            &query_params,
        ).await;
        let ride_rows = match ride_rows_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to query most active riders: {}", e);
                send_channel_message!(
                    interface,
                    &channel_name,
                    "Failed to query database. :disappointed:",
                ).await;
                return;
            },
        };

        let mut rider_to_ride_count = HashMap::new();
        for row in ride_rows {
            let rider_username: String = row.get(0);
            let ride_count: i64 = row.get(1);

            let rider_ride_and_vehicle_count = rider_to_ride_count
                .entry(rider_username.clone())
                .or_insert(0);
            *rider_ride_and_vehicle_count += ride_count;
        }

        let mut rider_and_ride_count: Vec<(String, i64)> = rider_to_ride_count
            .iter()
            .map(|(r, rc)| (r.clone(), *rc))
            .collect();
        rider_and_ride_count.sort_unstable_by_key(|(r, rc)| (-*rc, r.clone()));
        let mut rider_strings: Vec<String> = rider_and_ride_count.iter()
            .map(|(rider_name, ride_count)| {
                let ride_text = if *ride_count == 1 {
                    "one ride".to_owned()
                } else {
                    format!("{} rides", ride_count)
                };

                format!("{}: {}", rider_name, ride_text)
            })
            .collect();
        let prefix = if rider_strings.len() < 6 {
            "Top riders in this vehicle:\n"
        } else {
            rider_strings.drain(5..);
            "Top 5 riders in this vehicle:\n"
        };
        let riders_string = rider_strings.join("\n");

        let response_string = if riders_string.len() == 0 {
            "No top riders in this vehicle.".to_owned()
        } else {
            format!("{}{}", prefix, riders_string)
        };

        send_channel_message!(
            interface,
            &channel_name,
            &response_string,
        ).await;
    }

    pub(crate) async fn channel_command_topriders(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let lookback_range: LookbackRange = match Self::lookback_range_from_command(command) {
            Some(lr) => lr,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Hey, no mixing options that mean different time ranges!",
                ).await;
                return;
            },
        };

        let company = command
            .options.get("company")
            .or_else(|| command.options.get("c"))
            .map(|c| c.as_str().unwrap());

        let vehicle_number = VehicleNumber::from_string(
            command.rest.trim().to_owned(),
        );

        if vehicle_number.len() > 0 {
            self.do_topriders_vehicle(
                &channel_message.channel.name,
                company,
                lookback_range,
                &vehicle_number,
            ).await
        } else {
            self.do_topriders_all_vehicles(
                &channel_message.channel.name,
                company,
                lookback_range,
            ).await
        }
    }
}
