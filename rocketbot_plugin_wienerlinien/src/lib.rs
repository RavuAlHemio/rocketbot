mod model;


use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Weak;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use csv;
use hyper::body::Buf;
use log::error;
use reqwest;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;
use strsim::damerau_levenshtein;

use crate::model::{DepartureLine, DepartureTimeEntry, MonitorWrapper, StoppingPoint};


#[derive(Clone, Debug, PartialEq)]
struct StationDatabase {
    pub stations: Vec<(String, StoppingPoint)>,
    pub instant: Option<Instant>,
}
impl Default for StationDatabase {
    fn default() -> Self {
        Self {
            stations: Vec::new(),
            instant: None,
        }
    }
}


pub struct WienerLinienPlugin {
    interface: Weak<dyn RocketBotInterface>,

    stop_points_url: String,
    monitor_url_format: String,
    max_stations_age_min: u64,

    station_database: RwLock<StationDatabase>,
    http_client: Mutex<reqwest::Client>,
}
impl WienerLinienPlugin {
    async fn ensure_station_database_current(&self) {
        let mut database_guard = self.station_database
            .write().await;
        if let Some(instant) = database_guard.instant {
            let now = Instant::now();
            if instant <= now && now - instant <= Duration::from_secs(self.max_stations_age_min * 60) {
                // we are up to date
                return;
            }
            // if instant > now, the counter must have overrun;
            // load a new database
        }

        let stations = {
            let client_guard = self.http_client.lock().await;
            let request = client_guard.get(&self.stop_points_url);
            let response = match request.send().await {
                Ok(r) => r,
                Err(e) => {
                    error!("failed to send stations update request to {:?}: {}", self.stop_points_url, e);
                    return;
                },
            };
            let response_bytes = match response.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    error!("failed to obtain bytes of stations update to {:?}: {}", self.stop_points_url, e);
                    return;
                },
            };
            let response_reader = response_bytes.reader();
            let mut response_decoder = csv::ReaderBuilder::new()
                .delimiter(b';')
                .quote(b'"')
                .has_headers(true)
                .from_reader(response_reader);

            let mut stations = Vec::new();
            for record_res in response_decoder.deserialize() {
                let station: StoppingPoint = match record_res {
                    Ok(r) => r,
                    Err(e) => {
                        error!("failed to obtain a station entry from {:?}: {}", self.stop_points_url, e);
                        return;
                    },
                };
                let station_name_lower = station.name.to_lowercase();
                stations.push((station_name_lower, station));
            }

            stations
        };

        database_guard.stations = stations;
    }

    async fn get_departures(&self, station_id: u32, line_number: Option<&str>) -> Option<Vec<DepartureLine>> {
        let url = self.monitor_url_format
            .replace("{stopId}", &station_id.to_string());

        let client_guard = self.http_client.lock().await;
        let request = client_guard.get(&url);
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to send monitor request to {:?}: {}", url, e);
                return None;
            },
        };
        let response_bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                error!("failed to obtain bytes of monitor {:?}: {}", url, e);
                return None;
            },
        };
        let response_reader = response_bytes.reader();
        let monitor_wrapper: MonitorWrapper = match serde_json::from_reader(response_reader) {
            Ok(mw) => mw,
            Err(e) => {
                error!("failed to parse monitor {:?}: {}", url, e);
                return None;
            },
        };

        let mut dep_lines = BTreeMap::new();

        for monitor in &monitor_wrapper.data.monitors {
            for line in &monitor.lines {
                if let Some(ln) = line_number {
                    if line.name != ln {
                        continue;
                    }
                }

                for departure in &line.departure_data.departures {
                    let (line_and_target, target_full, barrier_free, realtime, traffic_jam) = if let Some(vehicle) = &departure.vehicle {
                        (
                            (vehicle.name.clone(), vehicle.towards.to_lowercase()),
                            vehicle.towards.clone(),
                            vehicle.barrier_free,
                            vehicle.realtime_supported,
                            vehicle.traffic_jam,
                        )
                    } else {
                        (
                            (line.name.clone(), line.towards.to_lowercase()),
                            line.towards.clone(),
                            line.barrier_free,
                            line.realtime_supported,
                            line.traffic_jam,
                        )
                    };

                    let dep_entry = dep_lines
                        .entry(line_and_target.clone())
                        .or_insert_with(|| DepartureLine::new(
                            line_and_target.0,
                            target_full,
                            Vec::new(),
                        ));
                    dep_entry.departures.push(DepartureTimeEntry::new(
                        departure.departure_time.countdown,
                        barrier_free,
                        realtime,
                        traffic_jam,
                    ))
                }
            }
        }

        let dep_line_vec: Vec<DepartureLine> = dep_lines.into_values().collect();
        Some(dep_line_vec)
    }

    async fn channel_command_dep(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        self.ensure_station_database_current().await;

        let line = command.options.get("line")
            .or_else(|| command.options.get("l"))
            .map(|v| v.as_str().expect("line not a string").to_owned());
        let station_name_lower = command.rest.trim().to_lowercase();

        // find the station
        let best_station_distance = {
            let mut bsd: Option<(StoppingPoint, usize)> = None;
            let db_guard = self.station_database
                .read().await;
            for (lower_name, station) in &db_guard.stations {
                let distance = damerau_levenshtein(&station_name_lower, lower_name);
                if let Some((_, best_distance)) = &bsd {
                    if distance < *best_distance {
                        bsd = Some((station.clone(), distance));
                    }
                } else {
                    bsd = Some((station.clone(), distance));
                }
            }
            bsd
        };

        let station = match best_station_distance {
            Some((st, _dist)) => st,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Station not found.",
                ).await;
                return;
            },
        };

        // obtain departure information
        let departures_opt = self.get_departures(
            station.stop_id,
            line.as_deref(),
        ).await;
        let departures = match departures_opt {
            Some(d) => d,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    "Failed to obtain departures.",
                ).await;
                return;
            },
        };

        let departures_string = if departures.len() == 0 {
            format!("No departures at *{}*", station.name)
        } else {
            let mut ds = format!("Departures at *{}*", station.name);
            for line in &departures {
                write!(&mut ds, "\n{} \u{2192} {}", line.line_name, line.target_station).unwrap();
                for (i, departure) in line.departures.iter().enumerate() {
                    if departure.accessible {
                        // italic
                        write!(&mut ds, " | _{}_", departure.countdown).unwrap();
                    } else {
                        // regular
                        write!(&mut ds, " | {}", departure.countdown).unwrap();
                    }

                    if departure.traffic_jam {
                        // jammed: police siren
                        ds.push_str(" \u{1F6A8}");
                    }
                    if !departure.realtime {
                        // not realtime: question mark
                        ds.push_str(" \u{2753}");
                    }
                }
            }
            ds
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &departures_string,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for WienerLinienPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let stop_points_url = config["stop_points_url"]
            .as_str().expect("stop_points_url not a string")
            .to_owned();
        let monitor_url_format = config["monitor_url_format"]
            .as_str().expect("monitor_url_format not a string")
            .to_owned();
        let max_stations_age_min = config["max_stations_age_min"]
            .as_u64().expect("max_stations_age_min not a u64");

        let station_database = RwLock::new(
            "WienerLinienPlugin::station_database",
            StationDatabase::default(),
        );
        let http_client = Mutex::new(
            "WienerLinienPlugin::client",
            reqwest::Client::new(),
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "dep".to_owned(),
                "wienerlinien".to_owned(),
                "{cpfx}dep [-l LINE] STATION".to_owned(),
                "Shows public transport departures from a given station.".to_owned(),
            )
                .add_option("l", CommandValueType::String)
                .add_option("line", CommandValueType::String)
                .build()
        ).await;

        Self {
            interface,
            stop_points_url,
            monitor_url_format,
            max_stations_age_min,
            station_database,
            http_client,
        }
    }

    async fn plugin_name(&self) -> String {
        "wienerlinien".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "dep" {
            self.channel_command_dep(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "dep" {
            Some(include_str!("../help/dep.md").to_owned())
        } else {
            None
        }
    }
}
