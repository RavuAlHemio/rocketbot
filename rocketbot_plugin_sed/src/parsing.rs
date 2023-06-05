use std::collections::{HashMap, HashSet};
use std::fmt;

use fancy_regex::Regex;
use once_cell::sync::Lazy;

use crate::commands::{ExchangeCommand, SedCommand, SubstituteCommand, TransposeCommand};


const SPLITTERS_STR: &'static str = "!\"#$%&'*+,-./:;=?^_`|~";
static SPLITTERS: Lazy<HashSet<char>> = Lazy::new(|| SPLITTERS_STR.chars().collect());
static KNOWN_COMMANDS: Lazy<HashSet<String>> = Lazy::new(|| vec![
    "s",
    "tr",
    "x",
].into_iter().map(|o| o.to_owned()).collect());


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ParserError {
    NonCommandCharacter{ character: char, index: usize },
    UnknownCommand{ command: String, splitter_index: usize },
    TooManySeparators{ separator: char, index: usize },
    IncompleteCommand,
    MissingFlags,
    InvalidSubFlags(SubFlagsError),
    InvalidTransposeMode(String),
    PatternSyntaxError{ pattern: String, error_description: String },
    ReplacementSyntaxError{ replacement: String, error: ReplacementError },
    TransposeRangeLengthMismatch{ froms: Vec<char>, tos: Vec<char> },
    TransposeFromsShort{ froms: Vec<char>, tos: Vec<char> },
    TransposeRepeatLastNothing{ froms: Vec<char> },
    RangeTrailingEscape,
    RangeInverted{ from: char, to: char },
    RangeTooLarge{ from: char, to: char, delta: usize, limit: usize },
}
impl ParserError {
    /// Returns whether this error hints that the string probably isn't a sed command.
    pub fn is_disqualifying(&self) -> bool {
        match self {
            Self::NonCommandCharacter{ .. } => true,
            Self::UnknownCommand{ .. } => true,
            Self::TooManySeparators{ .. } => true,
            Self::IncompleteCommand{ .. } => true,
            _ => false,
        }
    }
}
impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonCommandCharacter{ character, index }
                => write!(f, "non-command character {:?} at index {}", character, index),
            Self::UnknownCommand{ command, splitter_index }
                => write!(f, "unknown command {:?}; splitter index is {}", command, splitter_index),
            Self::TooManySeparators{ separator, index }
                => write!(f, "command contains supernumerary separator {:?} at index {}", separator, index),
            Self::IncompleteCommand
                => write!(f, "incomplete command"),
            Self::MissingFlags
                => write!(f, "no flags supplied to command"),
            Self::InvalidSubFlags(sfe)
                => write!(f, "invalid flags: {}", sfe),
            Self::InvalidTransposeMode(m)
                => write!(f, "invalid transposition mode {:?}", m),
            Self::PatternSyntaxError{ pattern, error_description }
                => write!(f, "syntax error in pattern {:?}: {}", pattern, error_description),
            Self::ReplacementSyntaxError{ replacement, error }
                => write!(f, "syntax error in replacement {:?}: {}", replacement, error),
            Self::TransposeRangeLengthMismatch{ froms, tos }
                => write!(f, "from characters ({}) and to characters ({}) differ in count", froms.len(), tos.len()),
            Self::TransposeFromsShort{ froms, tos }
                => write!(f, "fewer from characters ({}) than to characters ({})", froms.len(), tos.len()),
            Self::TransposeRepeatLastNothing{ froms: _ }
                => write!(f, "mode is RepeatLastTo but there are no to characters"),
            Self::RangeTrailingEscape
                => write!(f, "range contains trailing escape character"),
            Self::RangeInverted{ from, to }
                => write!(f, "character range from {:?} to {:?} is inverted", from, to),
            Self::RangeTooLarge{ from, to, delta, limit }
                => write!(f, "character range from {:?} to {:?} delta {} is greater than limit {}", from, to, delta, limit),
        }
    }
}
impl std::error::Error for ParserError {
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SubFlagsError {
    MinusWithinNumber{ index: usize },
    SecondNumberGroup{ first_group: String, index: usize },
    UnknownFlag{ flag_char: char, index: usize },
    InvalidCount{ count_string: String },
}
impl fmt::Display for SubFlagsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MinusWithinNumber{ index }
                => write!(f, "minus sign within number at index {}", index),
            Self::SecondNumberGroup{ first_group, index }
                => write!(f, "second group of numbers after {:?} found starting at index {}", first_group, index),
            Self::UnknownFlag{ flag_char, index }
                => write!(f, "unknown flag {:?} at index {}", flag_char, index),
            Self::InvalidCount{ count_string }
                => write!(f, "invalid count {:?}", count_string),
        }
    }
}
impl std::error::Error for SubFlagsError {
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ReplacementError {
    GroupOutOfRange{ group_number: usize },
    TrailingEscape,
    GroupNumberOverflow,
    InvalidGroupSyntax,
}
impl fmt::Display for ReplacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GroupOutOfRange{ group_number }
                => write!(f, "group with number {} out of range", group_number),
            Self::TrailingEscape
                => write!(f, "trailing escape character"),
            Self::GroupNumberOverflow
                => write!(f, "group number overflowed"),
            Self::InvalidGroupSyntax
                => write!(f, "invalid group syntax"),
        }
    }
}
impl std::error::Error for ReplacementError {
}


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


fn parse_sub_flags(flags: &str) -> Result<SubFlags, SubFlagsError> {
    let mut options = String::new();
    let mut first_match = 0isize;
    let mut replace_all = false;

    let mut reading_number = false;
    let mut first_match_builder = String::new();

    for (i, c) in flags.char_indices() {
        if c == '-' {
            if first_match_builder.len() > 0 {
                // minus midway through a number => invalid
                return Err(SubFlagsError::MinusWithinNumber{ index: i });
            }
            reading_number = true;
            first_match_builder.push(c);
        } else if c >= '0' && c <= '9' {
            if !reading_number && first_match_builder.len() > 0 {
                // i123n456 => invalid
                return Err(SubFlagsError::SecondNumberGroup{ first_group: first_match_builder, index: i });
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
                return Err(SubFlagsError::UnknownFlag{ flag_char: c, index: i });
            }
        }
    }

    if first_match_builder.len() > 0 {
        first_match = match first_match_builder.parse() {
            Ok(fm) => fm,
            Err(_) => {
                // invalid count
                return Err(SubFlagsError::InvalidCount{ count_string: first_match_builder });
            },
        };
    }

    Ok(SubFlags::new(
        options,
        first_match,
        replace_all,
    ))
}

fn transform_replacement_string(replacement_string_sed: &str, cap_group_count: usize) -> Result<String, ReplacementError> {
    let mut ret = String::with_capacity(replacement_string_sed.len());

    let mut escaping = false;
    let mut parsing_group = '\0';
    let mut parsing_group_value: usize = 0;
    for c in replacement_string_sed.chars() {
        if c == '\\' {
            if escaping {
                ret.push(c);
                escaping = false;
            } else {
                escaping = true;
            }
        } else if c == 'g' && escaping && parsing_group == '\0' {
            // parse numeric group ("\g123;")
            parsing_group = 'g';
            parsing_group_value = 0;
            escaping = false;
        } else if parsing_group == 'g' {
            if c >= '0' && c <= '9' {
                let digit = (c as usize) - ('0' as usize);
                parsing_group_value = parsing_group_value.checked_mul(10)
                    .ok_or_else(|| ReplacementError::GroupNumberOverflow)?
                    .checked_add(digit)
                    .ok_or_else(|| ReplacementError::GroupNumberOverflow)?;
            } else if c == ';' {
                if parsing_group_value >= cap_group_count {
                    return Err(ReplacementError::GroupOutOfRange{ group_number: parsing_group_value });
                }
                ret.push_str("${");
                ret.push_str(&parsing_group_value.to_string());
                ret.push('}');
                parsing_group = '\0';
            } else {
                return Err(ReplacementError::InvalidGroupSyntax);
            }
        } else if c == '$' {
            ret.push_str("$$");
            escaping = false;
        } else if c >= '0' && c <= '9' && escaping {
            // group reference
            let group_number = (c as usize) - ('0' as usize);
            if group_number >= cap_group_count {
                return Err(ReplacementError::GroupOutOfRange{ group_number });
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

    if escaping {
        return Err(ReplacementError::TrailingEscape);
    }

    Ok(ret)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ParserState {
    SkippingWhitespace,
    ReadingCommand,
    ReadingPattern,
    ReadingReplacement,
    ReadingFlags,
}

fn take_replacement_command(full_command: &str, start_at: usize) -> Result<(GenericReplacementCommand, usize), ParserError> {
    let mut command: Option<String> = None;
    let mut pattern: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut splitter: Option<char> = None;

    let mut state = ParserState::SkippingWhitespace;
    let mut escaping = false;
    let mut builder = String::new();

    for (i, c) in full_command.chars().enumerate() {
        if i < start_at {
            // skip beginning
            continue;
        }

        if state == ParserState::SkippingWhitespace {
            if c >= 'a' && c <= 'z' {
                builder.push(c);
                state = ParserState::ReadingCommand;
            } else if !c.is_whitespace() {
                return Err(ParserError::NonCommandCharacter{ character: c, index: i });
            }
        } else if state == ParserState::ReadingCommand {
            if c >= 'a' && c <= 'z' {
                builder.push(c);
            } else if SPLITTERS.contains(&c) {
                splitter = Some(c);
                command = Some(builder);
                builder = String::new();

                if !KNOWN_COMMANDS.contains(command.as_ref().unwrap()) {
                    // unknown command
                    return Err(ParserError::UnknownCommand{ command: command.unwrap(), splitter_index: i });
                }

                state = ParserState::ReadingPattern;
            } else {
                return Err(ParserError::NonCommandCharacter{ character: c, index: i });
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
                } else if state == ParserState::ReadingPattern {
                    pattern = Some(builder);
                    builder = String::new();
                    state = ParserState::ReadingReplacement;
                } else if state == ParserState::ReadingReplacement {
                    replacement = Some(builder);
                    builder = String::new();
                    state = ParserState::ReadingFlags;
                } else {
                    // too many separators!
                    return Err(ParserError::TooManySeparators{ separator: c, index: i });
                }
            } else if state == ParserState::ReadingFlags && c.is_whitespace() {
                // we're done

                // rest should include the current (whitespace) character!
                let grc = GenericReplacementCommand::new(
                    command.unwrap(),
                    pattern.unwrap(),
                    replacement.unwrap(),
                    Some(builder),
                );
                return Ok((grc, i));
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
        // incomplete command at end of string!
        return Err(ParserError::IncompleteCommand);
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
    Ok((grc, full_command.len()))
}

pub(crate) fn parse_replacement_commands(message: &str) -> Result<Vec<SedCommand>, ParserError> {
    let mut start_at = 0;
    let mut replacement_commands: Vec<GenericReplacementCommand> = Vec::new();
    while start_at < message.len() {
        let (sub_command, next_index) = take_replacement_command(message, start_at)?;
        replacement_commands.push(sub_command);

        // ensure that we are progressing through the string
        assert!(next_index > start_at);

        start_at = next_index;
    }

    let mut ret = Vec::with_capacity(replacement_commands.len());
    for replacement_command in &replacement_commands {
        // also add new commands to KNOWN_COMMANDS above
        let command = if replacement_command.command == "s" {
            make_substitute_command(replacement_command)?
        } else if replacement_command.command == "tr" {
            make_transpose_command(replacement_command)?
        } else if replacement_command.command == "x" {
            make_exchange_command(replacement_command)?
        } else {
            // command validity has already been checked by take_replacement_command
            unreachable!();
        };

        ret.push(command);
    }

    Ok(ret)
}

fn make_substitute_command(command: &GenericReplacementCommand) -> Result<SedCommand, ParserError> {
    let flags = match &command.flags {
        None => return Err(ParserError::MissingFlags),
        Some(f) => f,
    };
    let sub_flags = parse_sub_flags(&flags)
        .map_err(|sfe| ParserError::InvalidSubFlags(sfe))?;

    let flagged_regex_string = if sub_flags.options.len() > 0 {
        format!("(?{}){}", sub_flags.options, command.old_string)
    } else {
        command.old_string.clone()
    };
    let flagged_regex = Regex::new(&flagged_regex_string)
        .map_err(|e| ParserError::PatternSyntaxError{ pattern: flagged_regex_string, error_description: e.to_string() })?;

    let replacement_string = transform_replacement_string(
        &command.new_string,
        flagged_regex.captures_len(),
    )
        .map_err(|e| ParserError::ReplacementSyntaxError{ replacement: command.new_string.clone(), error: e })?;

    Ok(SedCommand::Substitute(SubstituteCommand::new(
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

fn make_transpose_command(command: &GenericReplacementCommand) -> Result<SedCommand, ParserError> {
    let flags = match &command.flags {
        None => return Err(ParserError::MissingFlags),
        Some(f) => f,
    };
    let transpo_mode = match flags.as_str() {
        "d" => TranspositionMode::DeleteMissingTo,
        "r" => TranspositionMode::RepeatLastTo,
        "" => TranspositionMode::OneToOne,
        _ => return Err(ParserError::InvalidTransposeMode(flags.clone())),
    };

    parse_transpositions(
        &command.old_string,
        &command.new_string,
        transpo_mode,
    )
        .map(|td| SedCommand::Transpose(TransposeCommand::new(td)))
}

fn parse_transpositions(from_string: &str, to_string: &str, transpo_mode: TranspositionMode) -> Result<HashMap<char, Option<char>>, ParserError> {
    let froms: Vec<char> = parse_with_ranges(&from_string)?;
    let tos: Vec<char> = parse_with_ranges(&to_string)?;

    match transpo_mode {
        TranspositionMode::OneToOne => {
            if froms.len() != tos.len() {
                return Err(ParserError::TransposeRangeLengthMismatch{ froms, tos });
            }
        },
        TranspositionMode::RepeatLastTo|TranspositionMode::DeleteMissingTo => {
            // tos may be shorter than froms but not vice versa
            if froms.len() < tos.len() {
                return Err(ParserError::TransposeFromsShort{ froms, tos });
            }
        },
    }

    if transpo_mode == TranspositionMode::RepeatLastTo && froms.len() > 0 && tos.len() == 0 {
        return Err(ParserError::TransposeRepeatLastNothing{ froms });
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

    Ok(ret)
}

fn parse_with_ranges(spec: &str) -> Result<Vec<char>, ParserError> {
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
                return Err(ParserError::RangeTrailingEscape);
            }
        } else if next_char == Some(&'-') {
            if let Some(&nboc) = next_but_one_char {
                // it's a range!
                // "a-z"

                if nboc < this_char {
                    // except it's an invalid one, ffs
                    // "z-a"
                    return Err(ParserError::RangeInverted{ from: this_char, to: nboc });
                }

                let range_delta: usize = (nboc as usize) - (this_char as usize);
                if range_delta > MAX_RANGE_DIFFERENCE {
                    // this range is far too long
                    // "\u{0000}-\u{FFFF}"
                    return Err(ParserError::RangeTooLarge{
                        from: this_char,
                        to: nboc,
                        delta: range_delta,
                        limit: MAX_RANGE_DIFFERENCE,
                    });
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

    Ok(ret)
}

fn make_exchange_command(command: &GenericReplacementCommand) -> Result<SedCommand, ParserError> {
    let from_regex = Regex::new(&command.old_string)
        .map_err(|e| ParserError::PatternSyntaxError { pattern: command.old_string.clone(), error_description: e.to_string() })?;
    let to_regex = Regex::new(&command.new_string)
        .map_err(|e| ParserError::PatternSyntaxError { pattern: command.new_string.clone(), error_description: e.to_string() })?;
    Ok(SedCommand::Exchange(ExchangeCommand::new(
        from_regex,
        to_regex,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_sub_flags() {
        {
            let flags = parse_sub_flags("123").unwrap();
            assert_eq!(flags.first_match, 123);
            assert_eq!(flags.options, "");
            assert!(!flags.replace_all);
        }

        {
            let flags = parse_sub_flags("321gi").unwrap();
            assert_eq!(flags.first_match, 321);
            assert_eq!(flags.options, "i");
            assert!(flags.replace_all);
        }

        {
            let flags = parse_sub_flags("31i").unwrap();
            assert_eq!(flags.first_match, 31);
            assert_eq!(flags.options, "i");
            assert!(!flags.replace_all);
        }

        {
            let flags = parse_sub_flags("g").unwrap();
            assert_eq!(flags.first_match, 0);
            assert_eq!(flags.options, "");
            assert!(flags.replace_all);
        }

        {
            let flags = parse_sub_flags("i-2").unwrap();
            assert_eq!(flags.first_match, -2);
            assert_eq!(flags.options, "i");
            assert!(!flags.replace_all);
        }

        {
            let flags = parse_sub_flags("i-2g").unwrap();
            assert_eq!(flags.first_match, -2);
            assert_eq!(flags.options, "i");
            assert!(flags.replace_all);
        }
    }

    #[test]
    fn test_parse_sub_flags_invalid() {
        assert_eq!(parse_sub_flags("1g59").unwrap_err(), SubFlagsError::SecondNumberGroup{ first_group: "1".to_owned(), index: 2 });
        assert_eq!(parse_sub_flags("1-23").unwrap_err(), SubFlagsError::MinusWithinNumber{ index: 1 });
        assert_eq!(parse_sub_flags("123q").unwrap_err(), SubFlagsError::UnknownFlag{ flag_char: 'q', index: 3 });
    }

    #[test]
    fn test_parse_with_ranges() {
        let empty_vec: Vec<char> = Vec::new();
        assert_eq!(parse_with_ranges("").unwrap(), empty_vec);
        assert_eq!(parse_with_ranges("abcde").unwrap(), vec!['a', 'b', 'c', 'd', 'e']);
        assert_eq!(parse_with_ranges("a-e").unwrap(), vec!['a', 'b', 'c', 'd', 'e']);
        assert_eq!(parse_with_ranges("a-cd-e").unwrap(), vec!['a', 'b', 'c', 'd', 'e']);
        assert_eq!(parse_with_ranges("a-dw-z").unwrap(), vec!['a', 'b', 'c', 'd', 'w', 'x', 'y', 'z']);
        assert_eq!(parse_with_ranges("w-za-d").unwrap(), vec!['w', 'x', 'y', 'z', 'a', 'b', 'c', 'd']);
        assert_eq!(parse_with_ranges("\\a\\-\\e").unwrap(), vec!['a', '-', 'e']);
        assert_eq!(parse_with_ranges("+--").unwrap(), vec!['+', ',', '-']);
    }

    #[test]
    fn test_parse_with_ranges_invalid() {
        assert_eq!(parse_with_ranges("a-de-a").unwrap_err(), ParserError::RangeInverted{ from: 'e', to: 'a' });
        assert_eq!(parse_with_ranges("!-\u{017F}").unwrap_err(), ParserError::RangeTooLarge{ from: '!', to: '\u{017F}', delta: 350, limit: 128 });
        assert_eq!(parse_with_ranges("a-d\\").unwrap_err(), ParserError::RangeTrailingEscape);
        assert_eq!(parse_with_ranges("\\a\\-\\d\\").unwrap_err(), ParserError::RangeTrailingEscape);
    }

    #[test]
    fn test_take_replacement_command() {
        {
            let (cmd, new_start) = take_replacement_command("s/one/two/g", 0).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "one");
            assert_eq!(cmd.new_string, "two");
            assert_eq!(cmd.flags.unwrap(), "g");
            assert_eq!(new_start, 11);
        }

        {
            let (cmd, new_start) = take_replacement_command("s/one/two", 0).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "one");
            assert_eq!(cmd.new_string, "two");
            assert_eq!(cmd.flags, None);
            assert_eq!(new_start, 9);
        }

        {
            let (cmd, new_start) = take_replacement_command("    s/one/two/", 0).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "one");
            assert_eq!(cmd.new_string, "two");
            assert_eq!(cmd.flags.unwrap(), "");
            assert_eq!(new_start, 14);
        }

        {
            let (cmd, new_start) = take_replacement_command("    s/one/two/    ", 0).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "one");
            assert_eq!(cmd.new_string, "two");
            assert_eq!(cmd.flags.unwrap(), "");
            assert_eq!(new_start, 14);
        }

        {
            let (cmd, new_start) = take_replacement_command("    s/one\\/two/three\\/four/g    ", 0).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "one\\/two");
            assert_eq!(cmd.new_string, "three\\/four");
            assert_eq!(cmd.flags.unwrap(), "g");
            assert_eq!(new_start, 28);
        }

        {
            let (cmd, new_start) = take_replacement_command("    s/one/two/ s/three/four/  ", 0).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "one");
            assert_eq!(cmd.new_string, "two");
            assert_eq!(cmd.flags.unwrap(), "");
            assert_eq!(new_start, 14);
        }
        // and, in succession,
        {
            let (cmd, new_start) = take_replacement_command("    s/one/two/ s/three/four/  ", 14).unwrap();
            assert_eq!(cmd.command, "s");
            assert_eq!(cmd.old_string, "three");
            assert_eq!(cmd.new_string, "four");
            assert_eq!(cmd.flags.unwrap(), "");
            assert_eq!(new_start, 28);
        }
    }

    #[test]
    fn test_take_replacement_command_invalid() {
        assert_eq!(take_replacement_command("s/one", 0).unwrap_err(), ParserError::IncompleteCommand);
        assert_eq!(
            take_replacement_command("xyz/one/two/three", 0).unwrap_err(),
            ParserError::UnknownCommand{ command: "xyz".to_string(), splitter_index: 3 },
        );
    }

    #[test]
    fn test_parse_replacement_commands() {
        {
            let cmds = parse_replacement_commands("s/one/two/ s/three/four/g").unwrap();
            assert_eq!(cmds.len(), 2);

            {
                let sub0 = match &cmds[0] {
                    SedCommand::Substitute(s) => s,
                    _ => panic!(),
                };
                assert_eq!(sub0.pattern().as_str(), "one");
                assert_eq!(sub0.replacement(), "two");
                assert_eq!(sub0.first_match(), 0);
                assert!(!sub0.replace_all());
            }

            {
                let sub1 = match &cmds[1] {
                    SedCommand::Substitute(s) => s,
                    _ => panic!(),
                };
                assert_eq!(sub1.pattern().as_str(), "three");
                assert_eq!(sub1.replacement(), "four");
                assert_eq!(sub1.first_match(), 0);
                assert!(sub1.replace_all());
            }
        }
    }

    #[test]
    fn test_parse_replacement_commands_failed() {
        assert_eq!(
            parse_replacement_commands("s/one/two/ only replaces the first occurrence").unwrap_err(),
            ParserError::NonCommandCharacter{ character: ' ', index: 15 },
        );
        assert_eq!(
            parse_replacement_commands("    s/one/two/ only replaces the first occurrence  ").unwrap_err(),
            ParserError::NonCommandCharacter{ character: ' ', index: 19 },
        );
    }

    #[test]
    fn test_transform_replacement_string() {
        assert_eq!(
            transform_replacement_string("four", 0).unwrap(),
            "four",
        );
        assert_eq!(
            transform_replacement_string("four", 2).unwrap(),
            "four",
        );
        assert_eq!(
            transform_replacement_string("four\\1", 2).unwrap(),
            "four${1}",
        );
        assert_eq!(
            transform_replacement_string("four\\g1;", 2).unwrap(),
            "four${1}",
        );
        assert_eq!(
            transform_replacement_string("four\\g12;three", 13).unwrap(),
            "four${12}three",
        );
        assert_eq!(
            transform_replacement_string("four\\g12;\\3", 13).unwrap(),
            "four${12}${3}",
        );
        assert_eq!(
            transform_replacement_string("four\\g12;$\\3", 13).unwrap(),
            "four${12}$$${3}",
        );
        assert_eq!(
            transform_replacement_string("four\\g12;$\\3\\g4;", 13).unwrap(),
            "four${12}$$${3}${4}",
        );
    }
}
