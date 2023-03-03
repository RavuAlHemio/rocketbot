use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;
use std::sync::Weak;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, error};
use rocketbot_interface::{JsonValueExtensions, send_channel_message, send_private_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, Message, PrivateMessage, User};
use tokio::sync::RwLock;
use url::Url;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Config {
    pub jack_to_port_api_uri: Url,
    pub port_api_uri: Url,
    pub authorized_usernames: BTreeSet<String>,
    pub timeout_ms: u64,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AnyMessage {
    Channel(ChannelMessage),
    Private(PrivateMessage),
}
impl AnyMessage {
    pub fn message(&self) -> &Message {
        match self {
            Self::Channel(c) => &c.message,
            Self::Private(p) => &p.message,
        }
    }

    pub fn sender(&self) -> &User {
        &self.message().sender
    }

    pub async fn respond<I: RocketBotInterface + ?Sized>(&self, interface: &I, message_body: &str) {
        match self {
            Self::Channel(c) => {
                send_channel_message!(
                    interface,
                    &c.channel.name,
                    message_body,
                ).await
            },
            Self::Private(p) => {
                send_private_message!(
                    interface,
                    &p.conversation.id,
                    message_body,
                ).await
            },
        }
    }
}


macro_rules! write_expect {
    ($dst:expr, $($arg:tt)*) => {
        write!($dst, $($arg)*).expect("write failed")
    };
}

/// Returns a string representation of the given JSON value.
///
/// If it is a string value, the string value is returned. If it is a different type of value, it is
/// converted into the JSON string representation and returned. This means that strings are returned
/// without quote marks but all other values are returned in their JSON representation.
fn stringify(value: &serde_json::Value) -> Cow<str> {
    if let Some(v) = value.as_str() {
        Cow::Borrowed(v)
    } else {
        Cow::Owned(value.to_string())
    }
}


/// Returns the difference between the values of two integer-valued counters.
fn counter_diff(older_counters: &serde_json::Value, newer_counters: &serde_json::Value, key: &str) -> Option<u64> {
    if let Some(older_value) = older_counters[key].as_u64() {
        if let Some(newer_value) = newer_counters[key].as_u64() {
            if newer_value >= older_value {
                return Some(newer_value - older_value);
            } else {
                // the counter has been reset midway
                return Some(newer_value);
            }
        }
    }
    None
}


fn extend_with_realtime_info(info_block: &mut String, port: &serde_json::Value) {
    let physical = &port["realtime"]["physical"];
    let aggregation = &port["realtime"]["aggregation"];
    let realtime = if !physical.is_null() {
        physical
    } else if !aggregation.is_null() {
        aggregation
    } else {
        &serde_json::Value::Null
    };

    if !realtime.is_null() {
        let common = &realtime["port"]["common"];
        let admin_status = &common["admin_status"];
        let oper_status = &common["oper_status"];
        let mut show_speed = false;
        if admin_status == "up" && oper_status == "up" {
            write_expect!(info_block, "\nport is up");
            show_speed = true;
        } else if admin_status == "up" && oper_status == "down" {
            write_expect!(info_block, "\nport is down");
        } else if admin_status == "down" && oper_status == "down" {
            write_expect!(info_block, "\nport is shut");
        } else {
            write_expect!(info_block, "\nport is administratively {}, operationally {}", stringify(admin_status), stringify(oper_status));
        }

        if let Some(dis_reason) = common["error_disabled_reason"].as_str() {
            write_expect!(info_block, "\nport is err-disabled; reason: {}", dis_reason);
        }

        if let Some(descr) = common["description"].as_str() {
            if descr.trim().len() > 0 {
                write_expect!(info_block, "\nport description: {}", descr);
            }
        }

        if show_speed {
            if let Some(port_speed) = common["speed_bps"].as_u64() {
                const SI_PREFIXES: [&str; 11] = ["", "K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];
                let mut modified_speed = port_speed;
                let mut si_prefix_index = 0;

                while modified_speed % 1000 == 0 && si_prefix_index < SI_PREFIXES.len() - 1 {
                    modified_speed /= 1000;
                    si_prefix_index += 1;
                }
                write_expect!(info_block, "\nspeed: {} {}b/s", modified_speed, SI_PREFIXES[si_prefix_index]);
            }
        }

        // VLANs
        let mut vlan_blocks = Vec::with_capacity(3);
        if let Some(vlan_id) = common["untagged_vlan_id"].as_u64() {
            if vlan_id != 0 {
                vlan_blocks.push(format!("VLAN: {}", vlan_id));
            }
        }
        if let Some(voice_vlan_id) = common["voice_vlan_id"].as_u64() {
            if voice_vlan_id != 0 {
                vlan_blocks.push(format!("voice VLAN: {}", voice_vlan_id));
            }
        }
        if let Some(tagged) = common["tagged_vlan_ids"].as_array() {
            let mut tagged_nums = String::new();
            for tagged_value in tagged {
                if let Some(tagged_u64) = tagged_value.as_u64() {
                    if tagged_nums.len() > 0 {
                        tagged_nums.push_str(", ");
                    }
                    write_expect!(tagged_nums, "{}", tagged_u64);
                }
            }
            if tagged_nums.len() > 0 {
                vlan_blocks.push(format!("tagged VLANs: {}", tagged_nums));
            }
        }
        if vlan_blocks.len() > 0 {
            write_expect!(info_block, "\n{}", vlan_blocks.join(", "));
        } else {
            write_expect!(info_block, "\nno VLANs");
        }

        // counters
        if let Some(counter_age_ms) = realtime["later_sample_delay_ms"].as_f64() {
            let older_counters = &common["counters"];
            let newer_counters = &realtime["later_counter_sample"];

            let mut counter_changes = Vec::with_capacity(6);
            // always show base values
            if let Some(incoming_delta) = counter_diff(older_counters, newer_counters, "incoming_bytes") {
                counter_changes.push(format!("{} B received", incoming_delta));
            }
            if let Some(outgoing_delta) = counter_diff(older_counters, newer_counters, "outgoing_bytes") {
                counter_changes.push(format!("{} B sent", outgoing_delta));
            }
            // only show error values if they aren't zero
            if let Some(incoming_discard_delta) = counter_diff(older_counters, newer_counters, "incoming_discarded_packets") {
                if incoming_discard_delta > 0 {
                    counter_changes.push(format!("{} incoming packets dropped", incoming_discard_delta));
                }
            }
            if let Some(incoming_error_delta) = counter_diff(older_counters, newer_counters, "incoming_error_packets") {
                if incoming_error_delta > 0 {
                    counter_changes.push(format!("{} incoming packets have errors", incoming_error_delta));
                }
            }
            if let Some(outgoing_discard_delta) = counter_diff(older_counters, newer_counters, "outgoing_discarded_packets") {
                if outgoing_discard_delta > 0 {
                    counter_changes.push(format!("{} outgoing packets dropped", outgoing_discard_delta));
                }
            }
            if let Some(outgoing_error_delta) = counter_diff(older_counters, newer_counters, "outgoing_error_packets") {
                if outgoing_error_delta > 0 {
                    counter_changes.push(format!("{} outgoing packets have errors", outgoing_error_delta));
                }
            }

            write_expect!(info_block, "\nstatistics within the last {} ms: {}", counter_age_ms, counter_changes.join(", "));
        }
    }
}


pub struct NetdevPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    known_commands: HashSet<String>,
    http_client: reqwest::Client,
}
impl NetdevPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let jack_to_port_api_uri_str = config["jack_to_port_api_uri"]
            .as_str().ok_or("jack_to_port_api_uri missing or not a string")?;
        let jack_to_port_api_uri = Url::parse(jack_to_port_api_uri_str)
            .or(Err("jack_to_port_api_uri not a valid URL"))?;
        let port_api_uri_str = config["port_api_uri"]
            .as_str().ok_or("port_api_uri missing or not a string")?;
        let port_api_uri = Url::parse(port_api_uri_str)
            .or(Err("port_api_uri not a valid URL"))?;
        let timeout_ms = config["timeout_ms"]
            .as_u64_or_strict(5_000).ok_or("timeout_ms missing or not an unsigned 64-bit integer")?;

        let authorized_usernames_iter = config["authorized_usernames"]
            .members_or_empty_strict().ok_or("authorized_usernames missing or not a list")?;
        let mut authorized_usernames = BTreeSet::new();
        for entry in authorized_usernames_iter {
            authorized_usernames.insert(
                entry
                    .as_str().ok_or("authorized_usernames entry not a string")?
                    .to_owned()
            );
        }

        Ok(Config {
            jack_to_port_api_uri,
            port_api_uri,
            authorized_usernames,
            timeout_ms,
        })
    }

    async fn handle_command(&self, message: AnyMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            Some(i) => i,
            None => return,
        };
        let config = {
            let config_guard = self.config.read().await;
            (*config_guard).clone()
        };

        if !self.known_commands.contains(&command.name) {
            // not one of our commands
            return;
        }

        if !config.authorized_usernames.contains(&message.sender().username) {
            message.respond(&*interface, "Sorry, you are not authorized to use this command.").await;
            return;
        }

        match command.name.as_str() {
            "dose" => self.handle_dose_command(message, command, &config).await,
            "port" => self.handle_port_command(message, command, &config).await,
            _ => {},
        };
    }

    async fn get_http_json(&self, uri: Url, timeout: Duration) -> Option<serde_json::Value> {
        let resp_res = self.http_client
            .get(uri.clone())
            .timeout(timeout)
            .send().await
            .and_then(|response| response.error_for_status());
        let resp = match resp_res {
            Ok(r) => r,
            Err(e) => {
                error!("failed to obtain {} response: {}", uri, e);
                return None;
            },
        };
        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                error!("failed to obtain {} bytes: {}", uri, e);
                return None;
            },
        };
        let json: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to decode {} as JSON: {}", uri, e);
                return None;
            },
        };
        Some(json)
    }

    async fn handle_dose_command(&self, message: AnyMessage, command: &CommandInstance, config: &Config) {
        let interface = match self.interface.upgrade() {
            Some(i) => i,
            None => return,
        };

        // resolve jack
        let jack_name = command.rest.trim();

        let mut jack_uri = config.jack_to_port_api_uri.clone();
        jack_uri.query_pairs_mut()
            .append_pair("jack", jack_name);
        let jack_data_opt = self
            .get_http_json(jack_uri, Duration::from_millis(config.timeout_ms))
            .await;
        let jack_data = match jack_data_opt {
            Some(jd) => jd,
            None => {
                message.respond(&*interface, "Failed to obtain jack data.").await;
                return;
            },
        };

        debug!("obtained jack data: {}", jack_data);

        let mut ports = jack_data["ports"].members_or_empty().peekable();
        if ports.peek().is_none() {
            // no entries
            message.respond(
                &*interface,
                &format!("Jack `{}` is not known to Coruscant tools -- is it patched?", jack_name),
            ).await;
            return;
        }

        let mut info_blocks = Vec::new();
        for port in ports {
            let switch_name = port["switch"].as_str().unwrap_or("???");
            let port_name = port["port"].as_str().unwrap_or("???");

            let mut info_block = format!("connected to {} port {}", switch_name, port_name);
            extend_with_realtime_info(&mut info_block, port);

            info_blocks.push(info_block);
        }

        let mut response_text = format!("jack `{}`:", jack_name);
        for info_block in info_blocks {
            response_text.push_str("\n\n");
            response_text.push_str(&info_block);
        }

        message.respond(&*interface, &response_text).await;
    }

    async fn handle_port_command(&self, message: AnyMessage, command: &CommandInstance, config: &Config) {
        let interface = match self.interface.upgrade() {
            Some(i) => i,
            None => return,
        };

        let switch_name = &command.args[0];
        let port_name = command.rest.trim();

        let mut port_uri = config.port_api_uri.clone();
        port_uri.query_pairs_mut()
            .append_pair("switch", switch_name)
            .append_pair("port", port_name);
        let port_data_opt = self
            .get_http_json(port_uri, Duration::from_millis(config.timeout_ms))
            .await;
        let port_data = match port_data_opt {
            Some(jd) => jd,
            None => {
                message.respond(&*interface, "Failed to obtain jack data.").await;
                return;
            },
        };

        debug!("obtained port data: {}", port_data);

        let mut ports = port_data["ports"].members_or_empty().peekable();
        if ports.peek().is_none() {
            // no entries
            message.respond(
                &*interface,
                &format!("Port `{}` on switch `{}` is not known to Coruscant tools.", port_name, switch_name),
            ).await;
            return;
        }

        let mut info_blocks = Vec::new();
        for port in ports {
            let actual_switch_name = port["switch"].as_str().unwrap_or("???");
            let actual_port_name = port["port"].as_str().unwrap_or("???");

            let mut info_block = format!("switch {} port {}", actual_switch_name, actual_port_name);
            extend_with_realtime_info(&mut info_block, port);

            info_blocks.push(info_block);
        }

        let mut response_text = String::new();
        for info_block in info_blocks {
            if response_text.len() > 0 {
                response_text.push_str("\n\n");
            }
            response_text.push_str(&info_block);
        }

        message.respond(&*interface, &response_text).await;
    }
}
#[async_trait]
impl RocketBotPlugin for NetdevPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(config_object);

        let http_client = reqwest::Client::new();

        let mut known_commands = HashSet::new();
        let commands = [
            CommandDefinitionBuilder::new(
                "dose",
                "netdev",
                "{cpfx}dose JACK",
                "Outputs information about a network jack and the switch port it is connected to.",
            )
                .build(),
            CommandDefinitionBuilder::new(
                "port",
                "netdev",
                "{cpfx}port SWITCH PORT",
                "Outputs information about a switch port.",
            )
                .arg_count(1)
                .build(),
        ];
        for command in commands {
            known_commands.insert(command.name.clone());
            my_interface.register_channel_command(&command).await;
            my_interface.register_private_message_command(&command).await;
        }

        Self {
            interface,
            config: config_lock,
            known_commands,
            http_client,
        }
    }

    async fn plugin_name(&self) -> String {
        "netdev".to_owned()
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

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        self.handle_command(AnyMessage::Channel(channel_message.clone()), command).await
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        self.handle_command(AnyMessage::Private(private_message.clone()), command).await
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "dose" {
            Some(include_str!("../help/dose.md").to_owned())
        } else if command_name == "port" {
            Some(include_str!("../help/port.md").to_owned())
        } else {
            None
        }
    }
}
