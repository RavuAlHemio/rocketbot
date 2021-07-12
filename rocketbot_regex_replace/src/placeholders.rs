use std::collections::HashMap;

use regex::{Captures, Regex};


#[derive(Debug)]
pub(crate) struct ReplacementState<'a> {
    input_string: &'a str,
    regex: &'a Regex,
    regex_match: &'a Captures<'a>,
    lookups: &'a HashMap<String, String>,
}
impl<'a> ReplacementState<'a> {
    pub fn new(
        input_string: &'a str,
        regex: &'a Regex,
        regex_match: &'a Captures<'a>,
        lookups: &'a HashMap<String, String>,
    ) -> ReplacementState<'a> {
        ReplacementState {
            input_string,
            regex,
            regex_match,
            lookups,
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Placeholder {
    ConstantString(String),
    EntireInputString,
    EntireMatch,
    TextBeforeMatch,
    TextAfterMatch,
    Lookup(String),
    NamedMatchGroup(String),
    NumberedMatchGroup(usize),
    LastMatchGroup,
    CasingNamedMatchGroup(String, String),
    CasingNumberedMatchGroup(String, usize),
}
impl Placeholder {
    pub fn replace(&self, state: &ReplacementState) -> String {
        match self {
            Placeholder::ConstantString(s)
                => s.clone(),
            Placeholder::EntireInputString
                => state.input_string.to_owned(),
            Placeholder::EntireMatch
                => state.regex_match.get(0).unwrap().as_str().to_owned(),
            Placeholder::TextBeforeMatch
                => state.input_string[0..state.regex_match.get(0).unwrap().start()].to_owned(),
            Placeholder::TextAfterMatch
                => state.input_string[state.regex_match.get(0).unwrap().end()..].to_owned(),
            Placeholder::Lookup(key)
                => state.lookups[key].clone(),
            Placeholder::NamedMatchGroup(name)
                => state.regex_match.name(name).unwrap().as_str().to_owned(),
            Placeholder::NumberedMatchGroup(number)
                => state.regex_match.get(*number).unwrap().as_str().to_owned(),
            Placeholder::LastMatchGroup
                => state.regex_match.get(state.regex_match.len()-1).unwrap().as_str().to_owned(),
            Placeholder::CasingNamedMatchGroup(string_to_case, case_template_group)
                => case_string_named(string_to_case, case_template_group, &state),
            Placeholder::CasingNumberedMatchGroup(string_to_case, case_template_group)
                => case_string_numbered(string_to_case, *case_template_group, &state),
        }
    }
}

fn case_string_named(string_to_case: &str, case_template_group: &str, state: &ReplacementState) -> String {
    let case_template = state.regex_match.name(case_template_group).unwrap().as_str();
    case_string(string_to_case, case_template, state)
}

fn case_string_numbered(string_to_case: &str, case_template_group: usize, state: &ReplacementState) -> String {
    let case_template = state.regex_match.get(case_template_group).unwrap().as_str();
    case_string(string_to_case, case_template, state)
}

fn case_string(string_to_case: &str, case_template: &str, state: &ReplacementState) -> String {
    if string_to_case.len() == 0 {
        return String::new();
    }

    if case_template.len() == string_to_case.len() {
        one_to_one_case(string_to_case, case_template)
    } else if case_template.len() == 0 {
        string_to_case.to_owned()
    } else if case_template.len() == 1 {
        if case_template.chars().nth(0).unwrap().is_uppercase() {
            string_to_case.to_uppercase()
        } else {
            string_to_case.to_lowercase()
        }
    } else {
        best_guess_case(string_to_case, case_template)
    }
}

fn one_to_one_case(string_to_case: &str, case_template: &str) -> String {
    let chars_to_case: Vec<char> = string_to_case.chars().collect();
    let case_template_chars: Vec<char> = case_template.chars().collect();

    assert_eq!(chars_to_case.len(), case_template_chars.len());

    let mut ret = String::with_capacity(string_to_case.len());
    for (cc, tc) in chars_to_case.iter().zip(case_template_chars.iter()) {
        if tc.is_uppercase() {
            for u in cc.to_uppercase() {
                ret.push(u);
            }
        } else if tc.is_lowercase() {
            for u in cc.to_lowercase() {
                ret.push(u);
            }
        } else {
            ret.push(*cc);
        }
    }
    ret
}

fn best_guess_case(string_to_case: &str, case_template: &str) -> String {
    let chars_to_case: Vec<char> = string_to_case.chars().collect();
    let case_template_chars: Vec<char> = case_template.chars().collect();

    assert!(case_template_chars.len() > 1);
    assert!(chars_to_case.len() > 0);

    let first_upper = case_template_chars[0].is_uppercase();
    let first_lower = case_template_chars[0].is_lowercase();
    let second_upper = case_template_chars[1].is_uppercase();
    let second_lower = case_template_chars[1].is_lowercase();

    let rest_chars_to_case: String = chars_to_case.iter()
        .skip(1)
        .collect();

    if first_upper && second_upper {
        // AA
        string_to_case.to_uppercase()
    } else if first_upper && second_lower {
        // Aa
        format!(
            "{}{}",
            chars_to_case[0].to_uppercase(),
            rest_chars_to_case.to_lowercase(),
        )
    } else if first_lower && second_upper {
        // aA
        format!(
            "{}{}",
            chars_to_case[0].to_lowercase(),
            rest_chars_to_case.to_uppercase(),
        )
    } else if first_lower && second_lower {
        // aa
        string_to_case.to_lowercase()
    } else {
        // 0a, 0A, a0, A0
        string_to_case.to_owned()
    }
}
