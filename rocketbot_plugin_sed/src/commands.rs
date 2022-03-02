use std::collections::HashMap;
use std::convert::TryInto;

use fancy_regex::{Captures, Regex};
use log::warn;


pub trait Transformer {
    fn transform(&self, text: &str) -> String;
}


#[derive(Debug)]
pub(crate) enum SedCommand {
    Substitute(SubstituteCommand),
    Transpose(TransposeCommand),
}
impl Transformer for SedCommand {
    fn transform(&self, text: &str) -> String {
        match self {
            SedCommand::Substitute(sc) => sc.transform(text),
            SedCommand::Transpose(tc) => tc.transform(text),
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
