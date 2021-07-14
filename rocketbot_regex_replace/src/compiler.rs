use std::fmt;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::placeholders::Placeholder;


static CASING_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^\\$case\\$(?P<template_group>[^\\$]+)\\$(?P<string_to_case>.+)$",
).expect("failed to compile casing regex"));
static LOOKUP_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^\\$lookup\\$(?P<key>.+)$"
).expect("failed to compile lookup regex"));
static SHORTEN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^\\$shorten\\$(?P<key>.+)\\$(?P<len>0|[1-9][0-9]*)$"
).expect("failed to compile shorten regex"));


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum State {
    Text,
    AfterDollar,
    DollarBrace,
    DollarNumber,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CompilationError {
    UnexpectedCharacterAfterDollar(char),
    UnterminatedDollarBraceExpression,
    TrailingDollarCharacter,
    CaseUnknownCapturingGroup(String),
    UnknownCapturingGroup(String),
    ShortenTooLong,
}
impl fmt::Display for CompilationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompilationError::UnexpectedCharacterAfterDollar(c)
                => write!(f, "unexpected character '{}' after '$' character", c),
            CompilationError::UnterminatedDollarBraceExpression
                => write!(f, "unterminated {} expression", "${...}"),
            CompilationError::TrailingDollarCharacter
                => write!(f, "trailing $ character"),
            CompilationError::CaseUnknownCapturingGroup(s)
                => write!(f, "case match references unknown capturing group named {:?}", s),
            CompilationError::UnknownCapturingGroup(s)
                => write!(f, "unknown capturing group named {:?}", s),
            CompilationError::ShortenTooLong
                => write!(f, "shortening length too long"),
        }
    }
}
impl std::error::Error for CompilationError {
}


pub(crate) fn compile(regex: &Regex, replacement_string: &str) -> Result<Vec<Placeholder>, CompilationError> {
    let mut state = State::Text;
    let mut sb = String::new();
    let mut placeholders: Vec<Placeholder> = Vec::new();

    for c in replacement_string.chars() {
        match state {
            State::Text => {
                match c {
                    '$' => {
                        if sb.len() > 0 {
                            placeholders.push(Placeholder::ConstantString(sb.clone()));
                            sb.clear();
                        }
                        state = State::AfterDollar;
                    },
                    other => {
                        sb.push(other);
                    },
                }
            },
            State::AfterDollar => {
                match c {
                    '$' => {
                        sb.push(c);
                        state = State::Text;
                    },
                    '{' => {
                        state = State::DollarBrace;
                    },
                    '0'..='9' => {
                        sb.push(c);
                        state = State::DollarNumber;
                    },
                    other => {
                        return Err(CompilationError::UnexpectedCharacterAfterDollar(other));
                    },
                }
            },
            State::DollarBrace => {
                match c {
                    '}' => {
                        process_named_group(&sb, regex, &mut placeholders)?;
                        sb.clear();
                        state = State::Text;
                    },
                    other => {
                        sb.push(other);
                    },
                }
            },
            State::DollarNumber => {
                match c {
                    '0'..='9' => {
                        sb.push(c);
                    },
                    other => {
                        process_number_group(&sb, regex, &mut placeholders)?;
                        sb.clear();
                        if other == '$' {
                            state = State::AfterDollar;
                        } else {
                            sb.push(other);
                            state = State::Text;
                        }
                    },
                }
            },
        }
    }

    match state {
        State::Text => {
            if sb.len() > 0 {
                placeholders.push(Placeholder::ConstantString(sb));
            }
        },
        State::DollarNumber => {
            process_number_group(&sb, regex, &mut placeholders)?;
        },
        State::DollarBrace => {
            return Err(CompilationError::UnterminatedDollarBraceExpression);
        },
        State::AfterDollar => {
            return Err(CompilationError::TrailingDollarCharacter);
        },
    }

    Ok(optimize(&placeholders))
}

fn process_named_group(group_name: &str, regex: &Regex, placeholders: &mut Vec<Placeholder>) -> Result<(), CompilationError> {
    if let Some(casing_match) = CASING_REGEX.captures(group_name) {
        let template_group_name = casing_match.name("template_group").unwrap().as_str();
        let string_to_case = casing_match.name("string_to_case").unwrap().as_str();

        let any_such_named_capture = regex
            .capture_names()
            .filter_map(|cn| cn)
            .any(|cn| cn == template_group_name);
        if any_such_named_capture {
            placeholders.push(Placeholder::CasingNamedMatchGroup(
                string_to_case.to_owned(),
                template_group_name.to_owned(),
            ));
            return Ok(());
        }

        // try parsing as a number
        if let Ok(u) = template_group_name.parse::<usize>() {
            if u < regex.captures_len() {
                placeholders.push(Placeholder::CasingNumberedMatchGroup(
                    string_to_case.to_owned(),
                    u,
                ));
                return Ok(());
            }
        }

        return Err(CompilationError::CaseUnknownCapturingGroup(template_group_name.to_owned()));
    }

    if let Some(lookup_match) = LOOKUP_REGEX.captures(group_name) {
        let key = lookup_match.name("key").unwrap().as_str();

        placeholders.push(Placeholder::Lookup(
            key.to_owned(),
        ));
        return Ok(());
    }

    if let Some(lookup_match) = SHORTEN_REGEX.captures(group_name) {
        let group_name = lookup_match.name("key").unwrap().as_str();
        let len: usize = match lookup_match.name("len").unwrap().as_str().parse() {
            Ok(u) => u,
            Err(e) => return Err(CompilationError::ShortenTooLong),
        };

        let any_such_named_capture = regex
            .capture_names()
            .filter_map(|cn| cn)
            .any(|cn| cn == group_name);
        if !any_such_named_capture {
            return Err(CompilationError::UnknownCapturingGroup(group_name.to_owned()));
        }

        placeholders.push(Placeholder::Shorten(
            group_name.to_owned(),
            len,
        ));
        return Ok(());
    }

    let any_such_named_capture = regex
        .capture_names()
        .filter_map(|cn| cn)
        .any(|cn| cn == group_name);
    if any_such_named_capture {
        placeholders.push(Placeholder::NamedMatchGroup(
            group_name.to_owned(),
        ));
        return Ok(());
    }

    // try parsing as a number
    if let Ok(u) = group_name.parse::<usize>() {
        if u < regex.captures_len() {
            placeholders.push(Placeholder::NumberedMatchGroup(u));
            return Ok(());
        }
    }

    Err(CompilationError::UnknownCapturingGroup(group_name.to_owned()))
}

fn process_number_group(group_name: &str, regex: &Regex, placeholders: &mut Vec<Placeholder>) -> Result<(), CompilationError> {
    let any_such_named_capture = regex
        .capture_names()
        .filter_map(|cn| cn)
        .any(|cn| cn == group_name);
    if any_such_named_capture {
        placeholders.push(Placeholder::NamedMatchGroup(
            group_name.to_owned(),
        ));
        return Ok(());
    }

    // try parsing as a number
    if let Ok(u) = group_name.parse::<usize>() {
        if u < regex.captures_len() {
            placeholders.push(Placeholder::NumberedMatchGroup(u));
            return Ok(());
        }
    }

    Err(CompilationError::UnknownCapturingGroup(group_name.to_owned()))
}

fn optimize(placeholders: &Vec<Placeholder>) -> Vec<Placeholder> {
    let mut sb = String::new();
    let mut ret = Vec::new();

    for placeholder in placeholders {
        if let Placeholder::ConstantString(s) = placeholder {
            sb.push_str(s);
        } else {
            if sb.len() > 0 {
                ret.push(Placeholder::ConstantString(sb.clone()));
                sb.clear();
            }
            ret.push(placeholder.clone());
        }
    }

    if sb.len() > 0 {
        ret.push(Placeholder::ConstantString(sb));
    }

    ret
}
