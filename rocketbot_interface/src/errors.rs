use std::error::Error;
use std::fmt;


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
