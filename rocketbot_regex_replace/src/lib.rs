pub mod compiler;
mod placeholders;


use std::collections::HashMap;

use regex::{Captures, Match, Regex};

use crate::compiler::{CompilationError, compile};
use crate::placeholders::{Placeholder, ReplacementState};


#[derive(Debug)]
pub struct ReplacerRegex {
    regex: Regex,
    replacement_string: String,
    placeholders: Vec<Placeholder>,
}
impl ReplacerRegex {
    pub(crate) fn new(
        regex: Regex,
        replacement_string: String,
        placeholders: Vec<Placeholder>,
    ) -> ReplacerRegex {
        ReplacerRegex {
            regex,
            replacement_string,
            placeholders,
        }
    }

    pub fn compile_new(
        regex: Regex,
        replacement_string: String,
    ) -> Result<ReplacerRegex, CompilationError> {
        let placeholders = compile(&regex, &replacement_string)?;
        Ok(ReplacerRegex::new(
            regex,
            replacement_string,
            placeholders,
        ))
    }

    pub fn find_at<'a>(&self, text: &'a str, start: usize) -> Option<Match<'a>> {
        self.regex.find_at(text, start)
    }

    pub fn replace(
        &self,
        input_string: &str,
        lookups: &HashMap<String, String>,
    ) -> String {
        self.regex.replace_all(
            input_string,
            |caps: &Captures| self.replace_match(input_string, caps, lookups)
        ).into_owned()
    }

    fn replace_match(
        &self,
        input_string: &str,
        caps: &Captures,
        lookups: &HashMap<String, String>,
    ) -> String {
        let state = ReplacementState::new(
            input_string,
            &self.regex,
            caps,
            lookups,
        );

        let replaced_bits: Vec<String> = self.placeholders
            .iter()
            .map(|p| p.replace(&state))
            .collect();
        replaced_bits.concat()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn test_replacement(
        expected: &str,
        regex_str: &str,
        replacement: &str,
        subject: &str,
        lookups: Option<HashMap<String, String>>,
    ) {
        let my_lookups = lookups.unwrap_or_else(|| HashMap::new());
        let regex = Regex::new(regex_str).unwrap();
        let replacer_regex = ReplacerRegex::compile_new(
            regex,
            replacement.to_owned(),
        ).unwrap();
        let replaced = replacer_regex.replace(subject, &my_lookups);
        assert_eq!(expected, replaced);
    }

    fn test_replacement_error(regex_str: &str, replacement: &str) -> CompilationError {
        let regex = Regex::new(regex_str).unwrap();
        ReplacerRegex::compile_new(
            regex,
            replacement.to_owned(),
        ).unwrap_err()
    }

    fn test_replacement_unknown_capturing_group(regex_str: &str, replacement: &str) {
        let err = test_replacement_error(regex_str, replacement);
        if let CompilationError::UnknownCapturingGroup(_) = err {
            // OK
        } else {
            panic!("wrong error");
        }
    }

    #[test]
    fn simple_replacement() {
        test_replacement(
            "aaqqccqqddqqeeqq",
            "b",
            "q",
            "aabbccbbddbbeebb",
            None,
        );
    }

    #[test]
    fn simple_longer_replacement() {
        test_replacement(
            "aapqpqccpqpqddpqpqeepqpq",
            "b",
            "pq",
            "aabbccbbddbbeebb",
            None,
        );
    }

    #[test]
    fn dollar_sign_replacement() {
        test_replacement(
            "aap$qp$qccp$qp$qddp$qp$qeep$qp$q",
            "b",
            "p$$q",
            "aabbccbbddbbeebb",
            None,
        );
    }

    #[test]
    fn numerical_group_reference_replacement() {
        test_replacement("pbqprqrpkqf", "a(.)c", "p$1q", "abcarcrakcf", None);
        test_replacement("bprprkpf", "a(.)c", "$1p", "abcarcrakcf", None);
        test_replacement("pbprrpkf", "a(.)c", "p$1", "abcarcrakcf", None);
        test_replacement("pbbqprrqrpkkqf", "a(.)c", "p$1$1q", "abcarcrakcf", None);
    }

    #[test]
    fn textual_group_reference_replacement() {
        test_replacement("pbqprqrpkqf", "a(?P<abc>.)c", "p${abc}q", "abcarcrakcf", None);
        test_replacement("bprprkpf", "a(?P<abc>.)c", "${abc}p", "abcarcrakcf", None);
        test_replacement("pbprrpkf", "a(?P<abc>.)c", "p${abc}", "abcarcrakcf", None);
        test_replacement("pbbqprrqrpkkqf", "a(?P<abc>.)c", "p${abc}${abc}q", "abcarcrakcf", None);
    }

    #[test]
    fn mixed_reference_replacement() {
        test_replacement("pbbqprrqrpkkqf", "a(?P<abc>.)c", "p$1${abc}q", "abcarcrakcf", None);
        test_replacement("pbbbqprrrqrpkkkqf", "a(?P<abc>.)c", "p$1${abc}${1}q", "abcarcrakcf", None);
    }

    #[test]
    fn case_replacement() {
        test_replacement("prqprqrprqf", "a(.)c", "p${$case$1$r}q", "abcarcrakcf", None);
        test_replacement("pMqpmqrpmqf", "(?i)a(.)c", "p${$case$1$m}q", "aBcArCrAkCf", None);

        test_replacement("what is dis", "(?i)(th)(is)", "${$case$1$d}$2", "what is this", None);
        test_replacement("WHAT IS DIS", "(?i)(th)(is)", "${$case$1$d}$2", "WHAT IS THIS", None);
        test_replacement("What Is Dis", "(?i)(th)(is)", "${$case$1$d}$2", "What Is This", None);
        test_replacement("wHAT iS dIS", "(?i)(th)(is)", "${$case$1$d}$2", "wHAT iS tHIS", None);

        test_replacement("what is dis", "(?i)(?P<th>th)(?P<is>is)", "${$case$th$d}${is}", "what is this", None);
        test_replacement("WHAT IS DIS", "(?i)(?P<th>th)(?P<is>is)", "${$case$th$d}${is}", "WHAT IS THIS", None);
        test_replacement("What Is Dis", "(?i)(?P<th>th)(?P<is>is)", "${$case$th$d}${is}", "What Is This", None);
        test_replacement("wHAT iS dIS", "(?i)(?P<th>th)(?P<is>is)", "${$case$th$d}${is}", "wHAT iS tHIS", None);

        test_replacement("kiwifruit", "(?i)(?P<kiw>kiw)(?P<i>i)", "${kiw}${$case$i$ifruit}", "kiwi", None);
        test_replacement("KIWIFRUIT", "(?i)(?P<kiw>kiw)(?P<i>i)", "${kiw}${$case$i$ifruit}", "KIWI", None);
        test_replacement("KiWifruit", "(?i)(?P<kiw>kiw)(?P<i>i)", "${kiw}${$case$i$ifruit}", "KiWi", None);
        test_replacement("kIwIFRUIT", "(?i)(?P<kiw>kiw)(?P<i>i)", "${kiw}${$case$i$ifruit}", "kIwI", None);
        test_replacement("KIWifruit", "(?i)(?P<kiw>kiw)(?P<i>i)", "${kiw}${$case$i$ifruit}", "KIWi", None);
        test_replacement("kiwIFRUIT", "(?i)(?P<kiw>kiw)(?P<i>i)", "${kiw}${$case$i$ifruit}", "kiwI", None);
    }

    #[test]
    fn lookup_replacement() {
        let mut lookups = HashMap::new();
        lookups.insert("username".to_owned(), "WHAT".to_owned());
        test_replacement("apWHATqpWHATqc", "b", "p${$lookup$username}q", "abbc", Some(lookups));
    }

    #[test]
    fn trailing_dollar_sign_error()
    {
        let err = test_replacement_error("abc", "pq$");
        if let CompilationError::TrailingDollarCharacter = err {
            // OK
        } else {
            panic!("wrong error: {}", err);
        }
    }

    #[test]
    fn unclosed_dollar_brace_error()
    {
        let err = test_replacement_error("abc", "p${q");
        if let CompilationError::UnterminatedDollarBraceExpression = err {
            // OK
        } else {
            panic!("wrong error: {}", err);
        }
    }

    #[test]
    fn match_group_index_out_of_range_error()
    {
        test_replacement_unknown_capturing_group("abc", "p$1q");
        test_replacement_unknown_capturing_group("abc", "p$1q");
        test_replacement_unknown_capturing_group("abc", "p${abc}q");
        test_replacement_unknown_capturing_group("a(b)c", "p$1q$2");
        test_replacement_unknown_capturing_group("a(<abc>b)c", "p${abc}q${def}");
    }
}
