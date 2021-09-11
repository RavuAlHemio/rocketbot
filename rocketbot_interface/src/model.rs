use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::is_sorted_no_dupes;
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
    Omnichannel,
}
impl TryFrom<&str> for ChannelType {
    type Error = ChannelTypeParseError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "c" => Ok(ChannelType::Channel),
            "p" => Ok(ChannelType::Group),
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
pub struct PrivateConversation {
    pub id: String,
    pub other_participants: Vec<User>,
}
impl PrivateConversation {
    pub fn new(
        id: String,
        other_participants: Vec<User>,
    ) -> Self {
        if !is_sorted_no_dupes(other_participants.iter().map(|u| &u.id)) {
            panic!("other_participants must be sorted by id");
        }

        Self {
            id,
            other_participants,
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
    pub raw: Option<String>,
    pub parsed: Option<Vec<MessageFragment>>,
    pub is_by_bot: bool,
    pub edit_info: Option<EditInfo>,
    pub attachments: Vec<MessageAttachment>,
    pub reply_to_message_id: Option<String>,
}
impl Message {
    pub fn new(
        id: String,
        timestamp: DateTime<Utc>,
        sender: User,
        raw: Option<String>,
        parsed: Option<Vec<MessageFragment>>,
        is_by_bot: bool,
        edit_info: Option<EditInfo>,
        attachments: Vec<MessageAttachment>,
        reply_to_message_id: Option<String>,
    ) -> Self {
        Self {
            id,
            timestamp,
            sender,
            raw,
            parsed,
            is_by_bot,
            edit_info,
            attachments,
            reply_to_message_id,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MessageAttachment {
    pub title: String,
    pub title_link: String,
    pub description: Option<String>,
    pub image_mime_type: Option<String>,
    pub image_size_bytes: Option<usize>,
}
impl MessageAttachment {
    pub fn new(
        title: String,
        title_link: String,
        description: Option<String>,
        image_mime_type: Option<String>,
        image_size_bytes: Option<usize>,
    ) -> Self {
        Self {
            title,
            title_link,
            description,
            image_mime_type,
            image_size_bytes,
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PrivateMessage {
    pub message: Message,
    pub conversation: PrivateConversation,
}
impl PrivateMessage {
    pub fn new(
        message: Message,
        conversation: PrivateConversation,
    ) -> Self {
        Self {
            message,
            conversation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChannelTextType {
    Description,
    Announcement,
    Topic,
}
impl AsRef<str> for ChannelTextType {
    fn as_ref(&self) -> &str {
        match self {
            ChannelTextType::Description => "description",
            ChannelTextType::Announcement => "announcement",
            ChannelTextType::Topic => "topic",
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImpersonationInfo {
    pub avatar_url: String,
    pub nickname: String,
}
impl ImpersonationInfo {
    pub fn new(
        avatar_url: String,
        nickname: String,
    ) -> Self {
        Self {
            avatar_url,
            nickname,
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutgoingMessage {
    pub body: String,
    pub impersonation: Option<ImpersonationInfo>,
    pub reply_to_message_id: Option<String>,
}
impl OutgoingMessage {
    pub fn new(
        body: String,
        impersonation: Option<ImpersonationInfo>,
        reply_to_message_id: Option<String>,
    ) -> Self {
        Self {
            body,
            impersonation,
            reply_to_message_id,
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutgoingMessageBuilder {
    message: OutgoingMessage,
}
impl OutgoingMessageBuilder {
    pub fn new(
        body: String,
    ) -> Self {
        let message = OutgoingMessage::new(
            body,
            None,
            None,
        );
        Self {
            message,
        }
    }

    pub fn body(mut self, new_body: String) -> Self {
        self.message.body = new_body;
        self
    }

    pub fn impersonation(mut self, new_impersonation: ImpersonationInfo) -> Self {
        self.message.impersonation = Some(new_impersonation);
        self
    }

    pub fn reply_to_message_id(mut self, new_reply_to_message_id: String) -> Self {
        self.message.reply_to_message_id = Some(new_reply_to_message_id);
        self
    }

    pub fn build(&self) -> OutgoingMessage {
        self.message.clone()
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Emoji {
    pub category: String,
    pub order: usize,
    pub short_name: String,
}
impl Emoji {
    pub fn new(
        category: String,
        order: usize,
        short_name: String,
    ) -> Self {
        Self {
            category,
            order,
            short_name,
        }
    }

    pub fn is_custom(&self) -> bool {
        self.category == "custom"
    }
}
