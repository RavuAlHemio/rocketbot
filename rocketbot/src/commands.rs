use std::collections::{HashMap, HashSet};

use rocketbot_interface::commands::{
    CommandBehaviors, CommandConfiguration, CommandDefinition, CommandInstance, CommandValue,
    CommandValueType,
};
use tracing::{debug, warn};

use crate::string_utils::Token;


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum OptionHandlingResult {
    Failure,
    Flag,
    Option,
}

pub(crate) fn parse_command(
    command: &CommandDefinition,
    command_config: &CommandConfiguration,
    pieces: &[Token],
    raw_message: &str,
    preceding_quote: Option<&str>,
) -> Option<CommandInstance> {
    // bot check must be performed by caller
    // preceding quote check must be performed by caller

    if command.behaviors.contains(CommandBehaviors::NO_ARGUMENT_PARSING) {
        let rest_string = if pieces.len() > 1 {
            raw_message[pieces[1].orig_range.start..].to_owned()
        } else {
            // nothing beyond the command itself
            String::new()
        };

        return Some(CommandInstance::new(
            command.name.clone(),
            HashSet::new(),
            HashMap::new(),
            Vec::new(),
            rest_string,
            preceding_quote.map(|q| q.to_owned()),
        ));
    }

    let mut i = 1;
    let mut set_flags = HashSet::new();
    let mut option_values = HashMap::new();
    let mut pos_args = Vec::with_capacity(command.arg_count);
    while i < pieces.len() {
        if pieces[i].value == command_config.stop_parse_option {
            // no more options beyond this point, just positional args
            // move forward and stop parsing
            i += 1;
            break;
        }

        // this code assumes that the long option prefix is not a prefix of the short option prefix
        // (e.g. short option prefix "--", long option prefix "-")
        assert!(!command_config.short_option_prefix.starts_with(&command_config.long_option_prefix));

        if pieces[i].value.starts_with(&command_config.long_option_prefix) {
            // it's a long option/flag!
            let option_name = &pieces[i].value[command_config.long_option_prefix.len()..];

            let handling_result = handle_option(
                &command,
                &option_name,
                None,
                &mut i,
                &mut set_flags,
                &mut option_values,
                &pieces,
                &command.name,
                &raw_message,
            );
            if let OptionHandlingResult::Failure = handling_result {
                // error messages already logged
                return None;
            }
        } else if pieces[i].value.starts_with(&command_config.short_option_prefix) {
            // it's a bunch of short options!
            let mut value_consumed_by_option: Option<String> = None;

            for c in pieces[i].value[command_config.short_option_prefix.len()..].chars() {
                let option_name: String = String::from(c);

                let handling_result = handle_option(
                    &command,
                    &option_name,
                    value_consumed_by_option.as_deref(),
                    &mut i,
                    &mut set_flags,
                    &mut option_values,
                    &pieces,
                    &command.name,
                    &raw_message,
                );
                match handling_result {
                    OptionHandlingResult::Failure => {
                        // error messages already logged
                        return None;
                    },
                    OptionHandlingResult::Flag => {
                        // no worries here
                    },
                    OptionHandlingResult::Option => {
                        // make sure we remember that this option gobbled up an argument
                        value_consumed_by_option = Some(option_name);
                    },
                }
            }
        } else if pos_args.len() < command.arg_count || command.behaviors.contains(CommandBehaviors::REST_AS_ARGS) {
            // positional argument
            pos_args.push(pieces[i].value.to_owned());
        } else {
            // assume it's part of the rest
            break;
        }

        i += 1;
    }

    // gobble up the remaining positional arguments
    while pos_args.len() < command.arg_count {
        if i >= pieces.len() {
            warn!("missing positional argument (got {}, need {}) passed to {}", pos_args.len(), command.arg_count, command.name);
            debug!("command line is {:?}", raw_message);
            return None;
        }
        pos_args.push(pieces[i].value.to_owned());
        i += 1;
    }

    // take the rest
    let rest_string = if i < pieces.len() {
        raw_message[pieces[i].orig_range.start..].to_owned()
    } else {
        String::new()
    };

    // it finally comes together
    Some(CommandInstance::new(
        command.name.clone(),
        set_flags,
        option_values,
        pos_args,
        rest_string,
        preceding_quote.map(|q| q.to_owned()),
    ))
}

fn handle_option(
    command: &CommandDefinition,
    option_name: &str,
    value_consumed_by_option: Option<&str>,
    i: &mut usize,
    set_flags: &mut HashSet<String>,
    option_values: &mut HashMap<String, CommandValue>,
    pieces: &[Token],
    command_name: &str,
    raw_message: &str,
) -> OptionHandlingResult {
    let is_flag = command.flags
        .as_ref()
        .map(|cf| cf.contains(option_name))
        .unwrap_or(false);

    if is_flag {
        // flag found!
        set_flags.insert(option_name.to_owned());
        OptionHandlingResult::Flag
    } else {
        // is it an option?
        let option_type = match command.options.get(option_name) {
            Some(ot) => *ot,
            None => {
                if command.flags.is_none() {
                    // command allows custom flags; it's one of those
                    set_flags.insert(option_name.to_owned());
                    return OptionHandlingResult::Flag;
                }

                warn!("unknown option {:?} passed to {}", option_name, command_name);
                debug!("command line is {:?}", raw_message);
                return OptionHandlingResult::Failure;
            }
        };

        if let Some(vcbo) = value_consumed_by_option {
            // e.g. "-abcd value" where both -a and -b take a value
            warn!("option {:?} passed to {} wants a value which was already consumed by option {:?}", option_name, command_name, vcbo);
            debug!("command line is {:?}", raw_message);
            return OptionHandlingResult::Failure;
        }

        // is there a next piece?
        if *i + 1 >= pieces.len() {
            warn!("option {:?} of {} is missing an argument", option_name, command_name);
            debug!("command line is {:?}", raw_message);
            return OptionHandlingResult::Failure;
        }
        let option_value_str = &pieces[*i + 1].value;

        if option_type == CommandValueType::MultiString {
            let current_values = option_values.entry(option_name.to_owned())
                .or_insert_with(|| CommandValue::MultiString(Vec::new()))
                .as_multi_string_mut().unwrap();
            current_values.push(option_value_str.to_owned());
        } else {
            let option_value = match option_type {
                CommandValueType::String => CommandValue::String(option_value_str.to_owned()),
                CommandValueType::Float => {
                    let float_val: f64 = match option_value_str.parse() {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("failed to parse argument {:?} for option {:?} of {} as a floating-point value: {}", option_value_str, option_name, command_name, e);
                            debug!("command line is {:?}", raw_message);
                            return OptionHandlingResult::Failure;
                        },
                    };
                    CommandValue::Float(float_val)
                },
                CommandValueType::Integer => {
                    let int_val: i64 = match option_value_str.parse() {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("failed to parse argument {:?} for option {:?} of {} as an integer: {}", option_value_str, option_name, command_name, e);
                            debug!("command line is {:?}", raw_message);
                            return OptionHandlingResult::Failure;
                        },
                    };
                    CommandValue::Integer(int_val)
                },
                CommandValueType::MultiString => unreachable!(),
            };

            option_values.insert(option_name.to_owned(), option_value);
        }

        // skip over one more argument (the value to this option)
        *i += 1;

        OptionHandlingResult::Option
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocketbot_interface::commands::{CommandBehaviors, CommandDefinitionBuilder};
    use crate::string_utils::tokenize;

    fn perform_test(command: &CommandDefinition, message: &str) -> Option<CommandInstance> {
        let pieces: Vec<Token> = tokenize(&message).collect();
        parse_command(
            command,
            &CommandConfiguration::default(),
            &pieces,
            &message,
            None,
        )
    }

    #[test]
    fn test_empty() {
        let command = CommandDefinition::new(
            "bloop".into(),
            "bloop".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}bloop [STUFF]".to_owned(),
            "Bloops.".to_owned(),
        );
        let cmd_inst = perform_test(
            &command,
            "!bloop",
        ).unwrap();

        assert_eq!("bloop", cmd_inst.name);
        assert_eq!(0, cmd_inst.flags.len());
        assert_eq!(0, cmd_inst.options.len());
        assert_eq!(0, cmd_inst.args.len());
        assert_eq!("", cmd_inst.rest);
    }

    #[test]
    fn test_rest() {
        let command = CommandDefinition::new(
            "bloop".into(),
            "bloop".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}bloop [STUFF]".to_owned(),
            "Bloops.".to_owned(),
        );
        let cmd_inst = perform_test(
            &command,
            "!bloop  one   two    three",
        ).unwrap();

        assert_eq!("bloop", cmd_inst.name);
        assert_eq!(0, cmd_inst.flags.len());
        assert_eq!(0, cmd_inst.options.len());
        assert_eq!(0, cmd_inst.args.len());
        assert_eq!("one   two    three", cmd_inst.rest);
    }

    #[test]
    fn test_single_arg() {
        let command = CommandDefinition::new(
            "bloop".into(),
            "bloop".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            1,
            CommandBehaviors::empty(),
            "{cpfx}bloop [STUFF]".to_owned(),
            "Bloops.".to_owned(),
        );
        let cmd_inst = perform_test(
            &command,
            "!bloop  one   two    three",
        ).unwrap();

        assert_eq!("bloop", cmd_inst.name);
        assert_eq!(0, cmd_inst.flags.len());
        assert_eq!(0, cmd_inst.options.len());
        assert_eq!(1, cmd_inst.args.len());
        assert_eq!("one", cmd_inst.args[0]);
        assert_eq!("two    three", cmd_inst.rest);
    }

    #[test]
    fn test_quoting() {
        let command = CommandDefinitionBuilder::new(
            "bloop",
            "bloop",
            "{cpfx}bloop [STUFF]",
            "Bloops.",
        )
            .arg_count(2)
            .build();
        let cmd_inst = perform_test(
            &command,
            "!bloop  \"one   two  \"  three",
        ).unwrap();

        assert_eq!("bloop", cmd_inst.name);
        assert_eq!(0, cmd_inst.flags.len());
        assert_eq!(0, cmd_inst.options.len());
        assert_eq!(2, cmd_inst.args.len());
        assert_eq!("one   two  ", cmd_inst.args[0]);
        assert_eq!("three", cmd_inst.args[1]);
        assert_eq!("", cmd_inst.rest);
    }

    #[test]
    fn test_multi_arg() {
        let command = CommandDefinitionBuilder::new(
            "bloop",
            "bloop",
            "{cpfx}bloop [{sopfx}o OPTION...]",
            "Bloops.",
        )
            .add_option("c", CommandValueType::MultiString)
            .build();
        let cmd_inst = perform_test(
            &command,
            "!bloop -c one -c another roesti",
        ).unwrap();

        assert_eq!("bloop", cmd_inst.name);
        assert_eq!(0, cmd_inst.flags.len());
        assert_eq!(1, cmd_inst.options.len());
        let option_c = cmd_inst.options.get("c")
            .expect("option -c value missing");
        let c_values = option_c.as_multi_string()
            .expect("option -c not a multistring");
        assert_eq!(c_values.len(), 2);
        assert_eq!(c_values[0], "one");
        assert_eq!(c_values[1], "another");
        assert_eq!(0, cmd_inst.args.len());
        assert_eq!("roesti", cmd_inst.rest);
    }
}
