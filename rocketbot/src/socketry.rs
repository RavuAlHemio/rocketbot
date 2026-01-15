use core::panic;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::Write as FmtWrite;
use std::io::{Cursor, Read, Write as IoWrite};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, LazyLock, Weak};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::StatusCode;
use hyper::body::{Bytes, Incoming};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client as HttpClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use rand::{Rng, SeedableRng};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use regex::Regex;
use rocketbot_interface::{JsonValueExtensions, rocketchat_timestamp_to_datetime};
use rocketbot_interface::commands::{CommandBehaviors, CommandConfiguration, CommandDefinition};
use rocketbot_interface::errors::HttpError;
use rocketbot_interface::interfaces::RocketBotInterface;
use rocketbot_interface::message::MessageFragment;
use rocketbot_interface::model::{
    Channel, ChannelMessage, ChannelTextType, ChannelType, EditInfo, Emoji, Message,
    MessageAttachment, OutgoingMessage, OutgoingMessageWithAttachment, PrivateConversation,
    PrivateMessage, User,
};
use rocketbot_interface::sync::{Mutex, RwLock};
use rocketbot_string::regex::EnjoyableRegex;
use serde_json;
use sha2::{Digest, Sha256};
use tokio;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::commands::parse_command;
use crate::config::{CONFIG, load_config, PluginConfig, set_config};
use crate::errors::WebSocketError;
use crate::jsonage::parse_message;
use crate::plugins::{load_plugins, Plugin};
use crate::rate_limiting::MaybeRateLimitedStream;
use crate::string_utils::{Token, tokenize};


static LOGIN_MESSAGE_ID: &'static str = "login4242";
static GET_SETTINGS_MESSAGE_ID: &'static str = "settings4242";
static GET_ROOMS_MESSAGE_ID: &'static str = "rooms4242";
static SUBSCRIBE_ROOMS_MESSAGE_ID: &'static str = "roomchanges4242";
static SEND_MESSAGE_MESSAGE_ID: &'static str = "sendmessage4242";
static ID_ALPHABET: &'static str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const ID_LENGTH: usize = 17;
static BOUNDARY_ALPHABET: &'static str = ID_ALPHABET;
const BOUNDARY_LENGTH: usize = 64;
static QUOTE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    "^\\s*\\[ \\]\\([^)]+\\)\\s*",
).expect("failed to compile quote regular expression"));


struct ChannelDatabase {
    channel_by_id: HashMap<String, Channel>,
    channel_by_name: HashMap<String, Channel>,
    users_by_channel_id: HashMap<String, HashSet<User>>,
    private_by_id: HashMap<String, PrivateConversation>,
    private_id_by_counterpart_username: HashMap<String, String>,
}
impl ChannelDatabase {
    fn new_empty() -> Self {
        Self {
            channel_by_id: HashMap::new(),
            channel_by_name: HashMap::new(),
            users_by_channel_id: HashMap::new(),
            private_by_id: HashMap::new(),
            private_id_by_counterpart_username: HashMap::new(),
        }
    }

    fn register_channel(&mut self, channel: Channel) {
        if channel.channel_type != ChannelType::Channel && channel.channel_type != ChannelType::Group {
            panic!("forbidden channel type {:?} passed to register_channel", channel.channel_type);
        }

        // make sure we either don't know the channel at all or we know it fully
        // (ensure there is no pair of channels with different IDs but the same name)
        let know_id = self.channel_by_id.contains_key(&channel.id);
        let know_name = self.channel_by_name.contains_key(&channel.name);
        if know_id != know_name {
            panic!(
                "attempting to register duplicate channel with ID {:?} (already known? {}) and name {:?} (already known? {})",
                channel.id, know_id, channel.name, know_name,
            );
        }

        self.channel_by_id.insert(channel.id.clone(), channel.clone());
        self.channel_by_name.insert(channel.name.clone(), channel.clone());
        self.users_by_channel_id.insert(channel.id.clone(), HashSet::new());
    }

    fn register_private_conversation(&mut self, convo: PrivateConversation) {
        self.private_by_id.insert(convo.id.clone(), convo.clone());
        if convo.other_participants.len() == 1 {
            self.private_id_by_counterpart_username.insert(
                convo.other_participants[0].username.clone(),
                convo.id.clone(),
            );
        }
    }

    fn get_channel_by_id(&self, id: &str) -> Option<&Channel> {
        self.channel_by_id.get(id)
    }

    fn get_channel_by_name(&self, name: &str) -> Option<&Channel> {
        self.channel_by_name.get(name)
    }

    /// Returns `true` if the channel or private conversation was known (and removed) and `false`
    /// if it was not known.
    fn forget_by_id(&mut self, id: &str) -> bool {
        if let Some(channel) = self.channel_by_id.remove(id) {
            self.channel_by_name.remove(&channel.name);
            self.private_id_by_counterpart_username.retain(|_k, v| v == &channel.id);
            self.users_by_channel_id.remove(&channel.id);

            true
        } else if let Some(convo) = self.private_by_id.remove(id) {
            self.private_id_by_counterpart_username.retain(|_k, v| v == &convo.id);
            true
        } else {
            false
        }
    }

    fn users_in_channel(&self, channel_id: &str) -> HashSet<User> {
        if let Some(cu) = self.users_by_channel_id.get(channel_id) {
            cu.clone()
        } else {
            HashSet::new()
        }
    }

    fn replace_users_in_channel(&mut self, channel_id: &str, new_users: HashSet<User>) {
        self.users_by_channel_id.insert(channel_id.to_owned(), new_users);
    }

    #[allow(unused)]
    fn user_added_to_channel(&mut self, channel_id: &str, user: &User) {
        self.users_by_channel_id.entry(channel_id.to_owned())
            .or_insert_with(|| HashSet::new())
            .insert(user.clone());
    }

    #[allow(unused)]
    fn user_removed_from_channel(&mut self, channel_id: &str, user_id: &str) {
        self.users_by_channel_id.entry(channel_id.to_owned())
            .or_insert_with(|| HashSet::new())
            .retain(|u| u.id != user_id);
    }

    #[allow(unused)]
    fn channel_by_id(&self) -> &HashMap<String, Channel> {
        &self.channel_by_id
    }

    #[allow(unused)]
    fn channel_by_name(&self) -> &HashMap<String, Channel> {
        &self.channel_by_name
    }

    #[allow(unused)]
    fn users_by_channel_id(&self) -> &HashMap<String, HashSet<User>> {
        &self.users_by_channel_id
    }

    fn get_private_conversation_id_by_counterpart_username(&self, counterpart_username: &str) -> Option<String> {
        self.private_id_by_counterpart_username
            .get(counterpart_username)
            .map(|cu| cu.clone())
    }

    fn get_private_conversation_by_id(&self, id: &str) -> Option<&PrivateConversation> {
        self.private_by_id.get(id)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ServerSettings {
    pub max_message_length: Option<usize>,
    pub username_regex: EnjoyableRegex,
}
impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            max_message_length: None,
            username_regex: EnjoyableRegex::from_regex(Regex::new(
                "[0-9a-zA-Z-_.]+"
            ).expect("failed to compile default username regex")),
        }
    }
}


struct SharedConnectionState {
    outgoing_sender: mpsc::UnboundedSender<serde_json::Value>,
    exit_notify: Notify,
    plugins: RwLock<Vec<Plugin>>,
    subscribed_channels: RwLock<ChannelDatabase>,
    rng: Mutex<StdRng>,
    command_config: CommandConfiguration,
    channel_commands: RwLock<HashMap<String, CommandDefinition>>,
    private_message_commands: RwLock<HashMap<String, CommandDefinition>>,
    http_client: HttpClient<HttpsConnector<HttpConnector>, Full<Bytes>>,
    my_user_id: RwLock<Option<String>>,
    my_auth_token: RwLock<Option<String>>,
    server_settings: RwLock<ServerSettings>,
    username_to_initial_private_message: Mutex<HashMap<String, OutgoingMessage>>,
    username_to_initial_private_message_with_attachment: Mutex<HashMap<String, OutgoingMessageWithAttachment>>,
    new_timer_sender: mpsc::UnboundedSender<(DateTime<Utc>, serde_json::Value)>,
    reload_config_sender: mpsc::Sender<()>,
    channel_id_to_texts: RwLock<HashMap<(String, ChannelTextType), String>>,
    emoji: RwLock<Vec<Emoji>>,
    active_behavior_flags: RwLock<serde_json::Map<String, serde_json::Value>>,
    bot_user_ids: RwLock<HashSet<String>>,
}
impl SharedConnectionState {
    fn new(
        outgoing_sender: mpsc::UnboundedSender<serde_json::Value>,
        exit_notify: Notify,
        plugins: RwLock<Vec<Plugin>>,
        subscribed_channels: RwLock<ChannelDatabase>,
        rng: Mutex<StdRng>,
        command_config: CommandConfiguration,
        channel_commands: RwLock<HashMap<String, CommandDefinition>>,
        private_message_commands: RwLock<HashMap<String, CommandDefinition>>,
        http_client: HttpClient<HttpsConnector<HttpConnector>, Full<Bytes>>,
        my_user_id: RwLock<Option<String>>,
        my_auth_token: RwLock<Option<String>>,
        server_settings: RwLock<ServerSettings>,
        username_to_initial_private_message: Mutex<HashMap<String, OutgoingMessage>>,
        username_to_initial_private_message_with_attachment: Mutex<HashMap<String, OutgoingMessageWithAttachment>>,
        new_timer_sender: mpsc::UnboundedSender<(DateTime<Utc>, serde_json::Value)>,
        reload_config_sender: mpsc::Sender<()>,
        channel_id_to_texts: RwLock<HashMap<(String, ChannelTextType), String>>,
        emoji: RwLock<Vec<Emoji>>,
        active_behavior_flags: RwLock<serde_json::Map<String, serde_json::Value>>,
        bot_user_ids: RwLock<HashSet<String>>,
    ) -> Self {
        Self {
            outgoing_sender,
            exit_notify,
            plugins,
            subscribed_channels,
            rng,
            command_config,
            channel_commands,
            private_message_commands,
            http_client,
            my_user_id,
            my_auth_token,
            server_settings,
            username_to_initial_private_message,
            username_to_initial_private_message_with_attachment,
            new_timer_sender,
            reload_config_sender,
            channel_id_to_texts,
            emoji,
            active_behavior_flags,
            bot_user_ids,
        }
    }
}


struct ConnectionState {
    shared_state: Arc<SharedConnectionState>,
    outgoing_receiver: mpsc::UnboundedReceiver<serde_json::Value>,
    timers: Vec<(DateTime<Utc>, serde_json::Value)>,
    new_timer_receiver: mpsc::UnboundedReceiver<(DateTime<Utc>, serde_json::Value)>,
    reload_config_receiver: mpsc::Receiver<()>,
    last_seen_message_timestamp: DateTime<Utc>,
    process_message_sender: mpsc::UnboundedSender<MessageToHandle>,
}
impl ConnectionState {
    fn new(
        shared_state: Arc<SharedConnectionState>,
        outgoing_receiver: mpsc::UnboundedReceiver<serde_json::Value>,
        timers: Vec<(DateTime<Utc>, serde_json::Value)>,
        new_timer_receiver: mpsc::UnboundedReceiver<(DateTime<Utc>, serde_json::Value)>,
        reload_config_receiver: mpsc::Receiver<()>,
        last_seen_message_timestamp: DateTime<Utc>,
        process_message_sender: mpsc::UnboundedSender<MessageToHandle>,
    ) -> ConnectionState {
        ConnectionState {
            shared_state,
            outgoing_receiver,
            timers,
            new_timer_receiver,
            reload_config_receiver,
            last_seen_message_timestamp,
            process_message_sender,
        }
    }
}

pub(crate) struct ServerConnection {
    shared_state: Arc<SharedConnectionState>,
}
impl ServerConnection {
    fn new(
        shared_state: Arc<SharedConnectionState>,
    ) -> ServerConnection {
        ServerConnection {
            shared_state,
        }
    }

    #[allow(unused)]
    pub fn send(&self, message: serde_json::Value) {
        self.shared_state.outgoing_sender.send(message)
            .expect("failed to enqueue message");
    }

    #[allow(unused)]
    pub fn disconnect(&self) {
        self.shared_state.exit_notify.notify_one();
    }

    fn downcase_command_if_needed<'a>(&self, command_name: &'a str) -> Cow<'a, str> {
        if self.shared_state.command_config.case_fold_commands {
            Cow::Owned(command_name.to_lowercase())
        } else {
            Cow::Borrowed(command_name)
        }
    }

    fn downcase_command_if_needed_mut(&self, command_name: &mut String) {
        if self.shared_state.command_config.case_fold_commands {
            *command_name = command_name.to_lowercase();
        }
    }
}
impl Clone for ServerConnection {
    fn clone(&self) -> Self {
        ServerConnection::new(
            Arc::clone(&self.shared_state),
        )
    }
}
#[async_trait]
impl RocketBotInterface for ServerConnection {
    async fn send_channel_message_advanced(&self, channel_name: &str, message: OutgoingMessage) -> Option<String> {
        let channel_opt = {
            let cdb_guard = self.shared_state.subscribed_channels
                .read().await;
            cdb_guard.get_channel_by_name(channel_name).map(|c| c.clone())
        };
        let channel = if let Some(c) = channel_opt {
            c
        } else {
            warn!("trying to send message to unknown channel {:?}", channel_name);
            return None;
        };

        do_send_channel_message(&self.shared_state, &channel, message).await
    }

    async fn send_private_message_advanced(&self, conversation_id: &str, message: OutgoingMessage) -> Option<String> {
        let convo_opt = {
            let cdb_guard = self.shared_state.subscribed_channels
                .read().await;
            cdb_guard.get_private_conversation_by_id(conversation_id).map(|c| c.clone())
        };
        let convo = if let Some(c) = convo_opt {
            c
        } else {
            warn!("trying to send message to unknown private conversation {:?}", conversation_id);
            return None;
        };

        do_send_private_message(&self.shared_state, &convo, message).await
    }

    async fn send_private_message_to_user_advanced(&self, username: &str, message: OutgoingMessage) {
        // find the channel with that user
        let pc_opt = {
            let channel_guard = self.shared_state.subscribed_channels
                .read().await;
            channel_guard
                .get_private_conversation_id_by_counterpart_username(username)
                .map(|chid| channel_guard.get_private_conversation_by_id(&chid).map(|c| c.clone()))
                .flatten()
        };

        if let Some(pc) = pc_opt {
            // send directly to the existing private conversation
            do_send_private_message(&self.shared_state, &pc, message).await;
        } else {
            // remember this message for when the room is created
            {
                let mut initpm_guard = self.shared_state.username_to_initial_private_message
                    .lock().await;
                initpm_guard.insert(username.to_owned(), message);
            }

            // create a new PM channel
            let message_body = serde_json::json!({
                "msg": "method",
                "method": "createDirectMessage",
                "id": format!("create_dm_{}", username),
                "params": [
                    username,
                ],
            });
            self.shared_state.outgoing_sender.send(message_body)
                .expect("failed to enqueue create-DM message");
        }
    }

    async fn send_channel_message_with_attachment(&self, channel_name: &str, message: OutgoingMessageWithAttachment) -> Option<String> {
        let channel_opt = {
            let cdb_guard = self.shared_state.subscribed_channels
                .read().await;
            cdb_guard.get_channel_by_name(channel_name).map(|c| c.clone())
        };
        let channel = if let Some(c) = channel_opt {
            c
        } else {
            warn!("trying to send message with attachment to unknown channel {:?}", channel_name);
            return None;
        };

        do_send_channel_message_with_attachment(&self.shared_state, &channel, message).await
    }

    async fn send_private_message_with_attachment(&self, conversation_id: &str, message: OutgoingMessageWithAttachment) -> Option<String> {
        let convo_opt = {
            let cdb_guard = self.shared_state.subscribed_channels
                .read().await;
            cdb_guard.get_private_conversation_by_id(conversation_id).map(|c| c.clone())
        };
        let convo = if let Some(c) = convo_opt {
            c
        } else {
            warn!("trying to send message with attachment to unknown private conversation {:?}", conversation_id);
            return None;
        };

        do_send_private_message_with_attachment(&self.shared_state, &convo, message).await
    }

    async fn send_private_message_to_user_with_attachment(&self, username: &str, message: OutgoingMessageWithAttachment) {
        // find the channel with that user
        let pc_opt = {
            let channel_guard = self.shared_state.subscribed_channels
                .read().await;
            channel_guard
                .get_private_conversation_id_by_counterpart_username(username)
                .map(|chid| channel_guard.get_private_conversation_by_id(&chid).map(|c| c.clone()))
                .flatten()
        };

        if let Some(pc) = pc_opt {
            // send directly to the existing private conversation
            do_send_private_message_with_attachment(&self.shared_state, &pc, message).await;
        } else {
            // remember this message for when the room is created
            {
                let mut initpma_guard = self.shared_state.username_to_initial_private_message_with_attachment
                    .lock().await;
                initpma_guard.insert(username.to_owned(), message);
            }

            // create a new PM channel
            let message_body = serde_json::json!({
                "msg": "method",
                "method": "createDirectMessage",
                "id": format!("create_dm_{}", username),
                "params": [
                    username,
                ],
            });
            self.shared_state.outgoing_sender.send(message_body)
                .expect("failed to enqueue create-DM message");
        }
    }

    async fn resolve_username(&self, username: &str) -> Option<String> {
        // ask all plugins, stop at the first non-None result
        {
            let plugins = self.shared_state.plugins
                .read().await;
            debug!("asking plugins to resolve username {:?}", username);
            for plugin in plugins.iter() {
                if let Some(un) = plugin.plugin.username_resolution(username).await {
                    return Some(un);
                }
            }
        }
        None
    }

    async fn obtain_users_in_channel(&self, channel_name: &str) -> Option<HashSet<User>> {
        let chan_guard = self.shared_state.subscribed_channels
            .read().await;
        let chan = match chan_guard.get_channel_by_name(channel_name) {
            None => return None,
            Some(c) => c,
        };
        let users = chan_guard.users_in_channel(&chan.id);
        Some(users)
    }

    async fn obtain_server_roles(&self) -> Option<HashSet<String>> {
        let no_query_options: [(&str, Option<&str>); 0] = [];
        let roles_res = get_api_json(
            &self.shared_state,
            "api/v1/roles.list",
            no_query_options,
        ).await;
        let roles = match roles_res {
            Ok(r) => r,
            Err(e) => {
                error!("HTTP error while fetching roles: {}", e);
                return None;
            },
        };

        if !roles["success"].as_bool().unwrap_or(false) {
            error!("response error obtaining server roles: {}", roles);
            return None;
        }

        let mut ret = HashSet::new();
        let role_iter = match roles["roles"].members() {
            Some(ri) => ri,
            None => {
                error!("server roles \"roles\" member is not a list: {}", roles);
                return None;
            },
        };
        for role in role_iter {
            let name = match role["name"].as_str() {
                Some(n) => n.to_owned(),
                None => {
                    warn!("server role entry does not contain \"name\" string; skipping: {}", role);
                    continue;
                },
            };
            ret.insert(name);
        }
        Some(ret)
    }

    async fn obtain_users_with_server_role(&self, role: &str) -> Option<HashSet<User>> {
        let users_res = get_api_json(
            &self.shared_state,
            "api/v1/roles.getUsersInRole",
            [("role", Some(role))],
        ).await;
        let users = match users_res {
            Ok(u) => u,
            Err(e) => {
                error!("HTTP error obtaining users in role: {}", e);
                return None;
            },
        };

        if !users["success"].as_bool().unwrap_or(false) {
            error!("response error obtaining users in role: {}", users);
            return None;
        }

        let mut ret = HashSet::new();
        let user_iter = match users["users"].members() {
            Some(ui) => ui,
            None => {
                error!("server roles users \"users\" member is not a list: {}", users);
                return None;
            },
        };
        for user_json in user_iter {
            let user_id = match user_json["_id"].as_str() {
                Some(s) => s,
                None => {
                    warn!("server role user does not have ID, skipping: {:?}", user_json);
                    continue;
                }
            };
            let username = match user_json["username"].as_str() {
                Some(s) => s,
                None => {
                    warn!("server role user {:?} does not have username, skipping: {:?}", user_id, user_json);
                    continue;
                }
            };
            let nickname = user_json["name"].as_str();

            let user = User::new(
                user_id.to_owned(),
                username.to_owned(),
                nickname.map(|n| n.to_owned()),
            );
            ret.insert(user);
        }

        Some(ret)
    }

    async fn register_channel_command(&self, command: &CommandDefinition) -> bool {
        let mut commands_guard = self.shared_state.channel_commands
            .write().await;
        let mut my_command = command.clone();
        self.downcase_command_if_needed_mut(&mut my_command.name);
        match commands_guard.entry(my_command.name.clone()) {
            HashMapEntry::Occupied(_) => {
                false
            },
            HashMapEntry::Vacant(ve) => {
                ve.insert(my_command);
                true
            },
        }
    }

    async fn register_private_message_command(&self, command: &CommandDefinition) -> bool {
        let mut commands_guard = self.shared_state.private_message_commands
            .write().await;
        let mut my_command = command.clone();
        if self.shared_state.command_config.case_fold_commands {
            my_command.name = my_command.name.to_lowercase();
        }
        match commands_guard.entry(my_command.name.clone()) {
            HashMapEntry::Occupied(_) => {
                false
            },
            HashMapEntry::Vacant(ve) => {
                ve.insert(my_command);
                true
            },
        }
    }

    async fn unregister_channel_command(&self, command_name: &str) -> bool {
        let mut commands_guard = self.shared_state.channel_commands
            .write().await;
        let downcased_name = self.downcase_command_if_needed(command_name);
        commands_guard.remove(downcased_name.as_ref()).is_some()
    }

    async fn unregister_private_message_command(&self, command_name: &str) -> bool {
        let mut commands_guard = self.shared_state.private_message_commands
            .write().await;
        let downcased_name = self.downcase_command_if_needed(command_name);
        commands_guard.remove(downcased_name.as_ref()).is_some()
    }

    async fn get_command_configuration(&self) -> CommandConfiguration {
        self.shared_state.command_config.clone()
    }

    async fn get_defined_channel_commands(&self, plugin: Option<&str>) -> Vec<CommandDefinition> {
        let commands_guard = self.shared_state.channel_commands
            .read().await;
        let mut commands: Vec<CommandDefinition> = commands_guard.values()
            .filter(|cd|
                if let Some(p) = plugin {
                    if let Some(cdpn) = &cd.plugin_name {
                        cdpn == p
                    } else {
                        true
                    }
                } else {
                    true
                }
            )
            .map(|cd| cd.clone())
            .collect();
        commands.sort_unstable_by_key(|cd| cd.name.clone());
        commands
    }

    async fn get_additional_channel_commands_usages(&self, plugin: Option<&str>) -> HashMap<String, (String, String)> {
        let mut ret = HashMap::new();
        {
            let loaded_plugins = self.shared_state.plugins
                .read().await;
            for loaded_plugin in loaded_plugins.iter() {
                if let Some(po) = plugin {
                    let plugin_name = loaded_plugin.plugin.plugin_name().await;
                    if po != plugin_name {
                        continue;
                    }
                }
                let mut commands_usages = loaded_plugin.plugin.get_additional_channel_commands_usages().await;
                ret.extend(commands_usages.drain());
            }
        }
        ret
    }

    async fn get_defined_private_message_commands(&self, plugin: Option<&str>) -> Vec<CommandDefinition> {
        let commands_guard = self.shared_state.private_message_commands
            .read().await;
        let mut commands: Vec<CommandDefinition> = commands_guard.values()
            .filter(|cd|
                if let Some(p) = plugin {
                    if let Some(cdpn) = &cd.plugin_name {
                        cdpn == p
                    } else {
                        true
                    }
                } else {
                    true
                }
            )
            .map(|cd| cd.clone())
            .collect();
        commands.sort_unstable_by_key(|cd| cd.name.clone());
        commands
    }

    async fn get_additional_private_message_commands_usages(&self, plugin: Option<&str>) -> HashMap<String, (String, String)> {
        let mut ret = HashMap::new();
        {
            let loaded_plugins = self.shared_state.plugins
                .read().await;
            for loaded_plugin in loaded_plugins.iter() {
                if let Some(po) = plugin {
                    let plugin_name = loaded_plugin.plugin.plugin_name().await;
                    if po != plugin_name {
                        continue;
                    }
                }
                let mut commands_usages = loaded_plugin.plugin.get_additional_private_message_commands_usages().await;
                ret.extend(commands_usages.drain());
            }
        }
        ret
    }

    async fn get_command_help(&self, name: &str) -> Option<String> {
        // ask all plugins, stop at the first non-None result
        {
            let plugins = self.shared_state.plugins
                .read().await;
            debug!("asking plugins to return help for {:?}", name);
            for plugin in plugins.iter() {
                if let Some(un) = plugin.plugin.get_command_help(name).await {
                    return Some(un);
                }
            }
        }
        None
    }

    async fn is_my_user_id(&self, user_id: &str) -> bool {
        let uid_guard = self.shared_state.my_user_id
            .read().await;
        match uid_guard.deref() {
            Some(uid) => uid == user_id,
            None => false,
        }
    }

    async fn obtain_bot_user_ids(&self) -> HashSet<String> {
        let bots_guard = self.shared_state.bot_user_ids
            .read().await;
        (*bots_guard).clone()
    }

    async fn get_plugin_names(&self) -> Vec<String> {
        let plugins = self.shared_state.plugins
            .read().await;
        debug!("asking plugins to return their names");
        let mut plugin_names = Vec::new();
        for plugin in plugins.iter() {
            let name = plugin.plugin.plugin_name().await;
            plugin_names.push(name);
        }
        plugin_names
    }

    async fn get_maximum_message_length(&self) -> Option<usize> {
        let settings_guard = self.shared_state.server_settings
            .read().await;
        settings_guard.max_message_length
    }

    async fn get_username_regex_string(&self) -> String {
        let settings_guard = self.shared_state.server_settings
            .read().await;
        settings_guard.username_regex.as_str().to_owned()
    }

    async fn register_timer(&self, timestamp: DateTime<Utc>, custom_data: serde_json::Value) {
        if let Err(e) = self.shared_state.new_timer_sender.send((timestamp, custom_data)) {
            error!("error registering new timer: {}", e);
        }
    }

    async fn get_channel_text(&self, channel_name: &str, text_type: ChannelTextType) -> Option<String> {
        let channel_id = {
            let channel_guard = self.shared_state.subscribed_channels
                .read().await;
            if let Some(channel) = channel_guard.get_channel_by_name(channel_name) {
                channel.id.clone()
            } else {
                return None;
            }
        };

        let text_guard = self.shared_state.channel_id_to_texts
            .read().await;
        text_guard.get(&(channel_id, text_type))
            .map(|s| s.clone())
    }

    async fn set_channel_text(&self, channel_name: &str, text_type: ChannelTextType, text: &str) {
        let channel_id = {
            let channel_guard = self.shared_state.subscribed_channels
                .read().await;
            if let Some(channel) = channel_guard.get_channel_by_name(channel_name) {
                channel.id.clone()
            } else {
                return;
            }
        };
        let realtime_api_text_type = match text_type {
            ChannelTextType::Announcement => "roomAnnouncement",
            ChannelTextType::Description => "roomDescription",
            ChannelTextType::Topic => "roomTopic",
        };
        let message_body = serde_json::json!({
            "msg": "method",
            "method": "saveRoomSettings",
            "id": format!("set_text_{}", channel_id),
            "params": [
                channel_id,
                realtime_api_text_type,
                text,
            ],
        });
        self.shared_state.outgoing_sender.send(message_body)
            .expect("failed to enqueue change-channel-text message");
    }

    async fn obtain_emoji(&self) -> Vec<Emoji> {
        let emoji_guard = self.shared_state.emoji
            .read().await;
        emoji_guard.deref().clone()
    }

    async fn add_reaction(&self, message_id: &str, emoji_short_name: &str) {
        debug!("reacting with emoji {emoji_short_name:?} to message {message_id:?}");
        let no_query_options: [(&str, Option<&str>); 0] = [];
        let res = post_api_json(
            &self.shared_state,
            "api/v1/chat.react",
            serde_json::json!({
                "messageId": message_id,
                "emoji": emoji_short_name,
                "shouldReact": true,
            }),
            no_query_options,
        ).await;
        if let Err(e) = res {
            error!("failed to react with emoji {emoji_short_name:?} to message {message_id:?}: {e}");
        }
    }

    async fn remove_reaction(&self, message_id: &str, emoji_short_name: &str) {
        debug!("removing reaction with emoji {emoji_short_name:?} from message {message_id:?}");
        let no_query_options: [(&str, Option<&str>); 0] = [];
        let res = post_api_json(
            &self.shared_state,
            "api/v1/chat.react",
            serde_json::json!({
                "messageId": message_id,
                "emoji": emoji_short_name,
                "shouldReact": false,
            }),
            no_query_options,
        ).await;
        if let Err(e) = res {
            error!("failed to remove reaction with emoji {emoji_short_name:?} from message {message_id:?}: {e}");
        }
    }

    async fn obtain_http_resource(&self, path: &str) -> Result<hyper::Response<hyper::body::Incoming>, HttpError> {
        let query_options: Vec<(String, Option<String>)> = Vec::with_capacity(0);
        get_http_from_server(&self.shared_state, path, query_options).await
    }

    async fn set_channel_typing_status(&self, channel_name: &str, typing: bool) {
        // get my username
        let username = {
            let config_guard = CONFIG
                .get().expect("config is set")
                .read().await;
            config_guard.server.username.clone()
        };

        // find channel ID
        let channel_id = {
            let sub_chan_guard = self.shared_state.subscribed_channels
                .read().await;
            match sub_chan_guard.get_channel_by_name(channel_name) {
                Some(ch) => ch.id.clone(),
                None => {
                    error!("failed to find channel named {:?}; cannot set typing status", channel_name);
                    return;
                }
            }
        };

        let message_body = serde_json::json!({
            "msg": "method",
            "method": "stream-notify-room",
            "id": "nvm",
            "params": [
                format!("{}/typing", channel_id),
                username,
                typing,
            ],
        });
        self.shared_state.outgoing_sender.send(message_body)
            .expect("failed to enqueue set-typing message");
    }

    async fn set_private_conversation_typing_status(&self, conversation_id: &str, typing: bool) {
        // get my username
        let username = {
            let config_guard = CONFIG
                .get().expect("config is set")
                .read().await;
            config_guard.server.username.clone()
        };

        let message_body = serde_json::json!({
            "msg": "method",
            "method": "stream-notify-room",
            "id": "nvm",
            "params": [
                format!("{}/typing", conversation_id),
                username,
                typing,
            ],
        });
        self.shared_state.outgoing_sender.send(message_body)
            .expect("failed to enqueue set-typing message");
    }

    async fn obtain_behavior_flags(&self) -> serde_json::Map<String, serde_json::Value> {
        let cloned_flags = {
            let flags_guard = self.shared_state.active_behavior_flags
                .read().await;
            flags_guard.deref().clone()
        };
        debug!("currently set behavior flags: {:?}", cloned_flags);
        cloned_flags
    }

    async fn set_behavior_flag(&self, key: &str, value: &serde_json::Value) {
        let owned_key = key.to_owned();
        let owned_value = value.clone();

        {
            let mut flags_guard = self.shared_state.active_behavior_flags
                .write().await;
            flags_guard.insert(owned_key, owned_value);
            debug!("behavior flags after setting {:?} to {}: {:?}", key, value, flags_guard.deref());
        }
    }

    async fn remove_behavior_flag(&self, key: &str) {
        {
            let mut flags_guard = self.shared_state.active_behavior_flags
                .write().await;
            flags_guard.remove(key);
            debug!("behavior flags after removing {:?}: {:?}", key, flags_guard.deref());
        }
    }

    async fn reload_configuration(&self) {
        if let Err(e) = self.shared_state.reload_config_sender.try_send(()) {
            error!("failed to trigger configuration reload: {}", e);
        }
    }
}


async fn generate_from_alphabet<R: Rng>(rng_lock: &Mutex<R>, alphabet: &str, output_length: usize) -> String {
    let alphabet_chars: Vec<char> = alphabet.chars().collect();
    let distribution = Uniform::new(0, alphabet_chars.len());
    let mut message_id = String::with_capacity(output_length);

    {
        let mut rng_guard = rng_lock.lock().await;
        for _ in 0..output_length {
            message_id.push(alphabet_chars[distribution.sample(rng_guard.deref_mut())]);
        }
    }

    message_id
}

async fn generate_message_id<R: Rng>(rng_lock: &Mutex<R>) -> String {
    generate_from_alphabet(rng_lock, ID_ALPHABET, ID_LENGTH).await
}

async fn generate_boundary_text<R: Rng>(rng_lock: &Mutex<R>) -> String {
    let mut s = generate_from_alphabet(rng_lock, BOUNDARY_ALPHABET, BOUNDARY_LENGTH).await;
    // convention dictates lots of dashes
    s.insert_str(0, "------------------------");
    s
}


pub(crate) async fn connect() -> Arc<ServerConnection> {
    debug!("connect: assembling state");
    let (outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
    let exit_notify = Notify::new();
    let subscribed_channels = RwLock::new(
        "SharedConnectionState::subscribed_channels",
        ChannelDatabase::new_empty(),
    );
    let plugins = RwLock::new(
        "SharedConnectionState::plugins",
        Vec::new(),
    );
    let rng = Mutex::new(
        "SharedConnectionState::rng",
        StdRng::from_entropy(),
    );
    let command_config = {
        let config_guard = CONFIG
            .get().expect("CONFIG is not set?!")
            .read().await;
        config_guard.commands.clone()
    };
    let channel_commands = RwLock::new(
        "SharedConnectionState::channel_commands",
        HashMap::new(),
    );
    let private_message_commands = RwLock::new(
        "SharedConnectionState::private_message_commands",
        HashMap::new(),
    );
    let https_connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("failed to create HttpsConnectorBuilder with native roots")
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();
    let http_client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(https_connector);
    let my_user_id: RwLock<Option<String>> = RwLock::new(
        "SharedConnectionState::my_user_id",
        None,
    );
    let my_auth_token: RwLock<Option<String>> = RwLock::new(
        "SharedConnectionState::my_auth_token",
        None,
    );
    let server_settings: RwLock<ServerSettings> = RwLock::new(
        "SharedConnectionState::server_settings",
        ServerSettings::default(),
    );
    let username_to_initial_private_message = Mutex::new(
        "SharedConnectionState::username_to_initial_private_message",
        HashMap::new(),
    );
    let username_to_initial_private_message_with_attachment = Mutex::new(
        "SharedConnectionState::username_to_initial_private_message_with_attachment",
        HashMap::new(),
    );
    let (new_timer_sender, new_timer_receiver) = mpsc::unbounded_channel();
    let (reload_config_sender, reload_config_receiver) = mpsc::channel(1);
    let channel_id_to_texts = RwLock::new(
        "SharedConnectionState::channel_id_to_texts",
        HashMap::new(),
    );
    let emoji = RwLock::new(
        "SharedConnectionState::emoji",
        Vec::new(),
    );
    let active_behavior_flags = RwLock::new(
        "SharedConnectionState::active_behavior_flags",
        serde_json::Map::new(),
    );
    let bot_user_ids = RwLock::new(
        "SharedConnectionState::bot_user_ids",
        HashSet::new(),
    );

    let shared_state = Arc::new(SharedConnectionState::new(
        outgoing_sender,
        exit_notify,
        plugins,
        subscribed_channels,
        rng,
        command_config,
        channel_commands,
        private_message_commands,
        http_client,
        my_user_id,
        my_auth_token,
        server_settings,
        username_to_initial_private_message,
        username_to_initial_private_message_with_attachment,
        new_timer_sender,
        reload_config_sender,
        channel_id_to_texts,
        emoji,
        active_behavior_flags,
        bot_user_ids,
    ));

    // start the message handler
    let (process_message_sender, process_message_receiver) = mpsc::unbounded_channel();
    let shared_state_weak = Arc::downgrade(&shared_state);
    tokio::spawn(async move {
        message_handler(shared_state_weak, process_message_receiver).await
    });

    let conn = Arc::new(ServerConnection::new(
        Arc::clone(&shared_state),
    ));
    let mut state = ConnectionState::new(
        shared_state,
        outgoing_receiver,
        Vec::new(),
        new_timer_receiver,
        reload_config_receiver,
        Utc
            .with_ymd_and_hms(1969, 1, 1, 0, 0, 0).unwrap(),
        process_message_sender,
    );
    let second_conn: Arc<ServerConnection> = Arc::clone(&conn);
    let generic_conn: Arc<dyn RocketBotInterface> = second_conn;

    debug!("connect: loading plugins");
    let mut loaded_plugins: Vec<Plugin> = load_plugins(Arc::downgrade(&generic_conn))
        .await;
    {
        let mut plugins_guard = state.shared_state.plugins
            .write().await;
        plugins_guard.append(&mut loaded_plugins);
    }

    debug!("connect: obtaining builtin emoji");
    let mut builtin_emoji = obtain_builtin_emoji(&mut state).await;
    {
        let mut emoji_guard = state.shared_state.emoji
            .write().await;
        emoji_guard.append(&mut builtin_emoji);
    }

    debug!("connect: spawning connection handling");
    tokio::spawn(async move {
        run_connections(state).await
    });

    conn
}


async fn run_connections(mut state: ConnectionState) {
    loop {
        // run a new connection
        match run_connection(&mut state).await {
            Ok(()) => break, // graceful disconnection
            Err(e) => {
                tracing::error!("{}", e);
            },
        };

        // wait a bit before reconnecting
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}


fn sha256_hexdigest(data: &[u8]) -> String {
    let mut sha256 = Sha256::new();
    sha256.update(data);
    let digest = sha256.finalize();

    let mut ret = String::with_capacity(digest.len());
    for b in digest {
        write!(&mut ret, "{:02x}", b)
            .expect("failed to write a byte");
    }
    ret
}


async fn run_connection(mut state: &mut ConnectionState) -> Result<(), WebSocketError> {
    let (websocket_uri, rate_limit_config) = {
        let config_lock = CONFIG
            .get().expect("no initial configuration set")
            .read().await;
        (config_lock.server.websocket_uri.clone(), config_lock.server.rate_limit.clone())
    };

    debug!("run_connection: establishing connection with {}", websocket_uri);
    let (websocket_stream, _response) = connect_async(&websocket_uri).await
        .map_err(|e| WebSocketError::Connecting(e))?;
    let mut stream = MaybeRateLimitedStream::new(websocket_stream, rate_limit_config);

    // connect!
    let connect_message = serde_json::json!({
        "msg": "connect",
        "version": "1",
        "support": ["1"]
    });
    state.shared_state.outgoing_sender.send(connect_message)
        .expect("failed to enqueue connect message");

    loop {
        // calculate the duration to the next timer
        let (next_timer_dur, next_timer_info) = if let Some((timestamp, info)) = state.timers.get(0) {
            let now = Utc::now();
            // saturate down to zero if the timestamp has already elapsed
            let duration = timestamp.signed_duration_since(now)
                .to_std()
                .unwrap_or(Duration::from_secs(0));
            (duration, info.clone())
        } else {
            (Duration::MAX, serde_json::Value::Null)
        };

        tokio::select! {
            _ = state.shared_state.exit_notify.notified() => {
                debug!("graceful exit requested");
                break;
            },
            received = stream.next() => {
                // message received
                let msg: WebSocketMessage = match received {
                    None => return Err(WebSocketError::StreamClosed),
                    Some(Err(e)) => return Err(WebSocketError::ReceivingMessage(e)),
                    Some(Ok(m)) => m,
                };

                // answer WebSocket ping with pong
                if let WebSocketMessage::Ping(data) = msg {
                    let response = WebSocketMessage::Pong(data);
                    if let Err(e) = stream.send(response).await {
                        return Err(WebSocketError::SendingMessage(e));
                    }
                    continue;
                }

                if let WebSocketMessage::Text(body_string) = msg {
                    debug!("message received: {:?}", body_string);

                    let body: serde_json::Value = match serde_json::from_str(&body_string) {
                        Ok(b) => b,
                        Err(e) => {
                            error!("failed to parse message {:?} ({}) -- skipping", body_string, e);
                            continue;
                        }
                    };
                    handle_received(&body, &mut state).await;
                }
            },
            send_me = state.outgoing_receiver.recv() => {
                let content = match send_me {
                    None => return Err(WebSocketError::OutgoingQueueClosed),
                    Some(c) => c,
                };

                let content_text = content.to_string();
                debug!("sending message: {:?}", content_text);
                let msg = WebSocketMessage::Text(content_text.into());
                if let Err(e) = stream.send(msg).await {
                    return Err(WebSocketError::SendingMessage(e));
                }
            },
            _ = tokio::time::sleep(next_timer_dur), if next_timer_dur != Duration::MAX => {
                debug!("timer elapsed: {:?}", next_timer_info);

                // deliver the timer
                deliver_timer(&next_timer_info, &state).await;

                // remove it
                state.timers.remove(0);
            },
            new_timer_opt = state.new_timer_receiver.recv() => {
                if let Some(new_timer) = new_timer_opt {
                    debug!("new timer received: at {} with info {:?}", new_timer.0, new_timer.1);

                    // ensure vector sorted ascending by timestamp
                    state.timers.push(new_timer);
                    state.timers.sort_unstable_by_key(|t| t.0);

                    // on the next loop; the freshest timer will be chosen
                } else {
                    error!("lost the timer channel!");

                    // break out, lest we loop infinitely, receiving None every time
                    break;
                }
            },
            _ = state.reload_config_receiver.recv() => {
                do_config_reload(&mut state).await;
            },
        };
    }

    Ok(())
}

async fn do_send_any_message(shared_state: &SharedConnectionState, target_id: &str, message: OutgoingMessage) -> String {
    // make an ID for this message
    let message_id = generate_message_id(&shared_state.rng).await;
    let mut message_body = serde_json::json!({
        "msg": "method",
        "method": "sendMessage",
        "id": SEND_MESSAGE_MESSAGE_ID,
        "params": [
            {
                "_id": message_id,
                "rid": target_id,
                "msg": message.body,
            },
        ],
    });
    if let Some(impersonation) = message.impersonation {
        message_body["params"][0].insert("alias".to_owned(), serde_json::Value::String(impersonation.nickname));
        message_body["params"][0].insert("avatar".to_owned(), serde_json::Value::String(impersonation.avatar_url));
    }
    if let Some(rtmid) = message.reply_to_message_id {
        message_body["params"][0].insert("tmid".to_owned(), serde_json::Value::String(rtmid));
    }
    shared_state.outgoing_sender.send(message_body)
        .expect("failed to enqueue channel message");
    message_id
}

async fn do_send_any_message_with_attachment(shared_state: &SharedConnectionState, target_id: &str, message: OutgoingMessageWithAttachment) -> Option<String> {
    // make a boundary text
    let boundary_text = generate_boundary_text(&shared_state.rng).await;

    // assemble the request body
    let mut body = Vec::new();

    // -> file
    write!(body, "--{}\r\n", boundary_text).unwrap();
    write!(body, "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n", message.attachment.file_name).unwrap();
    write!(body, "Content-Type: {}\r\n", message.attachment.mime_type).unwrap();
    write!(body, "\r\n").unwrap();
    body.extend_from_slice(&message.attachment.data);
    write!(body, "\r\n").unwrap();

    // trailing boundary
    write!(body, "--{}--\r\n", boundary_text).unwrap();

    // collect data for headers
    let user_id = {
        let uid_guard = shared_state.my_user_id
            .read().await;
        match uid_guard.deref() {
            Some(uid) => uid.clone(),
            None => {
                error!("cannot send message with attachment; user ID is missing!");
                return None;
            },
        }
    };
    let auth_token = {
        let token_guard = shared_state.my_auth_token
            .read().await;
        match token_guard.deref() {
            Some(tok) => tok.clone(),
            None => {
                error!("cannot send message with attachment; auth token is missing!");
                return None;
            },
        }
    };
    let web_uri = {
        let config_lock = CONFIG
            .get().expect("no initial configuration set")
            .read().await;
        Url::parse(&config_lock.server.web_uri)
            .expect("failed to parse web URI")
    };

    let mut full_uri = web_uri.clone();
    {
        let mut path_segments = full_uri.path_segments_mut().unwrap();
        path_segments.push("api");
        path_segments.push("v1");
        path_segments.push("rooms.media");
        path_segments.push(target_id);
    }

    let request = hyper::Request::builder()
        .method("POST")
        .uri(full_uri.as_str())
        .header("Content-Type", format!("multipart/form-data; boundary={}", boundary_text))
        .header("X-User-Id", &user_id)
        .header("X-Auth-Token", &auth_token)
        .body(Full::new(Bytes::from(body)))
        .expect("failed to construct request");

    debug!("sending message with attachment: {:?}", request);

    // send
    let response_res = shared_state.http_client
        .request(request).await;
    let response = match response_res {
        Ok(r) => r,
        Err(e) => {
            error!("cannot send message with attachment; failed to send request: {}", e);
            return None;
        },
    };

    // obtain content
    let (response_header, response_body) = response.into_parts();
    let response_bytes: Vec<u8> = match response_body.collect().await {
        Ok(rb) => rb.to_bytes().to_vec(),
        Err(e) => {
            error!("cannot send message with attachment; failed to obtain response bytes: {}", e);
            return None;
        },
    };

    if response_header.status != StatusCode::OK {
        error!("cannot send message with attachment; response code is not OK but {} (body is {:?})", response_header.status, response_bytes);
        return None;
    }

    let response_json: serde_json::Value = match serde_json::from_slice(&response_bytes) {
        Ok(rj) => rj,
        Err(e) => {
            error!("failed to parse send-message-with-attachment response: {} (response is {:?})", e, response_bytes);
            return None;
        },
    };
    if !response_json["success"].as_bool().unwrap_or(false) {
        error!("send-message-with-attachment not successful: {:?}", response_json);
        return None;
    }

    let file_id = match response_json["file"]["_id"].as_str() {
        Some(fid) => fid.to_owned(),
        None => {
            warn!("send-message-with-attachment response does not contain message ID string $.file._id: {:?}", response_json);
            return None;
        },
    };

    // second step to attach all the other information

    let mut confirm_full_uri = web_uri.clone();
    {
        let mut path_segments = confirm_full_uri.path_segments_mut().unwrap();
        path_segments.push("api");
        path_segments.push("v1");
        path_segments.push("rooms.mediaConfirm");
        path_segments.push(target_id);
        path_segments.push(&file_id);
    }

    let mut confirm_json_data = HashMap::new();
    confirm_json_data.insert("msg".to_owned(), String::new());
    confirm_json_data.insert("description".to_owned(), String::new());
    if let Some(b) = &message.body {
        confirm_json_data.insert("msg".to_owned(), b.clone());
    }
    if let Some(d) = &message.attachment.description {
        confirm_json_data.insert("description".to_owned(), d.clone());
    }
    if let Some(t) = &message.reply_to_message_id {
        confirm_json_data.insert("tmid".to_owned(), t.clone());
    }
    let confirm_json_string = serde_json::to_string(&confirm_json_data)
        .expect("failed to serialize confirmation JSON");

    let confirm_request = hyper::Request::builder()
        .method("POST")
        .uri(confirm_full_uri.as_str())
        .header("Content-Type", "application/json")
        .header("X-User-Id", &user_id)
        .header("X-Auth-Token", &auth_token)
        .body(Full::new(Bytes::from(confirm_json_string)))
        .expect("failed to construct confirmation request");

    debug!("confirming message with attachment: {:?}", confirm_request);

    let confirm_response_res = shared_state.http_client
        .request(confirm_request).await;
    let confirm_response = match confirm_response_res {
        Ok(r) => r,
        Err(e) => {
            error!("cannot confirm message with attachment; failed to send request: {}", e);
            return None;
        },
    };
    let (confirm_response_header, confirm_response_body) = confirm_response.into_parts();
    let confirm_response_bytes: Vec<u8> = match confirm_response_body.collect().await {
        Ok(rb) => rb.to_bytes().to_vec(),
        Err(e) => {
            error!("cannot confirm message with attachment; failed to obtain response bytes: {}", e);
            return None;
        },
    };
    if confirm_response_header.status != StatusCode::OK {
        error!("cannot confirm message with attachment; response code is not OK but {} (body is {:?})", confirm_response_header.status, confirm_response_bytes);
        return None;
    }

    let confirm_response_json: serde_json::Value = match serde_json::from_slice(&confirm_response_bytes) {
        Ok(rj) => rj,
        Err(e) => {
            error!("failed to parse confirm-message-with-attachment response: {} (response is {:?})", e, confirm_response_bytes);
            return None;
        },
    };
    if !confirm_response_json["success"].as_bool().unwrap_or(false) {
        error!("confirm-message-with-attachment not successful: {:?}", confirm_response_json);
        return None;
    }

    let message_id = match confirm_response_json["message"]["_id"].as_str() {
        Some(mid) => mid.to_owned(),
        None => {
            warn!("send-message-with-attachment response does not contain message ID string $.message._id: {:?}", confirm_response_json);
            return None;
        },
    };
    Some(message_id)
}

async fn do_send_channel_message(shared_state: &SharedConnectionState, channel: &Channel, message: OutgoingMessage) -> Option<String> {
    {
        // let the plugins review and possibly block the message
        let plugins = shared_state.plugins
            .read().await;
        debug!("asking plugins to review a message");
        for plugin in plugins.iter() {
            if !plugin.plugin.outgoing_channel_message(&channel, &message).await {
                return None;
            }
        }
    }

    Some(do_send_any_message(shared_state, &channel.id, message).await)
}

async fn do_send_private_message(shared_state: &SharedConnectionState, convo: &PrivateConversation, message: OutgoingMessage) -> Option<String> {
    {
        // let the plugins review and possibly block the message
        let plugins = shared_state.plugins
            .read().await;
        debug!("asking plugins to review a message");
        for plugin in plugins.iter() {
            if !plugin.plugin.outgoing_private_message(&convo, &message).await {
                return None;
            }
        }
    }

    Some(do_send_any_message(shared_state, &convo.id, message).await)
}

async fn do_send_channel_message_with_attachment(shared_state: &SharedConnectionState, channel: &Channel, message: OutgoingMessageWithAttachment) -> Option<String> {
    {
        // let the plugins review and possibly block the message
        let plugins = shared_state.plugins
            .read().await;
        debug!("asking plugins to review a message");
        for plugin in plugins.iter() {
            if !plugin.plugin.outgoing_channel_message_with_attachment(&channel, &message).await {
                return None;
            }
        }
    }

    do_send_any_message_with_attachment(shared_state, &channel.id, message).await
}

async fn do_send_private_message_with_attachment(shared_state: &SharedConnectionState, convo: &PrivateConversation, message: OutgoingMessageWithAttachment) -> Option<String> {
    {
        // let the plugins review and possibly block the message
        let plugins = shared_state.plugins
            .read().await;
        debug!("asking plugins to review a message");
        for plugin in plugins.iter() {
            if !plugin.plugin.outgoing_private_message_with_attachment(&convo, &message).await {
                return None;
            }
        }
    }

    do_send_any_message_with_attachment(shared_state, &convo.id, message).await
}

async fn subscribe_to_messages(state: &mut ConnectionState, channel_id: &str) {
    // subscribe to messages in this room
    let sub_body = serde_json::json!({
        "msg": "sub",
        "id": format!("sub_{}", channel_id),
        "name": "stream-room-messages",
        "params": [
            channel_id,
            false,
        ],
    });
    state.shared_state.outgoing_sender.send(sub_body)
        .expect("failed to enqueue subscription message");
}

async fn subscribe_to_typing_events(state: &mut ConnectionState, channel_id: &str) {
    let sub_body = serde_json::json!({
        "msg": "sub",
        "id": format!("sub_notify_{}", channel_id),
        "name": "stream-notify-room",
        "params": [
            format!("{}/typing", channel_id),
            false,
        ],
    });
    state.shared_state.outgoing_sender.send(sub_body)
        .expect("failed to enqueue subscription message");
}

async fn channel_joined(mut state: &mut ConnectionState, channel: Channel) {
    debug!("joined channel {:?}; subscribing to messages", channel.id);

    {
        let mut cdb_write = state.shared_state.subscribed_channels
            .write().await;
        cdb_write.register_channel(channel.clone());
    }

    subscribe_to_messages(&mut state, &channel.id).await;
    subscribe_to_typing_events(&mut state, &channel.id).await;
    obtain_users_in_room(&mut state, &channel).await;
}

async fn private_conversation_joined(mut state: &mut ConnectionState, convo_id: &str, all_participants: Vec<User>) {
    debug!("joined private conversation {:?}; subscribing to messages", convo_id);

    let my_user_id = {
        let user_id_guard = state.shared_state.my_user_id
            .read().await;
        match user_id_guard.deref() {
            Some(uid) => uid.clone(),
            None => return,
        }
    };

    // remove ourselves from what will become the "other participants" list
    let mut other_participants = all_participants;
    let mut found_myself = false;
    let mut i = 0;
    while i < other_participants.len() {
        if other_participants[i].id == my_user_id {
            found_myself = true;
            other_participants.remove(i);
        } else {
            i += 1;
        }
    }

    if !found_myself {
        warn!("apparently I'm not a member of private conversation {:?}; skipping", convo_id);
        return;
    }

    other_participants.sort_unstable_by_key(|p| p.id.clone());

    let convo = PrivateConversation::new(
        convo_id.to_owned(),
        other_participants,
    );

    {
        let mut cdb_write = state.shared_state.subscribed_channels
            .write().await;
        cdb_write.register_private_conversation(convo.clone());
    }

    subscribe_to_messages(&mut state, &convo.id).await;
    subscribe_to_typing_events(&mut state, &convo.id).await;
}

async fn channel_left(state: &mut ConnectionState, channel_id: &str) {
    debug!("left channel {:?}; unsubscribing from messages", channel_id);

    {
        let mut cdb_write = state.shared_state.subscribed_channels
            .write().await;
        cdb_write.forget_by_id(channel_id);
    }

    // unsubscribe
    let unsub_body = serde_json::json!({
        "msg": "unsub",
        "id": format!("sub_{}", channel_id),
    });
    state.shared_state.outgoing_sender.send(unsub_body)
        .expect("failed to enqueue unsubscription message");
}

fn message_is_current(state: &mut ConnectionState, message: &Message) -> bool {
    let mut message_timestamp = message.timestamp;
    if let Some(ei) = &message.edit_info {
        // take the newer of both timestamps
        message_timestamp = message_timestamp.max(ei.timestamp);
    }

    if message_timestamp <= state.last_seen_message_timestamp {
        // message is old => assume it has been updated; don't deliver it
        debug!(
            "message timestamp {} <= last-seen timestamp {} -> not delivering message {:?}",
            message_timestamp, state.last_seen_message_timestamp, message.id,
        );
        false
    } else {
        state.last_seen_message_timestamp = message_timestamp;
        true
    }
}

#[derive(Debug)]
enum MessageToHandle {
    Channel {
        sender_id: String,
        my_user_id: String,
        message: ChannelMessage,
    },
    Private {
        sender_id: String,
        my_user_id: String,
        message: PrivateMessage,
    }
}

async fn message_handler(shared_state_weak: Weak<SharedConnectionState>, mut message_receiver: mpsc::UnboundedReceiver<MessageToHandle>) {
    while let Some(message) = message_receiver.recv().await {
        let shared_state = match shared_state_weak.upgrade() {
            Some(ss) => ss,
            None => {
                info!("shared state is gone; ending message handler loop");
                return;
            },
        };

        match message {
            MessageToHandle::Channel { sender_id, my_user_id, message }
                => handle_channel_message(shared_state, &sender_id, &my_user_id, &message).await,
            MessageToHandle::Private { sender_id, my_user_id, message }
                => handle_private_message(shared_state, &sender_id, &my_user_id, &message).await,
        }
    }
}

async fn handle_channel_message(shared_state: Arc<SharedConnectionState>, sender_id: &str, my_user_id: &str, channel_message: &ChannelMessage) {
    // distribute among plugins
    {
        let plugins = shared_state.plugins
            .read().await;
        debug!("distributing channel message {:?} among plugins", channel_message.message.id);
        for plugin in plugins.iter() {
            if channel_message.message.edit_info.is_some() {
                plugin.plugin.channel_message_edited(&channel_message).await;
            } else if sender_id == my_user_id {
                plugin.plugin.channel_message_delivered(&channel_message).await;
            } else {
                plugin.plugin.channel_message(&channel_message).await;
            }
        }
    }

    if channel_message.message.edit_info.is_none() && sender_id != my_user_id {
        // parse commands if there are any (not on edited messages or the bot's own messages!)
        distribute_channel_message_commands(&channel_message, shared_state).await;
    }
}

async fn handle_private_message(shared_state: Arc<SharedConnectionState>, sender_id: &str, my_user_id: &str, private_message: &PrivateMessage) {
    // distribute among plugins
    {
        let plugins = shared_state.plugins
            .read().await;
        debug!("distributing private message {:?} among plugins", private_message.message.id);
        for plugin in plugins.iter() {
            if private_message.message.edit_info.is_some() {
                plugin.plugin.private_message_edited(&private_message).await;
            } else if sender_id == my_user_id {
                plugin.plugin.private_message_delivered(&private_message).await;
            } else {
                plugin.plugin.private_message(&private_message).await;
            }
        }
    }

    if private_message.message.edit_info.is_none() {
        // parse commands if there are any (not on edited messages!)
        distribute_private_message_commands(&private_message, shared_state).await;
    }
}

async fn handle_received(body: &serde_json::Value, mut state: &mut ConnectionState) {
    if body["msg"] == "ping" {
        // answer with a pong
        let pong_body = serde_json::json!({"msg": "pong"});
        state.shared_state.outgoing_sender.send(pong_body)
            .expect("failed to enqueue pong message");
    } else if body["msg"] == "connected" {
        // login
        let (username, password) = {
            let config_lock = CONFIG
                .get().expect("no initial configuration set")
                .read().await;
            (
                config_lock.server.username.clone(),
                config_lock.server.password.clone(),
            )
        };
        let password_sha256 = sha256_hexdigest(password.as_bytes());

        let login_body = serde_json::json!({
            "msg": "method",
            "method": "login",
            "id": LOGIN_MESSAGE_ID,
            "params": [
                {
                    "user": {
                        "username": username.clone(),
                    },
                    "password": {
                        "digest": password_sha256.clone(),
                        "algorithm": "sha-256",
                    },
                },
            ],
        });
        state.shared_state.outgoing_sender.send(login_body)
            .expect("failed to enqueue login message");
    } else if body["msg"] == "result" && body["id"] == LOGIN_MESSAGE_ID {
        // login successful

        // store our ID and token
        let user_id = body["result"]["id"].as_str()
            .expect("user ID missing or not a string")
            .to_owned();
        let auth_token = body["result"]["token"].as_str()
            .expect("auth token missing or not a string")
            .to_owned();
        {
            let mut uid_guard = state.shared_state.my_user_id.write().await;
            *uid_guard = Some(user_id.clone());
        }
        {
            let mut token_guard = state.shared_state.my_auth_token.write().await;
            *token_guard = Some(auth_token.clone());
        }

        // subscribe to changes to our room state
        let subscribe_room_change_body = serde_json::json!({
            "msg": "sub",
            "id": SUBSCRIBE_ROOMS_MESSAGE_ID,
            "name": "stream-notify-user",
            "params": [
                format!("{}/rooms-changed", user_id),
                false,
            ],
        });
        state.shared_state.outgoing_sender.send(subscribe_room_change_body)
            .expect("failed to enqueue room update subscription message");

        // get the server's settings
        let get_settings_body = serde_json::json!({
            "msg": "method",
            "method": "public-settings/get",
            "id": GET_SETTINGS_MESSAGE_ID,
        });
        state.shared_state.outgoing_sender.send(get_settings_body)
            .expect("failed to enqueue get-settings message");

        // get which rooms we are currently in
        let room_list_body = serde_json::json!({
            "msg": "method",
            "method": "rooms/get",
            "id": GET_ROOMS_MESSAGE_ID,
            "params": [
                {
                    "$date": 0,
                },
            ],
        });
        state.shared_state.outgoing_sender.send(room_list_body)
            .expect("failed to enqueue room list message");

        // get the list of custom emoji
        let mut custom_emoji = obtain_custom_emoji(&state.shared_state).await;

        {
            let mut emoji_guard = state.shared_state.emoji
                .write().await;
            emoji_guard.append(&mut custom_emoji);

            for emoji in emoji_guard.iter() {
                debug!("emoji: {:?}", emoji);
            }
        }

        // get the list of users known to be bots
        debug!("populating bot list");
        update_bot_list(&state.shared_state).await;

        // set our status
        debug!("setting status");
        let no_query_options: [(&str, Option<&str>); 0] = [];
        post_api_json(
            &state.shared_state,
            "api/v1/users.setStatus",
            serde_json::json!({
                "message": "",
                "status": "online",
            }),
            no_query_options,
        )
            .await.expect("failed to set status");
    } else if body["msg"] == "result" && body["id"] == GET_SETTINGS_MESSAGE_ID {
        let settings = &body["result"];
        for entry in settings.members_or_empty() {
            if entry["_id"] == "Message_MaxAllowedSize" {
                if let Some(mas) = entry["value"].as_usize() {
                    let mut settings_guard = state.shared_state.server_settings
                        .write().await;
                    settings_guard.max_message_length = Some(mas);
                }
            } else if entry["_id"] == "UTF8_User_Names_Validation" {
                if let Some(username_regex_str) = entry["value"].as_str() {
                    // trim anchors
                    let mut unanchored_regex_str = username_regex_str;
                    while unanchored_regex_str.starts_with("^") {
                        unanchored_regex_str = &unanchored_regex_str[1..];
                    }
                    while unanchored_regex_str.ends_with("$") && !unanchored_regex_str.ends_with("\\$") {
                        unanchored_regex_str = &unanchored_regex_str[..unanchored_regex_str.len()-1];
                    }
                    match Regex::new(unanchored_regex_str) {
                        Ok(r) => {
                            let mut settings_guard = state.shared_state.server_settings
                                .write().await;
                            settings_guard.username_regex = EnjoyableRegex::from_regex(r);
                        },
                        Err(e) => {
                            error!(
                                "ignoring username regex string {:?} (original {:?}) after error: {}",
                                unanchored_regex_str,
                                username_regex_str,
                                e,
                            );
                        },
                    }
                }
            }
        }
    } else if body["msg"] == "result" && body["id"] == GET_ROOMS_MESSAGE_ID {
        // update our rooms
        for update_room in body["result"]["update"].members_or_empty() {
            let room_id = match update_room["_id"].as_str() {
                Some(v) => v,
                None => {
                    error!("room missing ID; skipping it");
                    continue;
                },
            };

            // update last-message timestamp if it is newer than what we currently have
            if let Some(last_seen_rocket) = update_room["lm"]["$date"].as_i64() {
                let last_seen = rocketchat_timestamp_to_datetime(last_seen_rocket);
                if state.last_seen_message_timestamp < last_seen {
                    state.last_seen_message_timestamp = last_seen;
                }
            }

            // channel = "c", private channel = "p", direct = "d", omnichannel = "l"
            if update_room["t"] == "c" || update_room["t"] == "p" {
                let channel_type: ChannelType = update_room["t"]
                    .as_str().expect("t not representable as string")
                    .try_into().expect("invalid channel type");
                let name = match update_room["name"].as_str() {
                    Some(v) => v,
                    None => {
                        error!("channel {:?} missing ID; skipping it", room_id);
                        continue;
                    },
                };
                let frontend_name = update_room["fname"]
                    .as_str()
                    .map(|s| s.to_owned());

                // remember this room
                let channel = Channel::new(
                    room_id.to_owned(),
                    name.to_owned(),
                    frontend_name,
                    channel_type,
                );

                {
                    let mut room_text_guard = state.shared_state.channel_id_to_texts
                        .write().await;
                    if let Some(announcement) = update_room["announcement"].as_str() {
                        room_text_guard.insert((room_id.to_owned(), ChannelTextType::Announcement), announcement.to_owned());
                    }
                    if let Some(description) = update_room["description"].as_str() {
                        room_text_guard.insert((room_id.to_owned(), ChannelTextType::Description), description.to_owned());
                    }
                    if let Some(topic) = update_room["topic"].as_str() {
                        room_text_guard.insert((room_id.to_owned(), ChannelTextType::Topic), topic.to_owned());
                    }
                }

                channel_joined(&mut state, channel).await;
            } else if update_room["t"] == "d" {
                let participant_usernames = update_room["usernames"].members_or_empty();
                let participant_ids = update_room["uids"].members_or_empty();
                let mut participants = Vec::new();
                for (pun_val, puid_val) in participant_usernames.zip(participant_ids) {
                    let pun = match pun_val.as_str() {
                        Some(p) => p,
                        None => continue,
                    };
                    let puid = match puid_val.as_str() {
                        Some(p) => p,
                        None => continue,
                    };

                    participants.push(User::new(
                        puid.to_owned(),
                        pun.to_owned(),
                        None,
                    ));
                }
                private_conversation_joined(&mut state, room_id, participants).await;
            }
        }
        for remove_room in body["result"]["remove"].members_or_empty() {
            let room_id = match remove_room["_id"].as_str() {
                Some(s) => s,
                None => {
                    error!("error while handling removed rooms: room ID missing or not a string");
                    continue;
                }
            };

            channel_left(&mut state, room_id).await;
        }
    } else if body["msg"] == "changed" && body["collection"] == "stream-room-messages" {
        // we got a message! (probably)

        for message_json in body["fields"]["args"].members_or_empty() {
            let sender_id = match message_json["u"]["_id"].as_str() {
                Some(v) => v,
                None => {
                    error!("message missing sender ID; skipping it");
                    continue;
                },
            };
            let my_user_id = {
                let my_uid_guard = state.shared_state.my_user_id
                    .read().await;
                match my_uid_guard.deref() {
                    Some(muid) => muid.clone(),
                    None => continue,
                }
            };

            let room_id = match message_json["rid"].as_str() {
                Some(v) => v,
                None => {
                    error!("message missing room ID; skipping it");
                    continue;
                },
            };

            let is_by_bot = if let Some(sender_user_id) = message_json["u"]["_id"].as_str() {
                let bots_guard = state.shared_state.bot_user_ids.read().await;
                bots_guard.contains(sender_user_id)
            } else {
                // assume human
                false
            };

            let (channel_opt, convo_opt) = {
                let chandb_read = state.shared_state.subscribed_channels
                    .read().await;
                let channel_opt = chandb_read.get_channel_by_id(&room_id).map(|c| c.clone());
                let convo_opt = chandb_read.get_private_conversation_by_id(&room_id).map(|c| c.clone());
                (channel_opt, convo_opt)
            };
            if let Some(channel) = channel_opt {
                if message_json["t"] == "au" || message_json["t"] == "ru" || message_json["t"] == "uj"  || message_json["t"] == "ul" {
                    // add user/remove user/user joined/user left
                    // update user lists
                    obtain_users_in_room(&mut state, &channel).await;
                    continue;
                } else if message_json["t"] == "room_changed_announcement" || message_json["t"] == "room_changed_description" || message_json["t"] == "room_changed_topic" {
                    let text_type = match message_json["t"].as_str().expect("message type is not a string even though it is") {
                        "room_changed_announcement" => ChannelTextType::Announcement,
                        "room_changed_description" => ChannelTextType::Description,
                        "room_changed_topic" => ChannelTextType::Topic,
                        _ => unreachable!(),
                    };
                    let mut text_guard = state.shared_state.channel_id_to_texts
                        .write().await;
                    if let Some(msg) = message_json["msg"].as_str() {
                        text_guard.insert((channel.id.clone(), text_type), msg.to_owned());
                    } else {
                        text_guard.remove(&(channel.id.clone(), text_type));
                    }
                    continue;
                }

                let message = match message_from_json(message_json, is_by_bot) {
                    Some(m) => m,
                    None => {
                        // error already output
                        continue;
                    },
                };

                if !message_is_current(&mut state, &message) {
                    continue;
                }

                let channel_message = ChannelMessage::new(
                    message,
                    channel,
                );

                // send off for asynchronous processing
                state.process_message_sender.send(MessageToHandle::Channel {
                    sender_id: sender_id.to_owned(),
                    my_user_id: my_user_id.to_owned(),
                    message: channel_message,
                })
                    .expect("failed to enqueue channel message");
            } else if let Some(convo) = convo_opt {
                let message = match message_from_json(message_json, is_by_bot) {
                    Some(m) => m,
                    None => {
                        // error already output
                        continue;
                    },
                };

                if !message_is_current(&mut state, &message) {
                    continue;
                }

                let private_message = PrivateMessage::new(
                    message,
                    convo,
                );

                // send off for asynchronous processing
                state.process_message_sender.send(MessageToHandle::Private {
                    sender_id: sender_id.to_owned(),
                    my_user_id: my_user_id.to_owned(),
                    message: private_message,
                })
                    .expect("failed to enqueue private message");
            }
        }
    } else if body["msg"] == "changed" && body["collection"] == "stream-notify-user" {
        let my_user_id = {
            let uid_guard = state.shared_state.my_user_id.read().await;
            match uid_guard.deref() {
                Some(muid) => muid.clone(),
                None => return,
            }
        };
        let rooms_changed_event_name = format!("{}/rooms-changed", my_user_id);

        if body["fields"]["eventName"] == rooms_changed_event_name && body["fields"]["args"][0] == "inserted" {
            // somebody added us to a channel!
            // subscribe to its messages
            let update_room = &body["fields"]["args"][1];

            let room_id = match update_room["_id"].as_str() {
                Some(v) => v,
                None => {
                    error!("new room is missing room ID; skipping");
                    return;
                },
            };

            if update_room["t"] == "c" || update_room["t"] == "p" {
                let channel_type: ChannelType = update_room["t"]
                    .as_str().expect("t not representable as string")
                    .try_into().expect("invalid channel type");
                let name = match update_room["name"].as_str() {
                    Some(v) => v,
                    None => {
                        error!("updated channel {:?} is missing name; skipping", room_id);
                        return;
                    },
                };
                let frontend_name = update_room["fname"]
                    .as_str()
                    .map(|s| s.to_owned());

                // remember this room
                let channel = Channel::new(
                    room_id.to_owned(),
                    name.to_owned(),
                    frontend_name,
                    channel_type,
                );

                channel_joined(&mut state, channel).await;
            } else if update_room["t"] == "d" {
                let participant_usernames = update_room["usernames"].members_or_empty();
                let participant_ids = update_room["uids"].members_or_empty();
                let mut participants = Vec::new();
                for (pun_val, puid_val) in participant_usernames.zip(participant_ids) {
                    let pun = match pun_val.as_str() {
                        Some(p) => p,
                        None => continue,
                    };
                    let puid = match puid_val.as_str() {
                        Some(p) => p,
                        None => continue,
                    };

                    participants.push(User::new(
                        puid.to_owned(),
                        pun.to_owned(),
                        None,
                    ));
                }
                private_conversation_joined(&mut state, room_id, participants).await;
            }
        }
    } else if body["msg"] == "changed" && body["collection"] == "stream-notify-room" {
        let event_pieces: Vec<&str> = match body["fields"]["eventName"].as_str() {
            Some(en) => en.split("/").collect(),
            None => {
                error!("notify room event {:?} does not have string eventName; skipping", body);
                return;
            },
        };
        if event_pieces.get(1) == Some(&"typing") {
            // typing status changed
            let convo_id = match event_pieces.get(0) {
                Some(id) => *id,
                None => {
                    error!("failed to extract conversation ID from event name; skipping");
                    return;
                },
            };
            let event_args = &body["fields"]["args"];
            let username = match event_args[0].as_str() {
                Some(un) => un,
                None => {
                    error!("first argument to typing event not a string but {:?}; skipping", event_args[0]);
                    return;
                },
            };
            let is_typing = match event_args[1].as_bool() {
                Some(t) => t,
                None => {
                    error!("second argument to typing event not a bool but {:?}; skipping", event_args[1]);
                    return;
                },
            };

            // lookup channel and user
            let mut channel = None;
            let mut private_convo = None;

            let user = {
                let sub_chans = state.shared_state.subscribed_channels
                    .read().await;
                if let Some(chan) = sub_chans.get_channel_by_id(convo_id) {
                    channel = Some(chan.clone());
                } else if let Some(convo) = sub_chans.get_private_conversation_by_id(convo_id) {
                    private_convo = Some(convo.clone());
                }

                sub_chans.users_in_channel(convo_id)
                    .iter()
                    .filter(|u| u.username == username)
                    .map(|u| u.clone())
                    .nth(0)
            };

            if let Some(u) = user {
                if let Some(chan) = channel {
                    // distribute as channel
                    debug!("distributing to plugins that {:?} is typing in channel {:?}", u.username, chan.name);
                    let plugins = state.shared_state.plugins
                        .read().await;
                    for plugin in plugins.iter() {
                        plugin.plugin.user_typing_status_in_channel(&chan, &u, is_typing).await;
                    }
                } else if let Some(convo) = private_convo {
                    // distribute as private convo
                    debug!("distributing to plugins that {:?} is typing in private conversation {:?}", u.username, convo.id);
                    let plugins = state.shared_state.plugins
                        .read().await;
                    for plugin in plugins.iter() {
                        plugin.plugin.user_typing_status_in_private_conversation(&convo, &u, is_typing).await;
                    }
                } else {
                    error!("found {:?} (where {:?} is typing) neither as channel nor as private conversation", convo_id, u.username);
                }
            } else {
                error!("typing user {:?} not found", username);
            }
        }
    } else if body["msg"] == "result" && body["id"].as_str_or_empty().starts_with("create_dm_") {
        // new direct message channel created

        // we probably already received the message adding us to the channel
        // now, process the outgoing messages
        {
            let channel_guard = state.shared_state.subscribed_channels
                .read().await;
            let mut ipm_guard = state.shared_state.username_to_initial_private_message
                .lock().await;
            let mut ipma_guard = state.shared_state.username_to_initial_private_message_with_attachment
                .lock().await;

            let mut successful_usernames: HashSet<String> = HashSet::new();
            for (username, initial_message) in ipm_guard.iter() {
                let convo_opt = channel_guard.get_private_conversation_id_by_counterpart_username(username)
                    .map(|cid| channel_guard.get_private_conversation_by_id(&cid))
                    .flatten();
                if let Some(convo) = convo_opt {
                    do_send_private_message(&state.shared_state, convo, initial_message.clone()).await;
                    successful_usernames.insert(username.to_owned());
                }
            }
            for (username, initial_message_with_attachment) in ipma_guard.iter() {
                let convo_opt = channel_guard.get_private_conversation_id_by_counterpart_username(username)
                    .map(|cid| channel_guard.get_private_conversation_by_id(&cid))
                    .flatten();
                if let Some(convo) = convo_opt {
                    do_send_private_message_with_attachment(&state.shared_state, convo, initial_message_with_attachment.clone()).await;
                    successful_usernames.insert(username.to_owned());
                }
            }

            for succ in &successful_usernames {
                ipm_guard.remove(succ);
                ipma_guard.remove(succ);
            }
        }
    }
}

async fn deliver_timer(info: &serde_json::Value, state: &ConnectionState) {
    let plugins = state.shared_state.plugins
        .read().await;
    debug!("distributing timer {:?} among plugins", info);
    for plugin in plugins.iter() {
        plugin.plugin.timer_elapsed(info).await;
    }
}

fn message_from_json(message_json: &serde_json::Value, sender_is_bot: bool) -> Option<Message> {
    let message_id = match message_json["_id"].as_str() {
        Some(v) => v,
        None => {
            error!("message is missing ID; skipping");
            return None;
        },
    };
    let raw_message: Option<&str> = if message_json["msg"].is_null() {
        None
    } else {
        match message_json["msg"].as_str() {
            Some(v) => Some(v),
            None => {
                error!("message is missing raw content; skipping");
                return None;
            },
        }
    };

    let parsed_message: Option<Vec<MessageFragment>> = if message_json["md"].is_null() {
        None
    } else {
        match parse_message(&message_json["md"]) {
            Ok(pm) => Some(pm),
            Err(e) => {
                error!(
                    "failed to parse message {:?} from structure {:?}: {}",
                    raw_message, message_json["md"].to_string(), e,
                );
                None
            }
        }
    };

    let timestamp_rocket = match message_json["ts"]["$date"].as_i64() {
        Some(ts) => ts,
        None => {
            error!("message is missing timestamp; skipping");
            return None;
        },
    };
    let timestamp = rocketchat_timestamp_to_datetime(timestamp_rocket);

    let u_id = match message_json["u"]["_id"].as_str() {
        Some(v) => v,
        None => {
            error!("message is missing sender user ID; skipping");
            return None;
        },
    };
    let username = match message_json["u"]["username"].as_str() {
        Some(v) => v,
        None => {
            error!("message is missing sender username; skipping");
            return None;
        },
    };
    let nickname = message_json["u"]["name"]
        .as_str()
        .map(|s| s.to_owned());

    let edit_info = if message_json.has_key("editedAt") {
        // message has been edited

        // when?
        let edit_timestamp_rocket = match message_json["editedAt"]["$date"].as_i64() {
            Some(ts) => ts,
            None => {
                error!("edited message is missing timestamp; skipping");
                return None;
            }
        };
        let edit_timestamp = rocketchat_timestamp_to_datetime(edit_timestamp_rocket);

        let editor_id = match message_json["editedBy"]["_id"].as_str() {
            Some(v) => v,
            None => {
                error!("edited message is missing editor user ID; skipping");
                return None;
            },
        };
        let editor_username = match message_json["editedBy"]["username"].as_str() {
            Some(v) => v,
            None => {
                error!("edited message is missing editor username; skipping");
                return None;
            },
        };

        Some(EditInfo::new(
            edit_timestamp,
            User::new(
                editor_id.to_owned(),
                editor_username.to_owned(),
                None,
            ),
        ))
    } else {
        None
    };

    let mut attachments = Vec::new();
    for attachment in message_json["attachments"].members_or_empty() {
        let title = match attachment["title"].as_str() {
            Some(t) => t.to_owned(),
            None => continue,
        };
        let title_link = match attachment["title_link"].as_str() {
            Some(l) => l.to_owned(),
            None => continue,
        };
        let description = attachment["description"].as_str()
            .map(|d| d.to_owned());
        let image_mime_type = attachment["image_type"].as_str()
            .map(|mt| mt.to_owned());
        let image_size_bytes = attachment["image_size"].as_usize();

        attachments.push(MessageAttachment::new(
            title,
            title_link,
            description,
            image_mime_type,
            image_size_bytes,
        ));
    }

    let reply_to_message_id = message_json["tmid"].as_str()
        .map(|t| t.to_owned());

    Some(Message::new(
        message_id.to_owned(),
        timestamp,
        User::new(
            u_id.to_owned(),
            username.to_owned(),
            nickname,
        ),
        raw_message.map(|s| s.to_owned()),
        parsed_message,
        sender_is_bot,
        edit_info,
        attachments,
        reply_to_message_id,
    ))
}

async fn distribute_channel_message_commands(channel_message: &ChannelMessage, shared_state: Arc<SharedConnectionState>) {
    let command_config = &shared_state.command_config;

    let command_prefix = &command_config.command_prefix;
    let message = &channel_message.message;
    let raw_message_with_quote = match &message.raw {
        Some(rmwq) => rmwq,
        None => return, // no commands in non-textual messages
    };

    // do we have a quote to strip?
    let (raw_message, preceding_quote) = if let Some(stripped_quote) = QUOTE_RE.find(raw_message_with_quote) {
        if stripped_quote.start() == 0 {
            let (rm, iq) = raw_message_with_quote.split_at(stripped_quote.len());
            (rm, Some(iq))
        } else {
            (raw_message_with_quote.as_str(), None)
        }
    } else {
        (raw_message_with_quote.as_str(), None)
    };

    if !raw_message.starts_with(command_prefix) {
        return;
    }

    let pieces: Vec<Token> = tokenize(&raw_message).collect();
    let command_text = &pieces[0];
    if !command_text.value.starts_with(command_prefix) {
        return;
    }
    let mut command_name = Cow::Borrowed(&command_text.value[command_prefix.len()..]);
    if shared_state.command_config.case_fold_commands {
        command_name = Cow::Owned(command_name.to_lowercase());
    }

    // do we know this command?
    let command = {
        let commands_guard = shared_state.channel_commands
            .read().await;
        match commands_guard.get(command_name.as_ref()) {
            Some(cd) => cd.clone(),
            None => return,
        }
    };

    if channel_message.message.is_by_bot && !command.behaviors.contains(CommandBehaviors::ACCEPT_FROM_BOTS) {
        // command does not want to be triggered by bots
        return;
    }

    if preceding_quote.is_some() && !command.behaviors.contains(CommandBehaviors::ALLOW_PRECEDING_QUOTE) {
        // command does not allow a quote in front of it
        return;
    }

    let instance_opt = parse_command(&command, &command_config, &pieces, &raw_message, preceding_quote);

    // distribute among plugins
    {
        let plugins = shared_state.plugins
            .read().await;
        debug!("asking plugins to execute channel command {:?}", command.name);
        for plugin in plugins.iter() {
            if let Some(instance) = instance_opt.as_ref() {
                plugin.plugin.channel_command(&channel_message, instance).await;
            } else {
                plugin.plugin.channel_command_wrong(&channel_message, command_name.as_ref()).await;
            }
        }
    }
}

async fn distribute_private_message_commands(private_message: &PrivateMessage, shared_state: Arc<SharedConnectionState>) {
    let command_config = &shared_state.command_config;

    let command_prefix = &command_config.command_prefix;
    let message = &private_message.message;
    let raw_message_with_quote = match &message.raw {
        Some(rmwq) => rmwq,
        None => return, // no commands in non-textual messages
    };

    // do we have a quote to strip?
    let (raw_message, preceding_quote) = if let Some(stripped_quote) = QUOTE_RE.find(raw_message_with_quote) {
        if stripped_quote.start() == 0 {
            let (rm, iq) = raw_message_with_quote.split_at(stripped_quote.len());
            (rm, Some(iq))
        } else {
            (raw_message_with_quote.as_str(), None)
        }
    } else {
        (raw_message_with_quote.as_str(), None)
    };

    if !raw_message.starts_with(command_prefix) {
        return;
    }

    let pieces: Vec<Token> = tokenize(&raw_message).collect();
    let command_text = &pieces[0];
    if !command_text.value.starts_with(command_prefix) {
        return;
    }
    let mut command_name = Cow::Borrowed(&command_text.value[command_prefix.len()..]);
    if shared_state.command_config.case_fold_commands {
        command_name = Cow::Owned(command_name.to_lowercase());
    }

    // do we know this command?
    let command = {
        let commands_guard = shared_state.private_message_commands
            .read().await;
        match commands_guard.get(command_name.as_ref()) {
            Some(cd) => cd.clone(),
            None => return,
        }
    };

    if private_message.message.is_by_bot && !command.behaviors.contains(CommandBehaviors::ACCEPT_FROM_BOTS) {
        // command does not want to be triggered by bots
        return;
    }

    if preceding_quote.is_some() && !command.behaviors.contains(CommandBehaviors::ALLOW_PRECEDING_QUOTE) {
        // command does not allow a quote in front of it
        return;
    }

    let instance_opt = parse_command(&command, &command_config, &pieces, &raw_message, preceding_quote);

    // distribute among plugins
    {
        let plugins = shared_state.plugins
            .read().await;
        debug!("asking plugins to execute private message command {:?}", command.name);
        for plugin in plugins.iter() {
            if let Some(instance) = instance_opt.as_ref() {
                plugin.plugin.private_command(&private_message, &instance).await;
            } else {
                plugin.plugin.private_command_wrong(&private_message, command_name.as_ref()).await;
            }
        }
    }
}

async fn perform_http_json(state: &SharedConnectionState, request: hyper::Request<Full<Bytes>>) -> Result<serde_json::Value, HttpError> {
    let full_uri = request.uri().clone();

    let response_res = state.http_client
        .request(request).await;
    let response = match response_res {
        Ok(r) => r,
        Err(e) => {
            error!("error obtaining response for {}: {}", full_uri, e);
            return Err(HttpError::ObtainingResponse(e));
        },
    };
    let (parts, body) = response.into_parts();
    let response_bytes = match body.collect().await {
        Ok(b) => b.to_bytes().to_vec(),
        Err(e) => {
            error!("error getting bytes from response for {}: {}", full_uri, e);
            return Err(HttpError::ObtainingResponseBody(e));
        },
    };
    let unzipped_bytes = if response_bytes.len() > 1 && response_bytes[0] == 0x1f && response_bytes[1] == 0x8b {
        // gzip; unpack first
        let cursor = Cursor::new(response_bytes);
        let mut dec = GzDecoder::new(cursor);
        let mut gzip_decoded_bytes = Vec::new();
        if let Err(e) = dec.read_to_end(&mut gzip_decoded_bytes) {
            error!("{} is a gzipped file but apparently invalid: {}", full_uri, e);
            return Err(HttpError::DecodingAsGzip(e));
        }
        gzip_decoded_bytes
    } else {
        response_bytes
    };
    let response_string = match String::from_utf8(unzipped_bytes) {
        Ok(s) => s,
        Err(e) => {
            error!("error decoding response for {}: {}", full_uri, e);
            return Err(HttpError::DecodingAsUtf8(e));
        },
    };

    if parts.status != StatusCode::OK {
        error!(
            "error response {} for {}: {}",
            parts.status, full_uri, response_string,
        );
        return Err(HttpError::StatusNotOk(parts.status));
    }

    let json_value: serde_json::Value = match serde_json::from_str(&response_string) {
        Ok(v) => v,
        Err(e) => {
            error!("error parsing JSON for {}: {}", full_uri, e);
            return Err(HttpError::ParsingJson(e));
        },
    };

    Ok(json_value)
}

async fn obtain_request_for_http_from_server<Q, QK, QV, H, HK, HV>(
    state: &SharedConnectionState,
    uri_path: &str,
    method: &str,
    body: Full<Bytes>,
    query_options: Q,
    headers: H,
) -> Result<hyper::Request<Full<Bytes>>, HttpError>
    where
        Q: IntoIterator<Item = (QK, Option<QV>)>,
        QK: AsRef<str>,
        QV: AsRef<str>,
        H: IntoIterator<Item = (HK, HV)>,
        HK: AsRef<str>,
        HV: AsRef<str>,
{
    let user_id = {
        let uid_guard = state.my_user_id
            .read().await;
        match uid_guard.deref() {
            Some(uid) => uid.clone(),
            None => return Err(HttpError::MissingUserId),
        }
    };
    let auth_token = {
        let token_guard = state.my_auth_token
            .read().await;
        match token_guard.deref() {
            Some(tok) => tok.clone(),
            None => return Err(HttpError::MissingAuthToken),
        }
    };
    let web_uri = {
        let config_lock = CONFIG
            .get().expect("no initial configuration set")
            .read().await;
        Url::parse(&config_lock.server.web_uri)
            .expect("failed to parse web URI")
    };

    let mut full_uri = web_uri.join(uri_path)
        .expect("failed to join API endpoint to URI");

    {
        // peek first if we even have any query parameters
        // this avoids appending the question mark if we have none
        let mut query_options_peek = query_options.into_iter().peekable();
        if query_options_peek.peek().is_some() {
            let mut query_params = full_uri.query_pairs_mut();
            for (k, v) in query_options_peek {
                if let Some(sv) = v {
                    query_params.append_pair(k.as_ref(), sv.as_ref());
                } else {
                    query_params.append_key_only(k.as_ref());
                }
            }
        }
    }

    let mut request_builder = hyper::Request::builder()
        .method(method)
        .uri(full_uri.as_str())
        .header("X-User-Id", &user_id)
        .header("X-Auth-Token", auth_token);
    for (key, value) in headers {
        request_builder = request_builder.header(key.as_ref(), value.as_ref());
    }
    let request = request_builder
        .body(body)
        .expect("failed to construct request");

    Ok(request)
}

async fn get_http_from_server<Q, K, V>(state: &SharedConnectionState, uri_path: &str, query_options: Q) -> Result<hyper::Response<Incoming>, HttpError>
    where
        Q: IntoIterator<Item = (K, Option<V>)>,
        K: AsRef<str>,
        V: AsRef<str>,
{
    let no_headers: [(&str, &str); 0] = [];
    let request: hyper::Request<Full<Bytes>> = obtain_request_for_http_from_server(
        &state, uri_path,
        "GET",
        Full::new(Bytes::new()),
        query_options,
        no_headers,
    ).await?;
    let full_uri = request.uri().clone();
    let response_res = state.http_client
        .request(request).await;
    let response = match response_res {
        Ok(r) => r,
        Err(e) => {
            error!("error obtaining response for {}: {}", full_uri, e);
            return Err(HttpError::ObtainingResponse(e));
        },
    };
    Ok(response)
}

async fn get_api_json<Q, K, V>(state: &SharedConnectionState, uri_path: &str, query_options: Q) -> Result<serde_json::Value, HttpError>
    where
        Q: IntoIterator<Item = (K, Option<V>)>,
        K: AsRef<str>,
        V: AsRef<str>,
{
    let no_headers: [(&str, &str); 0] = [];
    let request = obtain_request_for_http_from_server(
        &state,
        uri_path,
        "GET",
        Full::new(Bytes::new()),
        query_options,
        no_headers,
    ).await?;
    perform_http_json(state, request).await
}

async fn post_api_json<Q, K, V>(
    state: &SharedConnectionState,
    uri_path: &str,
    body_json: serde_json::Value,
    query_options: Q,
) -> Result<serde_json::Value, HttpError>
    where
        Q: IntoIterator<Item = (K, Option<V>)>,
        K: AsRef<str>,
        V: AsRef<str>,
{
    let body_string = serde_json::to_string(&body_json)
        .expect("failed to serialize JSON value");
    let body = Full::new(Bytes::from(body_string));
    let headers: [(&str, &str); 1] = [
        ("Content-Type", "application/json"),
    ];
    let request = obtain_request_for_http_from_server(
        &state,
        uri_path,
        "POST",
        body,
        query_options,
        headers,
    ).await?;
    perform_http_json(state, request).await
}

async fn obtain_users_in_room(state: &ConnectionState, channel: &Channel) {
    let mut users: HashSet<User> = HashSet::new();

    // FIXME: delay a bit because Rocket.Chat loves giving out stale information
    tokio::time::sleep(Duration::from_millis(1000)).await;

    debug!("obtaining users in channel {:?} ({:?})", channel.name, channel.id);

    let uri_path = match channel.channel_type {
        ChannelType::Channel => "api/v1/channels.members",
        ChannelType::Group => "api/v1/groups.members",
        _ => return,
    };
    let keys_values: Vec<(&str, Option<&str>)> = vec![
        ("roomId", Some(&channel.id)),
        ("count", Some("50")),
    ];

    let mut offset = 0usize;
    loop {
        let offset_string = offset.to_string();
        let mut offset_keys_values = keys_values.clone();
        offset_keys_values.push(("offset", Some(&offset_string)));

        let json_value = match get_api_json(&state.shared_state, uri_path, offset_keys_values).await {
            Ok(jv) => jv,
            Err(_) => return,
        };
        debug!("channel {:?} members at offset {}: {}", channel.id, offset, json_value);

        let user_count = json_value["members"].members_or_empty().len();
        if user_count == 0 {
            break;
        }

        for user_json in json_value["members"].members_or_empty() {
            let user_id = match user_json["_id"].as_str() {
                Some(s) => s,
                None => {
                    error!("error getting user ID while fetching channel {:?} users", &channel.id);
                    return;
                }
            };
            let username = match user_json["username"].as_str() {
                Some(s) => s,
                None => {
                    error!("error getting username while fetching channel {:?} users", &channel.id);
                    return;
                }
            };
            let nickname = match user_json["name"].as_str() {
                Some(s) => s,
                None => {
                    error!("error getting user nickname while fetching channel {:?} users", &channel.id);
                    return;
                }
            };

            let user = User::new(
                user_id.to_owned(),
                username.to_owned(),
                Some(nickname.to_owned()),
            );
            users.insert(user);
        }

        offset += user_count;
    }

    {
        let mut chan_guard = state.shared_state.subscribed_channels
            .write().await;
        chan_guard.replace_users_in_channel(&channel.id, users);
    }

    {
        let plugins = state.shared_state.plugins
            .read().await;
        debug!("distributing {:?} user list among plugins", channel);
        for plugin in plugins.iter() {
            match channel.channel_type {
                ChannelType::Channel => plugin.plugin.channel_user_list_updated(&channel).await,
                _ => {},
            };
        }
    }
}

async fn obtain_builtin_emoji(state: &ConnectionState) -> Vec<Emoji> {
    let emoji_json_url = {
        let config_guard = CONFIG
            .get().expect("config initially set")
            .read().await;
        config_guard.server.emojione_emoji_json_uri.clone()
    };

    let request = hyper::Request::builder()
        .method("GET")
        .uri(emoji_json_url)
        .body(Full::new(Bytes::new()))
        .expect("failed to construct request");

    let json_value = match perform_http_json(&state.shared_state, request).await {
        Ok(jv) => jv,
        Err(_) => {
            return Vec::new();
        },
    };

    let json_entries = match json_value.as_object() {
        Some(je) => je,
        None => {
            error!("emoji JSON is not an object");
            return Vec::new();
        },
    };
    let mut emoji = Vec::with_capacity(json_entries.len());
    for (codepoint, properties) in json_entries {
        let category = match properties["category"].as_str() {
            Some(v) => v,
            None => {
                error!("emoji {:?} category is not a string", codepoint);
                continue;
            },
        };
        let order = match properties["order"].as_usize() {
            Some(v) => v,
            None => {
                error!("emoji {:?} category is not a string", codepoint);
                continue;
            },
        };
        let short_name = match properties["shortname"].as_str() {
            Some(v) => v,
            None => {
                error!("emoji {:?} shortname is not a string", codepoint);
                continue;
            },
        };
        if short_name.len() < 3 {
            error!("emoji {:?} shortname {:?} is too short (need at least 3 bytes)", codepoint, short_name);
            continue;
        }
        if !short_name.starts_with(":") {
            error!("emoji {:?} shortname {:?} does not start with a colon", codepoint, short_name);
            continue;
        }
        if !short_name.ends_with(":") {
            error!("emoji {:?} shortname {:?} does not end with a colon", codepoint, short_name);
            continue;
        }
        let is_specific = !properties["diversity"].is_null() || !properties["gender"].is_null();

        // strip off the colons
        let short_name_no_colon = &short_name[1..short_name.len()-1];

        let e = Emoji::new(
            category.to_owned(),
            order,
            short_name_no_colon.to_owned(),
            is_specific,
        );
        emoji.push(e);
    }

    emoji
}

async fn obtain_custom_emoji(shared_state: &SharedConnectionState) -> Vec<Emoji> {
    let no_query_options: [(&str, Option<&str>); 0] = [];
    let mut custom_emoji = Vec::new();
    let mut offset: usize = 0;
    loop {
        let custom_emoji_response = get_api_json(
            shared_state,
            &format!("api/v1/emoji-custom.all?offset={}", offset),
            no_query_options,
        )
            .await.expect("failed to obtain custom emoji");
        let mut obtained_count: usize = 0;
        for (i, custom_emoji_json) in custom_emoji_response["emojis"].members_or_empty().enumerate() {
            obtained_count += 1;
            let name = match custom_emoji_json["name"].as_str() {
                Some(n) => n.to_owned(),
                None => {
                    error!("custom emoji with index {}: name is not a string", i);
                    continue;
                },
            };
            let emoji = Emoji::new(
                "custom".to_owned(),
                i,
                name,
                false, // assume custom emoji are generic
            );
            custom_emoji.push(emoji);
        }

        if obtained_count == 0 {
            break;
        }
        offset += obtained_count;
    }
    custom_emoji
}

async fn do_config_reload(state: &mut ConnectionState) {
    info!("reloading configuration");
    // load new plugin configuration
    let new_config = match load_config().await {
        Ok(nc) => nc,
        Err(e) => {
            error!("error loading new config: {}", e);
            return;
        },
    };
    let new_enabled_plugins: Vec<&PluginConfig> = new_config.plugins.iter()
        .filter(|p| p.enabled)
        .collect();

    // verify if it's the same plugins
    let plugins = state.shared_state.plugins
        .read().await;
    if plugins.len() != new_enabled_plugins.len() {
        error!("plugins changed! {} currently loaded, {} newly configured", plugins.len(), new_enabled_plugins.len());
        return;
    }
    for (i, (plugin, new_plugin_config)) in plugins.iter().zip(new_enabled_plugins.iter()).enumerate() {
        if plugin.name != new_plugin_config.name {
            error!("plugins changed! index {}: {:?} loaded, {:?} newly configured", i, plugin.name, new_plugin_config.name);
            return;
        }
    }

    // store config
    set_config(new_config.clone()).await
        .expect("failed to store config");

    for (i, (plugin, plugin_config)) in plugins.iter().zip(new_enabled_plugins.iter()).enumerate() {
        debug!("updating configuration of plugin at index {} ({:?})...", i, plugin_config.name);
        let success = plugin.plugin.configuration_updated(plugin_config.config.clone()).await;
        if !success {
            warn!("updating configuration of plugin at index {} ({:?}) failed", i, plugin_config.name);
        } else {
            debug!("configuration update of plugin at index {} ({:?}) complete", i, plugin_config.name);
        }
    }
    info!("plugin configuration reloaded");

    // update list of bots
    update_bot_list(&state.shared_state).await;
}

async fn update_bot_list(shared_state: &SharedConnectionState) {
    const BOT_ROLE: &str = "bot";
    let users_res = get_api_json(
        shared_state,
        "api/v1/roles.getUsersInRole",
        [("role", Some(BOT_ROLE))],
    ).await;
    let users = match users_res {
        Ok(u) => u,
        Err(e) => {
            error!("HTTP error obtaining users in role {:?}: {}", BOT_ROLE, e);
            return;
        },
    };
    if !users["success"].as_bool().unwrap_or(false) {
        error!("response error obtaining users in role {:?}: {}", BOT_ROLE, users);
        return;
    }

    let mut bot_user_ids = HashSet::new();
    let user_iter = match users["users"].members() {
        Some(ui) => ui,
        None => {
            error!("server roles users \"users\" member is not a list: {}", users);
            return;
        },
    };
    for user_json in user_iter {
        let user_id = match user_json["_id"].as_str() {
            Some(s) => s,
            None => {
                warn!("server role user does not have ID, skipping: {:?}", user_json);
                continue;
            }
        };
        bot_user_ids.insert(user_id.to_owned());
    }

    // store it
    {
        let mut write_guard = shared_state.bot_user_ids.write().await;
        *write_guard = bot_user_ids;
    }
    info!("obtained current bot status")
}
