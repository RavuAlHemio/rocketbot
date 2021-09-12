use std::ops::Range;


#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct Token {
    pub value: String,
    pub orig_range: Range<usize>,
}
impl Token {
    fn new(
        value: String,
        orig_range: Range<usize>,
    ) -> Self {
        Self {
            value,
            orig_range,
        }
    }

    #[cfg(test)]
    fn new_str(
        value: &str,
        orig_range: Range<usize>,
    ) -> Self {
        Self::new(
            value.to_owned(),
            orig_range,
        )
    }
}

pub(crate) struct Tokenize<'a> {
    input_string: &'a str,
    current_index: usize,
    quote_char: Option<char>,
    escape_char: Option<char>,
}
impl<'a> Tokenize<'a> {
    fn new(
        input_string: &'a str,
        current_index: usize,
        quote_char: Option<char>,
        escape_char: Option<char>,
    ) -> Self {
        Self {
            input_string,
            current_index,
            quote_char,
            escape_char,
        }
    }
}
impl<'a> Tokenize<'a> {
    fn find_next(&self, start_index: usize, want_whitespace: bool) -> Option<usize> {
        let mut next_index: Option<usize> = None;
        let mut escape_at = None;
        let mut quoting = false;
        for (i, c) in self.input_string[start_index..].char_indices() {
            if let Some(ea) = escape_at {
                escape_at = None;
                if !want_whitespace {
                    next_index = Some(start_index + ea);
                    break;
                }
            } else if self.escape_char.map(|e| e == c).unwrap_or(false) {
                escape_at = Some(i);
            } else if self.quote_char.map(|q| q == c).unwrap_or(false) {
                quoting = !quoting;
                if quoting && !want_whitespace {
                    next_index = Some(start_index + i);
                    break;
                }
            } else if !quoting && c.is_whitespace() == want_whitespace {
                next_index = Some(start_index + i);
                break;
            }
        }
        next_index
    }

    fn unescape_unquote(&self, part: &str) -> String {
        let mut ret = String::with_capacity(part.len());
        let mut escaping = false;
        let mut quoting = false;
        for c in part.chars() {
            if escaping {
                ret.push(c);
                escaping = false;
            } else if self.escape_char.map(|e| e == c).unwrap_or(false) {
                escaping = true;
            } else if self.quote_char.map(|q| q == c).unwrap_or(false) {
                quoting = !quoting;
            } else {
                ret.push(c);
            }
        }
        if escaping {
            ret.push(self.escape_char.unwrap());
        }
        ret
    }
}
impl<'a> std::iter::Iterator for Tokenize<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.input_string.len() {
            return None;
        }

        // find the first index that is whitespace
        let first_whitespace_index_opt: Option<usize> = self.find_next(self.current_index, true);
        let first_whitespace_index = first_whitespace_index_opt
            .unwrap_or(self.input_string.len());

        // prepare the output slice
        let out_range = self.current_index..first_whitespace_index;
        let out_slice = &self.input_string[out_range.clone()];
        let unescaped_unquoted = self.unescape_unquote(out_slice);
        let out_chunk = Token::new(unescaped_unquoted, out_range);

        if first_whitespace_index_opt.is_none() {
            self.current_index = first_whitespace_index;
        } else {
            // there is whitespace to skip; do that
            let next_non_whitespace_index_opt: Option<usize> = self.find_next(first_whitespace_index, false);
            if let Some(nnwi) = next_non_whitespace_index_opt {
                self.current_index = nnwi;
            } else {
                // set to end
                self.current_index = self.input_string.len();
            }
        }

        Some(out_chunk)
    }
}

pub(crate) fn tokenize<'a>(string: &'a str) -> Tokenize<'a> {
    Tokenize::new(string, 0, Some('"'), Some('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_whitespace() {
        let mut iter = tokenize("foo");
        assert_eq!(Some(Token::new_str("foo", 0..3)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_simple_whitespace() {
        let mut iter = tokenize("the quick brown fox");
        assert_eq!(Some(Token::new_str("the", 0..3)), iter.next());
        assert_eq!(Some(Token::new_str("quick", 4..9)), iter.next());
        assert_eq!(Some(Token::new_str("brown", 10..15)), iter.next());
        assert_eq!(Some(Token::new_str("fox", 16..19)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_mixed_whitespace() {
        let mut iter = tokenize("the\tquick brown  \t  fox");
        assert_eq!(Some(Token::new_str("the", 0..3)), iter.next());
        assert_eq!(Some(Token::new_str("quick", 4..9)), iter.next());
        assert_eq!(Some(Token::new_str("brown", 10..15)), iter.next());
        assert_eq!(Some(Token::new_str("fox", 20..23)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_leading_whitespace() {
        let mut iter = tokenize("  the\tquick brown \t fox");
        assert_eq!(Some(Token::new_str("", 0..0)), iter.next());
        assert_eq!(Some(Token::new_str("the", 2..5)), iter.next());
        assert_eq!(Some(Token::new_str("quick", 6..11)), iter.next());
        assert_eq!(Some(Token::new_str("brown", 12..17)), iter.next());
        assert_eq!(Some(Token::new_str("fox", 20..23)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_trailing_whitespace() {
        let mut iter = tokenize("the\tquick brown \t fox  ");
        assert_eq!(Some(Token::new_str("the", 0..3)), iter.next());
        assert_eq!(Some(Token::new_str("quick", 4..9)), iter.next());
        assert_eq!(Some(Token::new_str("brown", 10..15)), iter.next());
        assert_eq!(Some(Token::new_str("fox", 18..21)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_leading_trailing_whitespace() {
        let mut iter = tokenize("  the\tquick brown \t fox  ");
        assert_eq!(Some(Token::new_str("", 0..0)), iter.next());
        assert_eq!(Some(Token::new_str("the", 2..5)), iter.next());
        assert_eq!(Some(Token::new_str("quick", 6..11)), iter.next());
        assert_eq!(Some(Token::new_str("brown", 12..17)), iter.next());
        assert_eq!(Some(Token::new_str("fox", 20..23)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_quote() {
        let mut iter = tokenize("  \"the\"\tquick \"brown \t fox\"  ");
        assert_eq!(Some(Token::new_str("", 0..0)), iter.next());
        assert_eq!(Some(Token::new_str("the", 2..7)), iter.next());
        assert_eq!(Some(Token::new_str("quick", 8..13)), iter.next());
        assert_eq!(Some(Token::new_str("brown \t fox", 14..27)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_escape() {
        let mut iter = tokenize("  the \\\"quick\\\" brown fox  ");
        assert_eq!(Some(Token::new_str("", 0..0)), iter.next());
        assert_eq!(Some(Token::new_str("the", 2..5)), iter.next());
        assert_eq!(Some(Token::new_str("\"quick\"", 6..15)), iter.next());
        assert_eq!(Some(Token::new_str("brown", 16..21)), iter.next());
        assert_eq!(Some(Token::new_str("fox", 22..25)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_quote_and_escape() {
        let mut iter = tokenize("  the \\\"quick\\\" \"brown \t\\\\ \\\"fox\\\" \"  ");
        assert_eq!(Some(Token::new_str("", 0..0)), iter.next());
        assert_eq!(Some(Token::new_str("the", 2..5)), iter.next());
        assert_eq!(Some(Token::new_str("\"quick\"", 6..15)), iter.next());
        assert_eq!(Some(Token::new_str("brown \t\\ \"fox\" ", 16..36)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }
}
