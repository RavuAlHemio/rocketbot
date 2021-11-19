use std::collections::{HashMap, HashSet};

use bitflags::bitflags;


bitflags! {
    pub struct CommandBehaviors: u64 {
        const ACCEPT_FROM_BOTS = 0b00000001;
        const NO_ARGUMENT_PARSING = 0b00000010;
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
    pub fn new(
        name: String,
        plugin_name: String,
        usage: String,
        description: String,
    ) -> Self {
        let definition = CommandDefinition::new(
            name,
            plugin_name,
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            usage,
            description,
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

    pub fn add_flag(mut self, new_flag: &str) -> Self {
        if let Some(flags) = &mut self.definition.flags {
            flags.insert(new_flag.to_owned());
        }
        self
    }

    pub fn options(mut self, new_options: HashMap<String, CommandValueType>) -> Self {
        self.definition.options = new_options;
        self
    }

    pub fn add_option(mut self, new_option: &str, new_type: CommandValueType) -> Self {
        self.definition.options.insert(new_option.to_owned(), new_type);
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


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CommandConfiguration {
    pub command_prefix: String,
    pub short_option_prefix: String,
    pub long_option_prefix: String,
    pub stop_parse_option: String,
}
impl CommandConfiguration {
    pub fn new(
        command_prefix: String,
        short_option_prefix: String,
        long_option_prefix: String,
        stop_parse_option: String,
    ) -> CommandConfiguration {
        CommandConfiguration {
            command_prefix,
            short_option_prefix,
            long_option_prefix,
            stop_parse_option,
        }
    }
}
impl Default for CommandConfiguration {
    fn default() -> Self {
        CommandConfiguration::new(
            String::from("!"),
            String::from("-"),
            String::from("--"),
            String::from("--"),
        )
    }
}
