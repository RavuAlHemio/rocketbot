use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::errors::ChannelTypeParseError;
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChannelType {
    Channel,
    Group,
    PrivateConversation,
    Omnichannel,
}
impl TryFrom<&str> for ChannelType {
    type Error = ChannelTypeParseError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "c" => Ok(ChannelType::Channel),
            "p" => Ok(ChannelType::Group),
            "d" => Ok(ChannelType::PrivateConversation),
            "l" => Ok(ChannelType::Omnichannel),
            o => Err(ChannelTypeParseError(o.to_owned())),
        }
    }
}
impl From<ChannelType> for &'static str {
    fn from(ct: ChannelType) -> Self {
        match ct {
            ChannelType::Channel => "c",
            ChannelType::Group => "p",
            ChannelType::PrivateConversation => "d",
            ChannelType::Omnichannel => "l",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub frontend_name: Option<String>,
    pub channel_type: ChannelType,
}
impl Channel {
    pub fn new(
        id: String,
        name: String,
        frontend_name: Option<String>,
        channel_type: ChannelType,
    ) -> Self {
        Self {
            id,
            name,
            frontend_name,
            channel_type,
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
    pub edit_info: Option<EditInfo>,
}
impl Message {
    pub fn new(
        id: String,
        timestamp: DateTime<Utc>,
        sender: User,
        raw: String,
        parsed: Vec<MessageFragment>,
        is_by_bot: bool,
        edit_info: Option<EditInfo>,
    ) -> Self {
        Self {
            id,
            timestamp,
            sender,
            raw,
            parsed,
            is_by_bot,
            edit_info,
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
