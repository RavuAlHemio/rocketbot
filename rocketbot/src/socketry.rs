use core::panic;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use json::JsonValue;
use log::{debug, error, log_enabled, warn};
use rand::{Rng, SeedableRng};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
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


static LOGIN_MESSAGE_ID: &'static str = "login4242";
static GET_ROOMS_MESSAGE_ID: &'static str = "rooms4242";
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


struct ConnectionState {
    outgoing_sender: mpsc::UnboundedSender<JsonValue>,
    outgoing_receiver: mpsc::UnboundedReceiver<JsonValue>,
    exit_notify: Arc<Notify>,
    plugins: Vec<Box<dyn RocketBotPlugin>>,
    last_channel_query_unixtime: i64,
    subscribed_channels: Arc<RwLock<ChannelDatabase>>,
}
impl ConnectionState {
    fn new(
        outgoing_sender: mpsc::UnboundedSender<JsonValue>,
        outgoing_receiver: mpsc::UnboundedReceiver<JsonValue>,
        exit_notify: Arc<Notify>,
        plugins: Vec<Box<dyn RocketBotPlugin>>,
        last_channel_query_unixtime: i64,
        subscribed_channels: Arc<RwLock<ChannelDatabase>>,
    ) -> ConnectionState {
        ConnectionState {
            outgoing_sender,
            outgoing_receiver,
            exit_notify,
            plugins,
            last_channel_query_unixtime,
            subscribed_channels,
        }
    }
}

pub(crate) struct ServerConnection {
    outgoing_sender: mpsc::UnboundedSender<JsonValue>,
    exit_notify: Arc<Notify>,
    subscribed_channels: Arc<RwLock<ChannelDatabase>>,
    rng: Mutex<StdRng>,
}
impl ServerConnection {
    fn new(
        outgoing_sender: mpsc::UnboundedSender<JsonValue>,
        exit_notify: Arc<Notify>,
        subscribed_channels: Arc<RwLock<ChannelDatabase>>,
        rng: Mutex<StdRng>,
    ) -> ServerConnection {
        ServerConnection {
            outgoing_sender,
            exit_notify,
            subscribed_channels,
            rng,
        }
    }

    pub fn send(&self, message: JsonValue) {
        self.outgoing_sender.send(message)
            .expect("failed to enqueue message");
    }

    pub fn disconnect(&self) {
        self.exit_notify.notify_one();
    }
}
impl Clone for ServerConnection {
    fn clone(&self) -> Self {
        ServerConnection::new(
            self.outgoing_sender.clone(),
            Arc::clone(&self.exit_notify),
            Arc::clone(&self.subscribed_channels),
            Mutex::new(StdRng::from_entropy()),
        )
    }
}
#[async_trait]
impl RocketBotInterface for ServerConnection {
    async fn send_channel_message(&self, channel: &str, message: &str) {
        // make an ID for this message
        let channel_opt = {
            let cdb_guard = self.subscribed_channels
                .read().await;
            cdb_guard.get_by_name(channel).map(|c| c.clone())
        };
        let channel = if let Some(c) = channel_opt {
            c
        } else {
            warn!("trying to send message to unknown channel {:?}", channel);
            return;
        };

        let message_id = generate_message_id(&self.rng).await;
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

        self.outgoing_sender.send(message_body)
            .expect("failed to enqueue channel message");
    }

    async fn send_private_message(&self, username: &str, message: &str) {
        todo!()
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
    let exit_notify = Arc::new(Notify::new());
    let subscribed_channels = Arc::new(RwLock::new(ChannelDatabase::new_empty()));

    let conn = Arc::new(ServerConnection::new(
        outgoing_sender.clone(),
        Arc::clone(&exit_notify),
        Arc::clone(&subscribed_channels),
        Mutex::new(StdRng::from_entropy()),
    ));
    let mut state = ConnectionState::new(
        outgoing_sender,
        outgoing_receiver,
        exit_notify,
        Vec::new(),
        0,
        subscribed_channels,
    );
    let second_conn: Arc<ServerConnection> = Arc::clone(&conn);
    let generic_conn: Arc<dyn RocketBotInterface> = second_conn;

    // load the plugins
    let mut plugins: Vec<Box<dyn RocketBotPlugin>> = load_plugins(Arc::downgrade(&generic_conn))
        .await;
    state.plugins.append(&mut plugins);

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
    state.outgoing_sender.send(connect_message)
        .expect("failed to enqueue connect message");

    loop {
        tokio::select! {
            _ = state.exit_notify.notified() => {
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

async fn handle_received(body: &JsonValue, state: &mut ConnectionState) {
    if body["msg"] == "ping" {
        // answer with a pong
        let pong_body = json::object! {msg: "pong"};
        state.outgoing_sender.send(pong_body)
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
        state.outgoing_sender.send(login_body)
            .expect("failed to enqueue login message");
    } else if body["msg"] == "result" && body["id"] == LOGIN_MESSAGE_ID {
        // login successful; get our rooms
        let room_list_body = json::object! {
            msg: "method",
            method: "rooms/get",
            id: GET_ROOMS_MESSAGE_ID,
            params: [
                {
                    "$date": state.last_channel_query_unixtime,
                },
            ],
        };
        state.outgoing_sender.send(room_list_body)
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

            {
                let mut cdb_write = state.subscribed_channels
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
            state.outgoing_sender.send(sub_body)
                .expect("failed to enqueue subscription message");
        }
        for remove_room in body["result"]["remove"].members() {
            {
                let mut chandb_write = state.subscribed_channels
                    .write().await;
                chandb_write
                    .forget_by_id(
                        &remove_room["_id"].to_string(),
                    );
            }

            // unsubscribe
            let unsub_body = json::object! {
                msg: "unsub",
                id: format!("sub_{}", remove_room["_id"]),
            };
            state.outgoing_sender.send(unsub_body)
                .expect("failed to enqueue unsubscription message");
        }
        state.last_channel_query_unixtime = Utc::now().timestamp();
        // TODO: update this periodically
    } else if body["msg"] == "changed" && body["collection"] == "stream-room-messages" {
        // we got a message! (probably)

        for message_json in body["fields"]["args"].members() {
            let channel_id = message_json["rid"].to_string();
            let channel_opt = {
                let chandb_read = state.subscribed_channels
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
            for plugin in &state.plugins {
                plugin.channel_message(&message).await;
            }
        }
    }
}
