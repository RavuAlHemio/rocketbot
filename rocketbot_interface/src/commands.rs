use std::collections::{HashMap, HashSet};

use bitflags::bitflags;
use serde::{Deserialize, Serialize};


bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct CommandBehaviors: u64 {
        const ACCEPT_FROM_BOTS = 0b00000001;
        const NO_ARGUMENT_PARSING = 0b00000010;
        const REST_AS_ARGS = 0b00000100;
    }
}


#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum CommandValueType {
    String,
    Integer,
    Float,
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDefinition {
    pub name: String,
    pub plugin_name: Option<String>,
    pub flags: Option<HashSet<String>>,
    pub options: HashMap<String, CommandValueType>,
    pub arg_count: usize,
    pub behaviors: CommandBehaviors,

    /// The following placeholders may be used in the usage string:
    /// `{cpfx}`: command prefix
    /// `{sopfx}`: short option prefix
    /// `{lopfx}`: long option prefix
    pub usage: String,
    pub description: String,
}
impl CommandDefinition {
    /// Pass `None` to `flags` to receive any and all flags specified by the user. Pass a `Some`
    /// value with an empty `HashSet<String>` to declare that the command does not take any flags.
    pub fn new(
        name: String,
        plugin_name: String,
        flags: Option<HashSet<String>>,
        options: HashMap<String, CommandValueType>,
        arg_count: usize,
        behaviors: CommandBehaviors,
        usage: String,
        description: String,
    ) -> CommandDefinition {
        CommandDefinition {
            name,
            plugin_name: Some(plugin_name),
            flags,
            options,
            arg_count,
            behaviors,
            usage,
            description,
        }
    }

    pub fn copy_named(&self, new_name: &str) -> CommandDefinition {
        let mut ret = self.clone();
        ret.name = new_name.to_owned();
        ret
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDefinitionBuilder {
    definition: CommandDefinition,
}
impl CommandDefinitionBuilder {
    pub fn new<N, P, U, D>(
        name: N,
        plugin_name: P,
        usage: U,
        description: D,
    ) -> Self
            where
                N: Into<String>,
                P: Into<String>,
                U: Into<String>,
                D: Into<String> {
        let definition = CommandDefinition::new(
            name.into(),
            plugin_name.into(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            usage.into(),
            description.into(),
        );
        Self {
            definition,
        }
    }

    pub fn flags(mut self, new_flags: Option<HashSet<String>>) -> Self {
        self.definition.flags = new_flags;
        self
    }

    pub fn any_flags(mut self) -> Self {
        self.definition.flags = None;
        self
    }

    pub fn add_flag<N: Into<String>>(mut self, new_flag: N) -> Self {
        if let Some(flags) = &mut self.definition.flags {
            flags.insert(new_flag.into());
        }
        self
    }

    pub fn options(mut self, new_options: HashMap<String, CommandValueType>) -> Self {
        self.definition.options = new_options;
        self
    }

    pub fn add_option<N: Into<String>>(mut self, new_option: N, new_type: CommandValueType) -> Self {
        self.definition.options.insert(new_option.into(), new_type);
        self
    }

    pub fn arg_count(mut self, new_arg_count: usize) -> Self {
        self.definition.arg_count = new_arg_count;
        self
    }

    pub fn behaviors(mut self, new_behaviors: CommandBehaviors) -> Self {
        self.definition.behaviors = new_behaviors;
        self
    }

    pub fn build(&self) -> CommandDefinition {
        self.definition.clone()
    }
}


#[derive(Clone, Debug, PartialEq)]
pub enum CommandValue {
    String(String),
    Integer(i64),
    Float(f64),
}
impl CommandValue {
    pub fn as_str(&self) -> Option<&str> {
        if let CommandValue::String(s) = self {
            Some(&s)
        } else {
            None
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        if let CommandValue::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        if let CommandValue::Float(f) = self {
            Some(*f)
        } else {
            None
        }
    }
}


#[derive(Clone, Debug, PartialEq)]
pub struct CommandInstance {
    pub name: String,
    pub flags: HashSet<String>,
    pub options: HashMap<String, CommandValue>,
    pub args: Vec<String>,
    pub rest: String,
}
impl CommandInstance {
    pub fn new(
        name: String,
        flags: HashSet<String>,
        options: HashMap<String, CommandValue>,
        args: Vec<String>,
        rest: String,
    ) -> CommandInstance {
        CommandInstance {
            name,
            flags,
            options,
            args,
            rest,
        }
    }
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CommandConfiguration {
    #[serde(default = "CommandConfiguration::default_command_prefix")]
    pub command_prefix: String,

    #[serde(default = "CommandConfiguration::default_short_option_prefix")]
    pub short_option_prefix: String,

    #[serde(default = "CommandConfiguration::default_long_option_prefix")]
    pub long_option_prefix: String,

    #[serde(default = "CommandConfiguration::default_stop_parse_option")]
    pub stop_parse_option: String,

    #[serde(default = "CommandConfiguration::default_case_fold_commands")]
    pub case_fold_commands: bool,
}
impl CommandConfiguration {
    pub fn new(
        command_prefix: String,
        short_option_prefix: String,
        long_option_prefix: String,
        stop_parse_option: String,
        case_fold_commands: bool,
    ) -> CommandConfiguration {
        CommandConfiguration {
            command_prefix,
            short_option_prefix,
            long_option_prefix,
            stop_parse_option,
            case_fold_commands,
        }
    }

    fn default_command_prefix() -> String { "!".to_owned() }
    fn default_short_option_prefix() -> String { "-".to_owned() }
    fn default_long_option_prefix() -> String { "--".to_owned() }
    fn default_stop_parse_option() -> String { "--".to_owned() }
    fn default_case_fold_commands() -> bool { false }
}
impl Default for CommandConfiguration {
    fn default() -> Self {
        CommandConfiguration::new(
            Self::default_command_prefix(),
            Self::default_short_option_prefix(),
            Self::default_long_option_prefix(),
            Self::default_stop_parse_option(),
            Self::default_case_fold_commands(),
        )
    }
}
