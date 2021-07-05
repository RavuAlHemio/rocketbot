use std::collections::{HashMap, HashSet};


#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum CommandValueType {
    String,
    Integer,
    Float,
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDefinition {
    pub name: String,
    pub flags: HashSet<String>,
    pub options: HashMap<String, CommandValueType>,
    pub arg_count: usize,
}
impl CommandDefinition {
    pub fn new(
        name: String,
        flags: HashSet<String>,
        options: HashMap<String, CommandValueType>,
        arg_count: usize,
    ) -> CommandDefinition {
        CommandDefinition {
            name,
            flags,
            options,
            arg_count,
        }
    }

    pub fn copy_named(&self, new_name: &str) -> CommandDefinition {
        let mut ret = self.clone();
        ret.name = new_name.to_owned();
        ret
    }
}


#[derive(Clone, Debug, PartialEq)]
pub enum CommandValue {
    String(String),
    Integer(i64),
    Float(f64),
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
