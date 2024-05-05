use std::error::Error;
use std::fmt;
use std::string::FromUtf8Error;

use hyper::StatusCode;
use serde_json;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PluginLoadError {
    MissingConfigurationOption(String),
    InvalidConfigurationOption(String, String),
}
impl fmt::Display for PluginLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginLoadError::MissingConfigurationOption(option)
                => write!(f, "missing configuration option {:?}", option),
            PluginLoadError::InvalidConfigurationOption(option, error)
                => write!(f, "invalid configuration option {:?}: {}", option, error),
        }
    }
}
impl Error for PluginLoadError {
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChannelTypeParseError(pub String);
impl fmt::Display for ChannelTypeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown channel type {:?}", self.0)
    }
}


#[derive(Debug)]
pub enum HttpError {
    MissingUserId,
    MissingAuthToken,
    ObtainingResponse(hyper_util::client::legacy::Error),
    ObtainingResponseBody(hyper::Error),
    DecodingAsGzip(std::io::Error),
    DecodingAsUtf8(FromUtf8Error),
    StatusNotOk(StatusCode),
    ParsingJson(serde_json::Error),
}
impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUserId =>
                write!(f, "user ID is not yet known"),
            Self::MissingAuthToken =>
                write!(f, "authentication token is not yet known"),
            Self::ObtainingResponse(e) =>
                write!(f, "error obtaining response: {}", e),
            Self::ObtainingResponseBody(e) =>
                write!(f, "error obtaining response body: {}", e),
            Self::DecodingAsGzip(e) =>
                write!(f, "error decoding response body as gzip: {}", e),
            Self::DecodingAsUtf8(e) =>
                write!(f, "error decoding response body as UTF-8: {}", e),
            Self::StatusNotOk(sc) =>
                write!(f, "non-OK status ({}) when receiving response", sc),
            Self::ParsingJson(e) =>
                write!(f, "error parsing JSON: {}", e),
        }
    }
}
impl Error for HttpError {
}
