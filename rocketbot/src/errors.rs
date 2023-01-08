use std::error;
use std::fmt;


#[derive(Debug)]
pub(crate) enum WebSocketError {
    Connecting(tokio_tungstenite::tungstenite::Error),
    ReceivingMessage(tokio_tungstenite::tungstenite::Error),
    StreamClosed,
    OutgoingQueueClosed,
    SendingMessage(tokio_tungstenite::tungstenite::Error),
}
impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketError::Connecting(e)
                => write!(f, "error connecting websocket: {}", e),
            WebSocketError::ReceivingMessage(e)
                => write!(f, "error receiving message: {}", e),
            WebSocketError::StreamClosed
                => write!(f, "stream closed"),
            WebSocketError::OutgoingQueueClosed
                => write!(f, "queue of outgoing messages closed"),
            WebSocketError::SendingMessage(e)
                => write!(f, "error sending message: {}", e),
        }
    }
}
impl error::Error for WebSocketError {
}


#[derive(Debug)]
pub(crate) enum ConfigError {
    OpeningFile(std::io::Error),
    Loading(serde_json::Error),
    Setting,
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::OpeningFile(e)
                => write!(f, "error opening config file: {}", e),
            ConfigError::Loading(e)
                => write!(f, "error loading configuration: {}", e),
            ConfigError::Setting
                => write!(f, "error setting configuration"),
        }
    }
}
impl error::Error for ConfigError {
}


#[derive(Debug)]
pub(crate) enum MessageParsingError {
    UnexpectedFragment(String, String),
    TypeNotString,
    BigEmojiValueNotEmoji,
    TaskStatusNotBool,
    CodeLanguageNotString,
    HeadingLevelNotU32,
    PlainTextValueNotString,
    LinkValueNotSinglePlainText,
    LinkValuePlainTextNotString,
    TargetValueNotSinglePlainText(String),
    InnerValueNotList,
}

impl fmt::Display for MessageParsingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageParsingError::UnexpectedFragment(frag_name, expectation)
                => write!(f, "unexpected fragment {:?}; expected {}", frag_name, expectation),
            MessageParsingError::TypeNotString
                => write!(f, "fragment type is either missing or not a string"),
            MessageParsingError::BigEmojiValueNotEmoji
                => write!(f, "big emoji value is not an emoji"),
            MessageParsingError::TaskStatusNotBool
                => write!(f, "task status is either missing or not a bool"),
            MessageParsingError::CodeLanguageNotString
                => write!(f, "code language is either missing or not a string"),
            MessageParsingError::HeadingLevelNotU32
                => write!(f, "heading level is either missing or not a u32"),
            MessageParsingError::PlainTextValueNotString
                => write!(f, "plaintext value is either missing or not a string"),
            MessageParsingError::LinkValueNotSinglePlainText
                => write!(f, "link value is not a single plaintext entry"),
            MessageParsingError::LinkValuePlainTextNotString
                => write!(f, "link value's plaintext entry is either missing or not a string"),
            MessageParsingError::TargetValueNotSinglePlainText(value_type)
                => write!(f, "{} value is not a single plaintext entry", value_type),
            MessageParsingError::InnerValueNotList
                => write!(f, "inner value is not a list"),
        }
    }
}
impl error::Error for MessageParsingError {
}


#[derive(Debug)]
pub(crate) enum GeneralError {
    WebSocket(WebSocketError),
    Config(ConfigError),
    MessageParsing(MessageParsingError),
}
impl fmt::Display for GeneralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeneralError::WebSocket(e)
                => write!(f, "{}", e),
            GeneralError::Config(e)
                => write!(f, "{}", e),
            GeneralError::MessageParsing(e)
                => write!(f, "{}", e),
        }
    }
}
impl error::Error for GeneralError {
}
impl From<WebSocketError> for GeneralError {
    fn from(e: WebSocketError) -> Self {
        GeneralError::WebSocket(e)
    }
}
impl From<ConfigError> for GeneralError {
    fn from(e: ConfigError) -> Self {
        GeneralError::Config(e)
    }
}
impl From<MessageParsingError> for GeneralError {
    fn from(e: MessageParsingError) -> Self {
        GeneralError::MessageParsing(e)
    }
}
