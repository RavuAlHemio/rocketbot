use std::collections::{HashMap, HashSet};

use log::info;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::commands::{SedCommand, SubstituteCommand, TransposeCommand};


const SPLITTERS_STR: &'static str = "!\"#$%&'*+,-./:;=?^_`|~";
static SPLITTERS: Lazy<HashSet<char>> = Lazy::new(|| SPLITTERS_STR.chars().collect());
static KNOWN_COMMANDS: Lazy<HashSet<String>> = Lazy::new(|| vec![
    "s",
    "tr",
].into_iter().map(|o| o.to_owned()).collect());


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct GenericReplacementCommand {
    pub command: String,
    pub old_string: String,
    pub new_string: String,
    pub flags: Option<String>,
}
impl GenericReplacementCommand {
    pub fn new(
        command: String,
        old_string: String,
        new_string: String,
        flags: Option<String>,
    ) -> GenericReplacementCommand {
        GenericReplacementCommand {
            command,
            old_string,
            new_string,
            flags,
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SubFlags {
    options: String,
    first_match: isize,
    replace_all: bool,
}
impl SubFlags {
    pub fn new(
        options: String,
        first_match: isize,
        replace_all: bool,
    ) -> SubFlags {
        SubFlags {
            options,
            first_match,
            replace_all,
        }
    }
}


fn parse_sub_flags(flags: &str) -> Option<SubFlags> {
    let mut options = String::new();
    let mut first_match = 0isize;
    let mut replace_all = false;

    let mut reading_number = false;
    let mut first_match_builder = String::new();

    for c in flags.chars() {
        if c == '-' {
            if first_match_builder.len() > 0 {
                // minus midway through a number => invalid
                return None;
            }
            reading_number = true;
            first_match_builder.push(c);
        } else if c >= '0' && c <= '9' {
            if !reading_number && first_match_builder.len() > 0 {
                // i123n456 => invalid
                return None;
            }
            reading_number = true;
            first_match_builder.push(c);
        } else {
            reading_number = false;

            if "inx".find(c).is_some() {
                options.push(c);
            } else if c == 'g' {
                replace_all = true;
            } else {
                // invalid flag
                return None;
            }
        }
    }

    if first_match_builder.len() > 0 {
        first_match = match first_match_builder.parse() {
            Ok(fm) => fm,
            Err(_) => {
                // invalid count
                return None;
            },
        };
    }

    Some(SubFlags::new(
        options,
        first_match,
        replace_all,
    ))
}

fn transform_replacement_string(replacement_string_sed: &str, cap_group_count: usize) -> Option<String> {
    let mut ret = String::with_capacity(replacement_string_sed.len());

    let mut escaping = false;
    for c in replacement_string_sed.chars() {
        if c == '\\' {
            if escaping {
                ret.push(c);
                escaping = false;
            } else {
                escaping = true;
            }
        } else if c == '$' {
            ret.push_str("$$");
            escaping = false;
        } else if c >= '0' && c <= '9' && escaping {
            // group reference
            let group_number = (c as usize) - ('0' as usize);
            if group_number >= cap_group_count {
                return None;
            }

            ret.push_str("${");
            ret.push(c);
            ret.push('}');
            escaping = false;
        } else {
            ret.push(c);
            escaping = false;
        }
    }

    Some(ret)
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TakenReplacementCommand {
    pub command: Option<GenericReplacementCommand>,
    pub rest: String,
    pub invalid_command: bool,
}
impl TakenReplacementCommand {
    pub fn new(
        command: Option<GenericReplacementCommand>,
        rest: String,
        invalid_command: bool,
    ) -> TakenReplacementCommand {
        TakenReplacementCommand {
            command,
            rest,
            invalid_command,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ParserState {
    AwaitingCommand,
    AwaitingPattern,
    AwaitingReplacement,
    AwaitingFlags,
}

fn take_replacement_command(mut full_command: &str) -> TakenReplacementCommand {
    let mut command: Option<String> = None;
    let mut pattern: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut splitter: Option<char> = None;

    let mut state = ParserState::AwaitingCommand;
    let mut escaping = false;
    let mut builder = String::new();

    full_command = full_command.trim_start();

    for (i, c) in full_command.chars().enumerate() {
        if state == ParserState::AwaitingCommand {
            if c >= 'a' && c <= 'z' {
                builder.push(c)
            } else if SPLITTERS.contains(&c) {
                splitter = Some(c);
                command = Some(builder);
                builder = String::new();

                if !KNOWN_COMMANDS.contains(command.as_ref().unwrap()) {
                    // unknown command
                    return TakenReplacementCommand::new(
                        None,
                        full_command.to_string(),
                        true,
                    );
                }

                state = ParserState::AwaitingPattern;
            } else {
                // obviously not a command
                return TakenReplacementCommand::new(
                    None,
                    full_command.to_string(),
                    true,
                );
            }
        } else {
            if c == '\\' {
                if escaping {
                    builder.push_str("\\\\");
                    escaping = false;
                } else {
                    escaping = true;
                }
            } else if c == splitter.unwrap() {
                if escaping {
                    builder.push('\\');
                    builder.push(c);
                    escaping = false;
                } else if state == ParserState::AwaitingPattern {
                    pattern = Some(builder);
                    builder = String::new();
                    state = ParserState::AwaitingReplacement;
                } else if state == ParserState::AwaitingReplacement {
                    replacement = Some(builder);
                    builder = String::new();
                    state = ParserState::AwaitingFlags;
                } else {
                    // too many separators!
                    return TakenReplacementCommand::new(
                        None,
                        full_command.to_string(),
                        false,
                    );
                }
            } else if state == ParserState::AwaitingFlags && c.is_whitespace() {
                // we're done

                // rest should include the current (whitespace) character!
                let grc = GenericReplacementCommand::new(
                    command.unwrap(),
                    pattern.unwrap(),
                    replacement.unwrap(),
                    Some(builder),
                );
                return TakenReplacementCommand::new(
                    Some(grc),
                    full_command[i..].to_owned(),
                    false,
                );
            } else {
                if escaping {
                    builder.push('\\');
                    builder.push(c);
                    escaping = false;
                } else {
                    builder.push(c);
                }
            }
        }
    }

    if command.is_none() || pattern.is_none() {
        // incomplete command!

        // bare word?
        let is_bare_word = builder.len() > 0;

        return TakenReplacementCommand::new(
            None,
            full_command.to_owned(),
            is_bare_word,
        );
    }

    // fell out of the loop: nothing left
    let grc = if replacement.is_none() {
        GenericReplacementCommand::new(
            command.unwrap(),
            pattern.unwrap(),
            builder,
            None,
        )
    } else {
        GenericReplacementCommand::new(
            command.unwrap(),
            pattern.unwrap(),
            replacement.unwrap(),
            Some(builder),
        )
    };
    TakenReplacementCommand::new(
        Some(grc),
        String::new(),
        false,
    )
}

pub(crate) fn parse_replacement_commands(message: &str) -> Option<Vec<SedCommand>> {
    let mut trimmed_message = message.trim().to_owned();

    // shortest possible command
    if trimmed_message.len() < "s/a//".len() {
        // too short
        // (if we fail at this stage, it's probably not supposed to be a sed command)
        return None;
    }
    if trimmed_message.chars().filter(|c| SPLITTERS.contains(c)).count() < 2 {
        // not enough splitter characters: not a command
        return None;
    }
    if SPLITTERS.iter().map(|s| trimmed_message.chars().filter(|c| c == s).count()).max().unwrap() < 2 {
        // not enough of the same splitter character: not a command
        return None;
    }

    let mut replacement_commands: Vec<GenericReplacementCommand> = Vec::new();
    loop {
        let sub_command = take_replacement_command(&trimmed_message);

        let cmd = match sub_command.command {
            None => {
                if sub_command.invalid_command {
                    // assume it's not supposed to be a sed command
                    return None;
                } else {
                    // assume it's a syntactically incorrect sed command
                    break;
                }
            },
            Some(cmd) => {
                if cmd.flags.is_none() {
                    // no flags: assume syntactically incorrect sed command as well
                    break;
                }

                cmd
            },
        };

        // ensure that the string is getting shorter
        assert!(sub_command.rest.len() < trimmed_message.len());

        replacement_commands.push(cmd);
        trimmed_message = sub_command.rest;
    }

    // probably is supposed to be a sed command but they are doing it wrong
    // return an empty list
    if replacement_commands.len() == 0 {
        info!("already the first replacement command was invalid in {}", trimmed_message);
        return Some(Vec::new());
    }

    let mut ret = Vec::with_capacity(replacement_commands.len());
    for replacement_command in &replacement_commands {
        let command_opt = if replacement_command.command == "s" {
            make_substitute_command(replacement_command)
        } else if replacement_command.command == "tr" {
            make_transpose_command(replacement_command)
        } else {
            // unknown command
            return Some(Vec::new());
        };

        if let Some(cmd) = command_opt {
            ret.push(cmd);
        } else {
            // building command failed
            return Some(Vec::new());
        }
    }

    Some(ret)
}

fn make_substitute_command(command: &GenericReplacementCommand) -> Option<SedCommand> {
    let flags = match &command.flags {
        None => return None,
        Some(f) => f,
    };
    let sub_flags = match parse_sub_flags(&flags) {
        None => {
            info!("invalid flag {}", flags);
            return None;
        },
        Some(sf) => sf,
    };

    let flagged_regex_string = if sub_flags.options.len() > 0 {
        format!("(?{}){}", sub_flags.options, command.old_string)
    } else {
        command.old_string.clone()
    };
    let flagged_regex = match Regex::new(&flagged_regex_string) {
        Err(_) => {
            info!("syntactic error in pattern {}", flagged_regex_string);
            return None;
        },
        Ok(r) => r,
    };

    let replacement_string_opt = transform_replacement_string(
        &command.new_string,
        flagged_regex.captures_len(),
    );
    let replacement_string = match replacement_string_opt {
        Some(rs) => rs,
        None => {
            info!(
                "error in replacement string {:?} for pattern {:?}",
                command.new_string,
                flagged_regex_string,
            );
            return None;
        },
    };

    Some(SedCommand::Substitute(SubstituteCommand::new(
        flagged_regex,
        replacement_string,
        sub_flags.first_match,
        sub_flags.replace_all,
    )))
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TranspositionMode {
    OneToOne,
    DeleteMissingTo,
    RepeatLastTo,
}

const MAX_RANGE_DIFFERENCE: usize = 128;

fn make_transpose_command(command: &GenericReplacementCommand) -> Option<SedCommand> {
    let flags = match &command.flags {
        None => return None,
        Some(f) => f,
    };
    let transpo_mode = match flags.as_str() {
        "d" => TranspositionMode::DeleteMissingTo,
        "r" => TranspositionMode::RepeatLastTo,
        "" => TranspositionMode::OneToOne,
        _ => {
            info!("incorrect flags {:?}", flags);
            return None;
        },
    };

    let transpo_dict_opt = parse_transpositions(
        &command.old_string,
        &command.new_string,
        transpo_mode,
    );
    match transpo_dict_opt {
        None => None,
        Some(td) => Some(
            SedCommand::Transpose(TransposeCommand::new(td)),
        ),
    }
}

fn parse_transpositions(from_string: &str, to_string: &str, transpo_mode: TranspositionMode) -> Option<HashMap<char, Option<char>>> {
    let froms: Vec<char> = match parse_with_ranges(&from_string) {
        Some(f) => f,
        None => return None,
    };
    let tos: Vec<char> = match parse_with_ranges(&to_string) {
        Some(t) => t,
        None => return None,
    };

    match transpo_mode {
        TranspositionMode::OneToOne => {
            if froms.len() != tos.len() {
                info!("from characters ({}) and to characters ({}) differ in count", froms.len(), tos.len());
                return None;
            }
        },
        TranspositionMode::RepeatLastTo|TranspositionMode::DeleteMissingTo => {
            // tos may be shorter than froms but not vice versa
            if froms.len() < tos.len() {
                info!("fewer from characters ({}) than to characters ({})", froms.len(), tos.len());
                return None;
            }
        },
    }

    if transpo_mode == TranspositionMode::RepeatLastTo && tos.len() == 0 {
        info!("mode is RepeatLastTo but there are no to characters");
        return None;
    }

    let mut ret = HashMap::new();
    for i in 0..froms.len().min(tos.len()) {
        ret.insert(froms[i], Some(tos[i]));
    }

    if transpo_mode == TranspositionMode::RepeatLastTo {
        assert!(froms.len() >= tos.len());
        for i in tos.len()..froms.len() {
            ret.insert(froms[i], Some(tos[tos.len() - 1]));
        }
    } else if transpo_mode == TranspositionMode::DeleteMissingTo {
        assert!(froms.len() >= tos.len());
        for i in tos.len()..froms.len() {
            ret.insert(froms[i], None);
        }
    }

    Some(ret)
}

fn parse_with_ranges(spec: &str) -> Option<Vec<char>> {
    let mut ret = Vec::new();
    let spec_chars: Vec<char> = spec.chars().collect();

    let mut i = 0usize;
    while i < spec_chars.len() {
        let this_char = spec_chars[i];
        let next_char = spec_chars.get(i+1);
        let next_but_one_char = spec_chars.get(i+2);

        if this_char == '\\' {
            if let Some(&nc) = next_char {
                // escaped character
                // "\a"
                ret.push(nc);

                i += 2;
            } else {
                // open escape at the end of the string
                info!("open escape at end");
                return None;
            }
        } else if next_char == Some(&'-') {
            if let Some(&nboc) = next_but_one_char {
                // it's a range!
                // "a-z"

                if nboc < this_char {
                    // except it's an invalid one, ffs
                    // "z-a"
                    info!("character range from {} to {} is inverted", this_char, nboc);
                    return None;
                }

                let range_delta: usize = (nboc as usize) - (this_char as usize);
                if range_delta > MAX_RANGE_DIFFERENCE {
                    // this range is far too long
                    // "\u{0000}-\u{FFFF}"
                    info!(
                        "character range from {} to {} is greater than limit {}",
                        this_char,
                        nboc,
                        MAX_RANGE_DIFFERENCE,
                    );
                    return None;
                }

                for c in this_char..=nboc {
                    ret.push(c);
                }

                // advance by three characters
                i += 3;
            } else {
                // string ends before the range end; assume the "-" is meant as a singular character
                // "a-"
                ret.push(this_char);
                ret.push('-');
            }
        } else {
            // "-ab" or "abc"

            // just add the current character
            ret.push(this_char);

            // advance by one
            i += 1;
        }
    }

    Some(ret)
}
