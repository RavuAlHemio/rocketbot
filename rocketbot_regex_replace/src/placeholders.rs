use std::collections::HashMap;

use regex::{Captures, Regex};


#[derive(Clone, Debug)]
pub(crate) struct ReplacementState<'a> {
    input_string: &'a str,
    #[allow(unused)] regex: &'a Regex,
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
    #[allow(unused)] EntireInputString,
    #[allow(unused)] EntireMatch,
    #[allow(unused)] TextBeforeMatch,
    #[allow(unused)] TextAfterMatch,
    Lookup(String),
    NamedMatchGroup(String),
    NumberedMatchGroup(usize),
    #[allow(unused)] LastMatchGroup,
    CasingNamedMatchGroup(String, String),
    CasingNumberedMatchGroup(String, usize),
    Shorten(String, usize),
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
            Placeholder::Shorten(group_name, length)
                => shorten(group_name, *length, &state),
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

fn case_string(string_to_case: &str, case_template: &str, _state: &ReplacementState) -> String {
    let chars_to_case: Vec<char> = string_to_case.chars().collect();
    let case_template_chars: Vec<char> = case_template.chars().collect();

    case_chars(&chars_to_case, &case_template_chars)
}

fn case_chars(chars_to_case: &[char], case_template_chars: &[char]) -> String {
    if chars_to_case.len() == 0 {
        return String::new();
    }

    if case_template_chars.len() == chars_to_case.len() {
        one_to_one_case(chars_to_case, case_template_chars)
    } else if case_template_chars.len() == 0 {
        chars_to_case.iter().collect()
    } else if case_template_chars.len() == 1 {
        if case_template_chars[0].is_uppercase() {
            let s: String = chars_to_case.iter().collect();
            s.to_uppercase()
        } else {
            let s: String = chars_to_case.iter().collect();
            s.to_lowercase()
        }
    } else {
        best_guess_case(chars_to_case, case_template_chars)
    }
}

fn one_to_one_case(chars_to_case: &[char], case_template_chars: &[char]) -> String {
    assert_eq!(chars_to_case.len(), case_template_chars.len());

    let mut ret = String::with_capacity(chars_to_case.len());
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

fn best_guess_case(chars_to_case: &[char], case_template_chars: &[char]) -> String {
    if case_template_chars.len() < 2 {
        return chars_to_case.iter().collect();
    }

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
        let s: String = chars_to_case.iter().collect();
        s.to_uppercase()
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
        let s: String = chars_to_case.iter().collect();
        s.to_lowercase()
    } else {
        // 0a, 0A, a0, A0
        chars_to_case.iter().collect()
    }
}

fn shorten(group_name: &str, length: usize, state: &ReplacementState) -> String {
    let match_str = state.regex_match.name(group_name).unwrap().as_str();
    match_str.chars().take(length).collect()
}
