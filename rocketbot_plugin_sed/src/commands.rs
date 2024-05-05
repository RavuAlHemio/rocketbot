use std::collections::HashMap;
use std::ops::Range;

use fancy_regex::{Captures, Regex};
use tracing::warn;


pub trait Transformer {
    fn transform(&self, text: &str) -> String;
}


#[derive(Debug)]
pub(crate) enum SedCommand {
    Substitute(SubstituteCommand),
    Transpose(TransposeCommand),
    Exchange(ExchangeCommand),
}
impl Transformer for SedCommand {
    fn transform(&self, text: &str) -> String {
        match self {
            SedCommand::Substitute(sc) => sc.transform(text),
            SedCommand::Transpose(tc) => tc.transform(text),
            SedCommand::Exchange(ec) => ec.transform(text),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SubstituteCommand {
    pattern: Regex,
    replacement: String,
    first_match: isize,
    replace_all: bool,
}
impl SubstituteCommand {
    pub fn new(
        pattern: Regex,
        replacement: String,
        first_match: isize,
        replace_all: bool,
    ) -> SubstituteCommand {
        SubstituteCommand {
            pattern,
            replacement,
            first_match,
            replace_all,
        }
    }

    #[allow(unused)] pub fn pattern(&self) -> &Regex { &self.pattern }
    #[allow(unused)] pub fn replacement(&self) -> &str { self.replacement.as_str() }
    #[allow(unused)] pub fn first_match(&self) -> isize { self.first_match }
    #[allow(unused)] pub fn replace_all(&self) -> bool { self.replace_all }
}
impl Transformer for SubstituteCommand {
    fn transform(&self, text: &str) -> String {
        let first_match: isize = if self.first_match < 0 {
            // match from end => we must count the matches first
            let match_count_usize = self.pattern.find_iter(text).count();
            let match_count: isize = match match_count_usize.try_into() {
                Ok(mc) => mc,
                Err(_) => {
                    // give up
                    warn!("failed to convert match count {} from usize to isize", match_count_usize);
                    return text.to_owned();
                },
            };

            if match_count + self.first_match < 0 {
                // give up
                warn!(
                    "match_count ({}) plus first_match ({}) are less than 0 ({})",
                    match_count, self.first_match, match_count + self.first_match,
                );
                return text.to_owned();
            }

            match_count + self.first_match
        } else {
            self.first_match
        };

        let mut match_index: isize = -1;
        let replaced = self.pattern.replace_all(text, |caps: &Captures| {
            match_index += 1;

            if match_index < first_match {
                // unchanged
                caps.get(0).expect("failed to get full match")
                    .as_str().to_owned()
            } else if match_index > first_match && !self.replace_all {
                // unchanged
                caps.get(0).expect("failed to get full match")
                    .as_str().to_owned()
            } else {
                let mut ret = String::new();
                caps.expand(&self.replacement, &mut ret);
                ret
            }
        });

        replaced.into_owned()
    }
}

#[derive(Debug)]
pub(crate) struct TransposeCommand {
    transposition_dictionary: HashMap<char, Option<char>>,
}
impl TransposeCommand {
    pub fn new(
        transposition_dictionary: HashMap<char, Option<char>>,
    ) -> TransposeCommand {
        TransposeCommand {
            transposition_dictionary,
        }
    }
}
impl Transformer for TransposeCommand {
    fn transform(&self, text: &str) -> String {
        let mut ret = String::with_capacity(text.len());
        for c in text.chars() {
            match self.transposition_dictionary.get(&c) {
                Some(Some(r)) => {
                    // it is in the dictionary and calls for a replacement
                    ret.push(*r);
                },
                Some(None) => {
                    // it is in the dictionary and calls to be dropped
                },
                None => {
                    // it is not in the dictionary; take it unchanged
                    ret.push(c);
                }
            }
        }
        ret
    }
}

#[derive(Debug)]
pub(crate) struct ExchangeCommand {
    from_regex: Regex,
    to_regex: Regex,
}
impl ExchangeCommand {
    pub fn new(
        from_regex: Regex,
        to_regex: Regex,
    ) -> Self {
        Self {
            from_regex,
            to_regex,
        }
    }
}
impl Transformer for ExchangeCommand {
    fn transform(&self, text: &str) -> String {
        let from_match_opt = self.from_regex
            .find(text).expect("from_regex.find failed");
        let from_match = match from_match_opt {
            Some(fm) => fm,
            None => return text.to_owned(),
        };
        let mut to_match_opt = None;
        for match_res in self.to_regex.find_iter(text) {
            let m = match_res.expect("to_regex.find failed");
            if !ranges_overlap(&from_match.range(), &m.range()) {
                to_match_opt = Some(m);
                break;
            }
        }
        let to_match = match to_match_opt {
            Some(tm) => tm,
            None => return text.to_owned(),
        };

        let mut ret = String::with_capacity(text.len());
        if from_match.start() < to_match.start() {
            ret.push_str(&text[..from_match.start()]);
            ret.push_str(to_match.as_str());
            ret.push_str(&text[from_match.end()..to_match.start()]);
            ret.push_str(from_match.as_str());
            ret.push_str(&text[to_match.end()..]);
        } else {
            assert!(to_match.start() < from_match.start());
            ret.push_str(&text[..to_match.start()]);
            ret.push_str(from_match.as_str());
            ret.push_str(&text[to_match.end()..from_match.start()]);
            ret.push_str(to_match.as_str());
            ret.push_str(&text[from_match.end()..]);
        }
        ret
    }
}


fn ranges_overlap<T: PartialOrd>(one: &Range<T>, other: &Range<T>) -> bool {
    if one.is_empty() || other.is_empty() {
        return false;
    }

    !(
        one.end <= other.start
        || other.end <= one.start
    )
}


#[cfg(test)]
mod tests {
    use super::{ExchangeCommand, Transformer};
    use fancy_regex::Regex;

    fn tec1(from_regex_str: &str, to_regex_str: &str, subject: &str, expected: &str) {
        let from_regex = Regex::new(from_regex_str).unwrap();
        let to_regex = Regex::new(to_regex_str).unwrap();
        let cmd = ExchangeCommand::new(from_regex.clone(), to_regex.clone());
        let transformed = cmd.transform(subject);
        assert_eq!(expected, transformed.as_str());
    }

    fn tec(from_regex_str: &str, to_regex_str: &str, subject: &str, expected: &str) {
        let from_regex = Regex::new(from_regex_str).unwrap();
        let to_regex = Regex::new(to_regex_str).unwrap();
        let cmd = ExchangeCommand::new(from_regex.clone(), to_regex.clone());
        let transformed = cmd.transform(subject);
        assert_eq!(expected, transformed.as_str());

        // also try it the other way around
        let cmd2 = ExchangeCommand::new(to_regex.clone(), from_regex.clone());
        let transformed2 = cmd2.transform(subject);
        assert_eq!(expected, transformed2.as_str());
    }

    #[test]
    fn test_exchange_command() {
        tec(
            "fox", "dog",
            "the quick brown fox jumps over the lazy dog",
            "the quick brown dog jumps over the lazy fox",
        );

        tec(
            "two", "three",
            "onetwothreefour",
            "onethreetwofour",
        );

        // when overlapping, results may differ
        tec1(
            "overlap", "lap",
            "do not exchange overlapping parts as overlap breeds confusion",
            "do not exchange lapping parts as overoverlap breeds confusion",
        );
        tec1(
            "lap", "overlap",
            "do not exchange overlapping parts as overlap breeds confusion",
            "do not exchange overoverlapping parts as lap breeds confusion",
        );
    }
}
