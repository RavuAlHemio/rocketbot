mod model;


use std::collections::{BTreeMap, BTreeSet, HashMap};
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


fn find_station<'a, 'b>(database: &'a StationDatabase, station_name_lower: &'b str, force_search: bool) -> Option<&'a StoppingPoint> {
    if !force_search {
        // try pinpointing the station using the number
        if let Ok(station_number) = station_name_lower.parse::<u32>() {
            for (_lower_name, station) in &database.stations {
                if station.stop_id == station_number {
                    return Some(station);
                }
            }
        }
    }

    // find the station using Damerau-Levenshtein
    let best_station_distance = {
        let mut bsd: Option<(&StoppingPoint, usize)> = None;
        for (lower_name, station) in &database.stations {
            let distance = damerau_levenshtein(&station_name_lower, lower_name);
            if let Some((_, best_distance)) = &bsd {
                if distance < *best_distance {
                    bsd = Some((&station, distance));
                }
            } else {
                bsd = Some((&station, distance));
            }
        }
        bsd
    };

    if let Some((best_station, _best_distance)) = best_station_distance {
        Some(best_station)
    } else {
        None
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

    async fn get_departures(&self, station_id: u32, line_number: Option<&str>) -> Option<Vec<Vec<DepartureLine>>> {
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

        let mut platform_to_deps: HashMap<Option<i64>, BTreeMap<(String, String), DepartureLine>> = HashMap::new();
        for monitor in &monitor_wrapper.data.monitors {
            let platform_number = monitor.location_stop.properties.attributes.rbl;
            let dep_lines = platform_to_deps
                .entry(platform_number)
                .or_insert_with(|| BTreeMap::new());

            for line in &monitor.lines {
                if let Some(ln) = line_number {
                    if line.name != ln {
                        continue;
                    }
                }

                for departure in &line.departure_data.departures {
                    let countdown = match departure.departure_time.countdown {
                        Some(cd) => cd,
                        None => continue,
                    };

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
                        countdown,
                        barrier_free,
                        realtime,
                        traffic_jam,
                    ))
                }
            }
        }

        let mut ret_monitors: Vec<Vec<DepartureLine>> = platform_to_deps.into_values()
            .map(|deps| deps.into_values().collect())
            .collect();
        ret_monitors.sort_unstable_by_key(|rm: &Vec<DepartureLine>| {
            let rm_vec: Vec<(String, String)> = rm.iter()
                .map(|dl| (dl.line_name.clone(), dl.target_station.to_lowercase()))
                .collect();
            rm_vec
        });

        Some(ret_monitors)
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
        let force_search = command.flags.contains("s") || command.flags.contains("search");

        let station = {
            let db_guard = self.station_database
                .read().await;
            match find_station(&*db_guard, &station_name_lower, force_search) {
                Some(st) => st.clone(),
                None => {
                    send_channel_message!(
                        interface,
                        &channel_message.channel.name,
                        "Station not found.",
                    ).await;
                    return;
                },
            }
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
            let mut first_platform = true;
            for platform in &departures {
                if first_platform {
                    first_platform = false;
                } else {
                    ds.push_str("\n");
                }

                for line in platform {
                    write!(&mut ds, "\n{} \u{2192} {}", line.line_name, line.target_station).unwrap();
                    for departure in &line.departures {
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
            }
            ds
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &departures_string,
        ).await;
    }

    async fn channel_command_stations(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        self.ensure_station_database_current().await;

        let wanted_station_name_lower = command.rest.trim().to_lowercase();

        {
            let mut final_pieces = Vec::new();

            let db_guard = self.station_database
                .read().await;
            let mut substring_stations: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
            for (station_name_lower, station) in &db_guard.stations {
                if station_name_lower.contains(&wanted_station_name_lower) {
                    let station_numbers = substring_stations
                        .entry(station.name.clone())
                        .or_insert_with(|| BTreeSet::new());
                    station_numbers.insert(station.stop_id);
                }
            }
            if substring_stations.len() > 0 {
                let mut substring_piece = String::from("Substring matches:");
                for (name, numbers) in &substring_stations {
                    let number_strings: Vec<String> = numbers
                        .iter()
                        .map(|n| n.to_string())
                        .collect();
                    let number_string = number_strings.join(", ");
                    write!(&mut substring_piece, "\n{}: {}", name, number_string).unwrap();
                }
                final_pieces.push(substring_piece);
            }

            let dl_station_opt = find_station(
                &*db_guard,
                &wanted_station_name_lower,
                true,
            );
            if let Some(dl_station) = dl_station_opt {
                final_pieces.push(format!(
                    "Most similarly-named station: {} ({})",
                    dl_station.name, dl_station.stop_id,
                ));
            }

            if final_pieces.len() == 0 {
                final_pieces.push("No stations found.".to_owned());
            }

            let response = final_pieces.join("\n");
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &response,
            ).await;
        }
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
                .add_flag("s")
                .add_flag("search")
                .add_option("l", CommandValueType::String)
                .add_option("line", CommandValueType::String)
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "stations".to_owned(),
                "wienerlinien".to_owned(),
                "{cpfx}stations TEXT".to_owned(),
                "Find station names containing or similar to the given name.".to_owned(),
            )
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
        } else if command.name == "stations" {
            self.channel_command_stations(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "dep" {
            Some(include_str!("../help/dep.md").to_owned())
        } else if command_name == "stations" {
            Some(include_str!("../help/stations.md").to_owned())
        } else {
            None
        }
    }
}
