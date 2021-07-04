use core::panic;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::Write;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use json::JsonValue;
use log::{debug, error, log_enabled, warn};
use rand::{Rng, SeedableRng};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance, CommandValue, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{Channel, ChannelMessage, Message, User};
use sha2::{Digest, Sha256};
use tokio;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;

use crate::config::CONFIG;
use crate::errors::WebSocketError;
use crate::jsonage::parse_message;
use crate::plugins::load_plugins;
use crate::string_utils::{SplitChunk, split_whitespace};


static LOGIN_MESSAGE_ID: &'static str = "login4242";
static GET_ROOMS_MESSAGE_ID: &'static str = "rooms4242";
static SUBSCRIBE_ROOMS_MESSAGE_ID: &'static str = "roomchanges4242";
static SEND_MESSAGE_MESSAGE_ID: &'static str = "sendmessage4242";
static ID_ALPHABET: &'static str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const ID_LENGTH: usize = 17;


struct ChannelDatabase {
    by_id: HashMap<String, Channel>,
    by_name: HashMap<String, Channel>,
}
impl ChannelDatabase {
    fn new_empty() -> Self {
        Self {
            by_id: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    fn register_channel(&mut self, channel: Channel) {
        // make sure we either don't know the channel at all or we know it fully
        // (ensure there is no pair of channels with different IDs but the same name)
        let know_id = self.by_id.contains_key(&channel.id);
        let know_name = self.by_name.contains_key(&channel.id);
        if know_id != know_name {
            panic!(
                "attempting to register duplicate channel with ID {:?} (already known? {}) and name {:?} (already known? {})",
                channel.id, know_id, channel.name, know_name,
            );
        }

        self.by_id.insert(channel.id.clone(), channel.clone());
        self.by_name.insert(channel.name.clone(), channel);
    }

    fn get_by_id(&self, id: &str) -> Option<&Channel> {
        self.by_id.get(id)
    }

    fn get_by_name(&self, name: &str) -> Option<&Channel> {
        self.by_name.get(name)
    }

    /// Returns `true` if the channel was known (and removed) and `false` if it was not known.
    fn forget_by_id(&mut self, id: &str) -> bool {
        if let Some(channel) = self.by_id.remove(id) {
            self.by_name.remove(&channel.name);
            true
        } else {
            false
        }
    }

    fn by_id(&self) -> &HashMap<String, Channel> {
        &self.by_id
    }

    fn by_name(&self) -> &HashMap<String, Channel> {
        &self.by_name
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CommandConfiguration {
    command_prefix: String,
    short_option_prefix: String,
    long_option_prefix: String,
    stop_parse_option: String,
}
impl CommandConfiguration {
    fn new(
        command_prefix: String,
        short_option_prefix: String,
        long_option_prefix: String,
        stop_parse_option: String,
    ) -> CommandConfiguration {
        CommandConfiguration {
            command_prefix,
            short_option_prefix,
            long_option_prefix,
            stop_parse_option,
        }
    }
}
impl Default for CommandConfiguration {
    fn default() -> Self {
        CommandConfiguration::new(
            String::from("!"),
            String::from("-"),
            String::from("--"),
            String::from("--"),
        )
    }
}


struct SharedConnectionState {
    outgoing_sender: mpsc::UnboundedSender<JsonValue>,
    exit_notify: Notify,
    plugins: RwLock<Vec<Box<dyn RocketBotPlugin>>>,
    subscribed_channels: RwLock<ChannelDatabase>,
    rng: Mutex<StdRng>,
    command_config: CommandConfiguration,
    commands: RwLock<HashMap<String, CommandDefinition>>,
}
impl SharedConnectionState {
    fn new(
        outgoing_sender: mpsc::UnboundedSender<JsonValue>,
        exit_notify: Notify,
        plugins: RwLock<Vec<Box<dyn RocketBotPlugin>>>,
        subscribed_channels: RwLock<ChannelDatabase>,
        rng: Mutex<StdRng>,
        command_config: CommandConfiguration,
        commands: RwLock<HashMap<String, CommandDefinition>>,
    ) -> Self {
        Self {
            outgoing_sender,
            exit_notify,
            plugins,
            subscribed_channels,
            rng,
            command_config,
            commands,
        }
    }
}


struct ConnectionState {
    shared_state: Arc<SharedConnectionState>,
    outgoing_receiver: mpsc::UnboundedReceiver<JsonValue>,
    my_user_id: Option<String>,
}
impl ConnectionState {
    fn new(
        shared_state: Arc<SharedConnectionState>,
        outgoing_receiver: mpsc::UnboundedReceiver<JsonValue>,
        my_user_id: Option<String>,
    ) -> ConnectionState {
        ConnectionState {
            shared_state,
            outgoing_receiver,
            my_user_id,
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

    pub fn send(&self, message: JsonValue) {
        self.shared_state.outgoing_sender.send(message)
            .expect("failed to enqueue message");
    }

    pub fn disconnect(&self) {
        self.shared_state.exit_notify.notify_one();
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
    async fn send_channel_message(&self, channel: &str, message: &str) {
        // make an ID for this message
        let channel_opt = {
            let cdb_guard = self.shared_state.subscribed_channels
                .read().await;
            cdb_guard.get_by_name(channel).map(|c| c.clone())
        };
        let channel = if let Some(c) = channel_opt {
            c
        } else {
            warn!("trying to send message to unknown channel {:?}", channel);
            return;
        };

        let message_id = generate_message_id(&self.shared_state.rng).await;
        let message_body = json::object! {
            msg: "method",
            method: "sendMessage",
            id: SEND_MESSAGE_MESSAGE_ID,
            params: [
                {
                    _id: message_id.clone(),
                    rid: channel.id.clone(),
                    msg: message,
                },
            ],
        };

        self.shared_state.outgoing_sender.send(message_body)
            .expect("failed to enqueue channel message");
    }

    async fn send_private_message(&self, username: &str, message: &str) {
        todo!()
    }

    async fn resolve_username(&self, username: &str) -> Option<String> {
        // ask all plugins, stop at the first non-None result
        {
            let plugins = self.shared_state.plugins
                .read().await;
            for plugin in plugins.iter() {
                if let Some(un) = plugin.username_resolution(username).await {
                    return Some(un);
                }
            }
        }
        None
    }

    async fn register_channel_command(&self, command: &CommandDefinition) -> bool {
        let mut commands_guard = self.shared_state.commands
            .write().await;
        match commands_guard.entry(command.name.clone()) {
            HashMapEntry::Occupied(_) => {
                false
            },
            HashMapEntry::Vacant(ve) => {
                ve.insert(command.clone());
                true
            },
        }
    }

    async fn register_private_message_command(&self, command: &CommandDefinition) -> bool {
        todo!();
    }
}


async fn generate_message_id<R: Rng>(rng_lock: &Mutex<R>) -> String {
    let alphabet_chars: Vec<char> = ID_ALPHABET.chars().collect();
    let distribution = Uniform::new(0, alphabet_chars.len());
    let mut message_id = String::with_capacity(ID_LENGTH);

    {
        let mut rng_guard = rng_lock.lock().await;
        for _ in 0..ID_LENGTH {
            message_id.push(alphabet_chars[distribution.sample(rng_guard.deref_mut())]);
        }
    }

    message_id
}


pub(crate) async fn connect() -> Arc<ServerConnection> {
    let (outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
    let exit_notify = Notify::new();
    let subscribed_channels = RwLock::new(ChannelDatabase::new_empty());
    let plugins = RwLock::new(Vec::new());
    let rng = Mutex::new(StdRng::from_entropy());
    let command_config = Default::default();
    let commands = RwLock::new(HashMap::new());

    let shared_state = Arc::new(SharedConnectionState::new(
        outgoing_sender,
        exit_notify,
        plugins,
        subscribed_channels,
        rng,
        command_config,
        commands,
    ));

    let conn = Arc::new(ServerConnection::new(
        Arc::clone(&shared_state),
    ));
    let state = ConnectionState::new(
        shared_state,
        outgoing_receiver,
        None,
    );
    let second_conn: Arc<ServerConnection> = Arc::clone(&conn);
    let generic_conn: Arc<dyn RocketBotInterface> = second_conn;

    // load the plugins
    let mut loaded_plugins: Vec<Box<dyn RocketBotPlugin>> = load_plugins(Arc::downgrade(&generic_conn))
        .await;
    {
        let mut plugins_guard = state.shared_state.plugins
            .write().await;
        plugins_guard.append(&mut loaded_plugins);
    }

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
                log::error!("{}", e);
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
    let websocket_uri = {
        let config_lock = CONFIG
            .get().expect("no initial configuration set")
            .read().await;
        config_lock.server.websocket_uri.clone()
    };

    let (mut stream, _response) = connect_async(&websocket_uri).await
        .map_err(|e| WebSocketError::Connecting(e))?;

    // connect!
    let connect_message = json::object! {
        msg: "connect",
        version: "1",
        support: ["1"]
    };
    state.shared_state.outgoing_sender.send(connect_message)
        .expect("failed to enqueue connect message");

    loop {
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

                    let body: JsonValue = match json::parse(&body_string) {
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

                if log_enabled!(log::Level::Debug) {
                    debug!("sending message: {:?}", content.dump());
                }

                let msg = WebSocketMessage::Text(content.dump());
                if let Err(e) = stream.send(msg).await {
                    return Err(WebSocketError::SendingMessage(e));
                }
            }
        };
    }

    Ok(())
}

async fn channel_joined(state: &mut ConnectionState, channel: Channel) {
    {
        let mut cdb_write = state.shared_state.subscribed_channels
            .write().await;
        cdb_write
            .register_channel(channel.clone());
    }

    // subscribe to messages in this room
    let sub_body = json::object! {
        msg: "sub",
        id: format!("sub_{}", channel.id),
        name: "stream-room-messages",
        params: [
            channel.id.clone(),
            false,
        ],
    };
    state.shared_state.outgoing_sender.send(sub_body)
        .expect("failed to enqueue subscription message");
}

async fn channel_left(state: &mut ConnectionState, channel_id: &str) {
    {
        let mut cdb_write = state.shared_state.subscribed_channels
            .write().await;
        cdb_write
            .forget_by_id(channel_id);
    }

    // unsubscribe
    let unsub_body = json::object! {
        msg: "unsub",
        id: format!("sub_{}", channel_id),
    };
    state.shared_state.outgoing_sender.send(unsub_body)
        .expect("failed to enqueue unsubscription message");
}

async fn handle_received(body: &JsonValue, mut state: &mut ConnectionState) {
    if body["msg"] == "ping" {
        // answer with a pong
        let pong_body = json::object! {msg: "pong"};
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

        let login_body = json::object! {
            msg: "method",
            method: "login",
            id: LOGIN_MESSAGE_ID,
            params: [
                {
                    user: {
                        username: username.clone(),
                    },
                    password: {
                        digest: password_sha256.clone(),
                        algorithm: "sha-256",
                    },
                },
            ],
        };
        state.shared_state.outgoing_sender.send(login_body)
            .expect("failed to enqueue login message");
    } else if body["msg"] == "result" && body["id"] == LOGIN_MESSAGE_ID {
        // login successful

        // store our ID
        let user_id = body["result"]["id"].as_str()
            .expect("user ID missing or not a string")
            .to_owned();
        state.my_user_id = Some(user_id.clone());

        // subscribe to changes to our room state
        let subscribe_room_change_body = json::object! {
            msg: "sub",
            id: SUBSCRIBE_ROOMS_MESSAGE_ID,
            name: "stream-notify-user",
            params: [
                format!("{}/rooms-changed", user_id),
                false,
            ],
        };
        state.shared_state.outgoing_sender.send(subscribe_room_change_body)
            .expect("failed to enqueue room update subscription message");

        // get which rooms we are currently in
        let room_list_body = json::object! {
            msg: "method",
            method: "rooms/get",
            id: GET_ROOMS_MESSAGE_ID,
            params: [
                {
                    "$date": 0,
                },
            ],
        };
        state.shared_state.outgoing_sender.send(room_list_body)
            .expect("failed to enqueue room list message");
    } else if body["msg"] == "result" && body["id"] == GET_ROOMS_MESSAGE_ID {
        // update our rooms
        for update_room in body["result"]["update"].members() {
            if update_room["t"] != "c" {
                // not a channel; skip
                // TODO: private messages?
                continue;
            }

            // remember this room
            let channel = Channel::new(
                update_room["_id"].to_string(),
                update_room["name"].to_string(),
                update_room["fname"].to_string(),
            );

            channel_joined(&mut state, channel).await;
        }
        for remove_room in body["result"]["remove"].members() {
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

        for message_json in body["fields"]["args"].members() {
            let channel_id = message_json["rid"].to_string();
            let channel_opt = {
                let chandb_read = state.shared_state.subscribed_channels
                    .read().await;
                chandb_read.get_by_id(&channel_id).map(|c| c.clone())
            };
            let channel = match channel_opt {
                None => {
                    // TODO: proactively look up channel?
                    warn!("message from unknown channel {:?}", channel_id);
                    return;
                },
                Some(c) => c,
            };

            let raw_message = message_json["msg"].to_string();
            let parsed_message = match parse_message(&message_json["md"]) {
                Ok(pm) => pm,
                Err(e) => {
                    error!("failed to parse message {:?} from structure {:?}: {}", raw_message, message_json["md"], e);
                    return;
                },
            };

            let message = ChannelMessage::new(
                Message::new(
                    User::new(
                        message_json["u"]["_id"].to_string(),
                        message_json["u"]["username"].to_string(),
                        message_json["u"]["name"].to_string(),
                    ),
                    raw_message,
                    parsed_message,
                ),
                channel,
            );

            // distribute among plugins
            {
                let plugins = state.shared_state.plugins
                    .read().await;
                for plugin in plugins.iter() {
                    plugin.channel_message(&message).await;
                }
            }

            // parse commands if there are any
            distribute_channel_message_commands(&message, &mut state).await;
        }
    } else if body["msg"] == "changed" && body["collection"] == "stream-notify-user" {
        let my_user_id = match &state.my_user_id {
            Some(muid) => muid.as_str(),
            None => return,
        };
        let rooms_changed_event_name = format!("{}/rooms-changed", my_user_id);

        if body["fields"]["eventName"] == rooms_changed_event_name && body["fields"]["args"][0] == "inserted" {
            // somebody added us to a channel!
            // subscribe to its messages
            let update_room = &body["fields"]["args"][1];
            if update_room["t"] != "c" {
                // not a channel; skip
                // TODO: private messages?
                return;
            }

            // remember this room
            let channel = Channel::new(
                update_room["_id"].to_string(),
                update_room["name"].to_string(),
                update_room["name"].to_string(), // fname is missing for some reason
            );

            channel_joined(&mut state, channel).await;
        }
    }
}

async fn distribute_channel_message_commands(channel_message: &ChannelMessage, state: &mut ConnectionState) {
    let command_config = &state.shared_state.command_config;

    let command_prefix = &command_config.command_prefix;
    let message = &channel_message.message;
    if !message.raw.starts_with(command_prefix) {
        return;
    }

    let pieces: Vec<SplitChunk> = split_whitespace(&message.raw).collect();
    let command_text = pieces[0];
    if !command_text.chunk.starts_with(command_prefix) {
        return;
    }
    let command_name = &command_text.chunk[command_prefix.len()..];

    // do we know this command?
    let command = {
        let commands_guard = state.shared_state.commands
            .read().await;
        match commands_guard.get(command_name) {
            Some(cd) => cd.clone(),
            None => return,
        }
    };

    let mut i = 0;
    let mut set_flags = HashSet::new();
    let mut option_values = HashMap::new();
    while i < pieces.len() {
        if pieces[i].chunk == command_config.stop_parse_option {
            // no more options beyond this point, just positional args
            // move forward and stop parsing
            i += 1;
            break;
        }

        // this code assumes that the long option prefix is not a prefix of the short option prefix
        // (e.g. short option prefix "--", long option prefix "-")
        assert!(!command_config.short_option_prefix.starts_with(&command_config.long_option_prefix));

        if pieces[i].chunk.starts_with(&command_config.long_option_prefix) {
            // it's a long option/flag!
            let option_name = &pieces[i].chunk[command_config.long_option_prefix.len()..];

            let handling_result = handle_option(
                &command,
                &option_name,
                None,
                &mut i,
                &mut set_flags,
                &mut option_values,
                &pieces,
                &command_name,
                &message.raw,
            );
            if let OptionHandlingResult::Failure = handling_result {
                // error messages already logged
                return;
            }
        } else if pieces[i].chunk.starts_with(&command_config.short_option_prefix) {
            // it's a bunch of short options!
            let mut value_consumed_by_option: Option<String> = None;

            for c in pieces[i].chunk[command_config.short_option_prefix.len()..].chars() {
                let option_name: String = String::from(c);

                let handling_result = handle_option(
                    &command,
                    &option_name,
                    value_consumed_by_option.as_deref(),
                    &mut i,
                    &mut set_flags,
                    &mut option_values,
                    &pieces,
                    &command_name,
                    &message.raw,
                );
                match handling_result {
                    OptionHandlingResult::Failure => {
                        // error messages already logged
                        return;
                    },
                    OptionHandlingResult::Flag => {
                        // no worries here
                    },
                    OptionHandlingResult::Option => {
                        // make sure we remember that this option gobbled up an argument
                        value_consumed_by_option = Some(option_name);
                    },
                }
            }
        }

        i += 1;
    }

    // gobble up the requisite positional arguments
    let mut pos_args = Vec::with_capacity(command.arg_count);
    for j in 0..command.arg_count {
        if i >= pieces.len() {
            warn!("missing positional argument with index {} passed to {}", j, command_name);
            debug!("command line is {:?}", message.raw);
            return;
        }
        pos_args.push(pieces[i].chunk.to_owned());
        i += 1;
    }

    // take the rest
    let rest_string = if i < pieces.len() {
        message.raw[pieces[i].orig_index..].to_owned()
    } else {
        String::new()
    };

    // it finally comes together
    let instance = CommandInstance::new(
        command.name.clone(),
        set_flags,
        option_values,
        pos_args,
        rest_string,
    );

    // distribute among plugins
    {
        let plugins = state.shared_state.plugins
            .read().await;
        for plugin in plugins.iter() {
            plugin.channel_command(&channel_message, &instance).await;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum OptionHandlingResult {
    Failure,
    Flag,
    Option,
}

fn handle_option(
    command: &CommandDefinition,
    option_name: &str,
    value_consumed_by_option: Option<&str>,
    i: &mut usize,
    set_flags: &mut HashSet<String>,
    option_values: &mut HashMap<String, CommandValue>,
    pieces: &[SplitChunk],
    command_name: &str,
    raw_message: &str,
) -> OptionHandlingResult {
    if command.flags.contains(option_name) {
        // flag found!
        set_flags.insert(option_name.to_owned());
        OptionHandlingResult::Flag
    } else {
        // is it an option?
        let option_type = match command.options.get(option_name) {
            Some(ot) => ot,
            None => {
                warn!("unknown option {:?} passed to {}", option_name, command_name);
                debug!("command line is {:?}", raw_message);
                return OptionHandlingResult::Failure;
            }
        };

        if let Some(vcbo) = value_consumed_by_option {
            // e.g. "-abcd value" where both -a and -b take a value
            warn!("option {:?} passed to {} wants a value which was already consumed by option {:?}", option_name, command_name, vcbo);
            debug!("command line is {:?}", raw_message);
            return OptionHandlingResult::Failure;
        }

        // is there a next piece?
        if *i + 1 >= pieces.len() {
            warn!("option {:?} of {} is missing an argument", option_name, command_name);
            debug!("command line is {:?}", raw_message);
            return OptionHandlingResult::Failure;
        }
        let option_value_str = pieces[*i + 1].chunk;

        let option_value = match option_type {
            CommandValueType::String => CommandValue::String(option_value_str.to_owned()),
            CommandValueType::Float => {
                let float_val: f64 = match option_value_str.parse() {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("failed to parse argument {:?} for option {:?} of {} as a floating-point value: {}", option_value_str, option_name, command_name, e);
                        debug!("command line is {:?}", raw_message);
                        return OptionHandlingResult::Failure;
                    },
                };
                CommandValue::Float(float_val)
            },
            CommandValueType::Integer => {
                let int_val: i64 = match option_value_str.parse() {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("failed to parse argument {:?} for option {:?} of {} as an integer: {}", option_value_str, option_name, command_name, e);
                        debug!("command line is {:?}", raw_message);
                        return OptionHandlingResult::Failure;
                    },
                };
                CommandValue::Integer(int_val)
            },
        };

        option_values.insert(option_name.to_owned(), option_value);

        // skip over one more argument (the value to this option)
        *i += 1;

        OptionHandlingResult::Option
    }
}
