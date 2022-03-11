use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use log::{debug, warn};
use rocketbot_interface::send_channel_message;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::Channel;
use rocketbot_interface::sync::Mutex;
use serde_json;


#[derive(Clone, Debug, Eq, PartialEq)]
struct ChannelInfo {
    pub usernames: Option<HashSet<String>>,
    pub join_message_format: Option<String>,
    pub leave_message_format: Option<String>,
}
impl ChannelInfo {
    pub fn new(
        usernames: Option<HashSet<String>>,
        join_message_format: Option<String>,
        leave_message_format: Option<String>,
    ) -> Self {
        Self {
            usernames,
            join_message_format,
            leave_message_format,
        }
    }
}


pub struct UserListMessagePlugin {
    interface: Weak<dyn RocketBotInterface>,
    channel_name_to_info: Mutex<HashMap<String, ChannelInfo>>,
}
#[async_trait]
impl RocketBotPlugin for UserListMessagePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        // read configuration
        let channel_array = config["channels"].as_object()
            .expect("channels is not an object");
        let mut channel_name_to_info_map = HashMap::new();
        for (channel_name, channel_config) in channel_array {
            let join_message_format = if channel_config["join_message_format"].is_null() {
                None
            } else {
                Some(
                    channel_config["join_message_format"]
                        .as_str().expect("join_message_format is neither null nor str")
                        .to_owned()
                )
            };
            let leave_message_format = if channel_config["leave_message_format"].is_null() {
                None
            } else {
                Some(
                    channel_config["leave_message_format"]
                        .as_str().expect("leave_message_format is neither null nor str")
                        .to_owned()
                )
            };

            channel_name_to_info_map.insert(
                channel_name.clone(),
                ChannelInfo::new(
                    None,
                    join_message_format,
                    leave_message_format,
                ),
            );
        }

        let channel_name_to_info = Mutex::new(
            "UserListMessagePlugin::channel_name_to_info",
            channel_name_to_info_map,
        );
        Self {
            interface,
            channel_name_to_info,
        }
    }

    async fn plugin_name(&self) -> String {
        "user_list_message".to_owned()
    }

    async fn channel_user_list_updated(&self, channel: &Channel) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let mut channel_guard = self.channel_name_to_info.lock().await;
        let channel_info = match channel_guard.get_mut(&channel.name) {
            None => return,
            Some(ci) => ci,
        };

        let new_users = match interface.obtain_users_in_channel(&channel.name).await {
            None => return,
            Some(nu) => nu,
        };
        let new_usernames: HashSet<String> = new_users.iter()
            .map(|u| u.username.clone())
            .collect();
        debug!("new usernames for {:?}: {:?}", channel.name, new_usernames);

        if let Some(old_usernames) = &channel_info.usernames {
            // we can compare the old and the new list
            debug!("old usernames for {:?}: {:?}", channel.name, old_usernames);

            if let Some(jmf) = &channel_info.join_message_format {
                let joined_usernames: HashSet<&String> = new_usernames
                    .difference(old_usernames)
                    .collect();
                for joined_username in joined_usernames {
                    let joined_message = jmf
                        .replace("{USERNAME}", joined_username);
                    send_channel_message!(
                        interface,
                        &channel.name,
                        &joined_message,
                    ).await;
                }
            }

            if let Some(lmf) = &channel_info.leave_message_format {
                let left_usernames: HashSet<&String> = old_usernames
                    .difference(&new_usernames)
                    .collect();
                for left_username in left_usernames {
                    let left_message = lmf
                        .replace("{USERNAME}", left_username);
                    send_channel_message!(
                        interface,
                        &channel.name,
                        &left_message,
                    ).await;
                }
            }
        }

        // remember the current users for later
        channel_info.usernames = Some(new_usernames);
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        warn!("configuration updates are not yet supported for the user_list_message plugin");
        false
    }
}
