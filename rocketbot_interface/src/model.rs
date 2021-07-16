use chrono::{DateTime, Utc};
use crate::message::MessageFragment;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct User {
    pub id: String,
    pub username: String,
    pub nickname: Option<String>,
}
impl User {
    pub fn new(
        id: String,
        username: String,
        nickname: Option<String>,
    ) -> Self {
        Self {
            id,
            username,
            nickname,
        }
    }

    pub fn nickname_or_username(&self) -> &str {
        self.nickname.as_ref().unwrap_or(&self.username)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub frontend_name: Option<String>,
}
impl Channel {
    pub fn new(
        id: String,
        name: String,
        frontend_name: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            frontend_name,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EditInfo {
    pub timestamp: DateTime<Utc>,
    pub editor: User,
}
impl EditInfo {
    pub fn new(
        timestamp: DateTime<Utc>,
        editor: User,
    ) -> Self {
        Self {
            timestamp,
            editor,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Message {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub sender: User,
    pub raw: String,
    pub parsed: Vec<MessageFragment>,
    pub is_by_bot: bool,
}
impl Message {
    pub fn new(
        id: String,
        timestamp: DateTime<Utc>,
        sender: User,
        raw: String,
        parsed: Vec<MessageFragment>,
        is_by_bot: bool,
    ) -> Self {
        Self {
            id,
            timestamp,
            sender,
            raw,
            parsed,
            is_by_bot,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChannelMessage {
    pub message: Message,
    pub channel: Channel,
}
impl ChannelMessage {
    pub fn new(
        message: Message,
        channel: Channel,
    ) -> Self {
        Self {
            message,
            channel,
        }
    }
}
