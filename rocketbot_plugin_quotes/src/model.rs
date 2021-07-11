use std::convert::TryFrom;
use std::fmt;

use chrono::{DateTime, Utc};


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MessageTypeFromCharError {
    UnknownChar,
}
impl fmt::Display for MessageTypeFromCharError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageTypeFromCharError::UnknownChar
                => write!(f, "character does not correspond to a message type"),
        }
    }
}
impl std::error::Error for MessageTypeFromCharError {
}


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MessageType {
    Message,
    Action,
    FreeForm,
}
impl TryFrom<char> for MessageType {
    type Error = MessageTypeFromCharError;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            'M' => Ok(MessageType::Message),
            'A' => Ok(MessageType::Action),
            'F' => Ok(MessageType::FreeForm),
            _ => Err(MessageTypeFromCharError::UnknownChar),
        }
    }
}
impl From<MessageType> for char {
    fn from(mt: MessageType) -> Self {
        match mt {
            MessageType::Message => 'M',
            MessageType::Action => 'A',
            MessageType::FreeForm => 'F',
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Quote {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub channel: String,
    pub author: String,
    pub message_type: MessageType,
    pub body: String,
}
impl Quote {
    pub fn new(
        id: i64,
        timestamp: DateTime<Utc>,
        channel: String,
        author: String,
        message_type: MessageType,
        body: String,
    ) -> Quote {
        Quote {
            id,
            timestamp,
            channel,
            author,
            message_type,
            body,
        }
    }
}
impl fmt::Display for Quote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.message_type {
            MessageType::Message => write!(f, "<{}> {}", self.author, self.body),
            MessageType::Action => write!(f, "* {} {}", self.author, self.body),
            MessageType::FreeForm => write!(f, "{}", self.body),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct QuoteAndVoteSum {
    pub quote: Quote,
    pub vote_sum: i64,
}
impl QuoteAndVoteSum {
    pub fn new(
        quote: Quote,
        vote_sum: i64,
    ) -> QuoteAndVoteSum {
        QuoteAndVoteSum {
            quote,
            vote_sum,
        }
    }

    pub fn format_output(&self, requestor_vote: &str) -> String {
        format!(
            "[{}{}] {}",
            self.vote_sum, requestor_vote, self.quote,
        )
    }
}
