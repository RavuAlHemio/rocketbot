use crate::message::MessageFragment;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct User {
    pub id: String,
    pub username: String,
    pub nickname: String,
}
impl User {
    pub fn new(
        id: String,
        username: String,
        nickname: String,
    ) -> Self {
        Self {
            id,
            username,
            nickname,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub frontend_name: String,
}
impl Channel {
    pub fn new(
        id: String,
        name: String,
        frontend_name: String,
    ) -> Self {
        Self {
            id,
            name,
            frontend_name,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Message {
    pub sender: User,
    pub raw: String,
    pub parsed: Vec<MessageFragment>,
    pub is_by_bot: bool,
}
impl Message {
    pub fn new(
        sender: User,
        raw: String,
        parsed: Vec<MessageFragment>,
        is_by_bot: bool,
    ) -> Self {
        Self {
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
