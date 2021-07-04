#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct SplitChunk<'a> {
    pub chunk: &'a str,
    pub orig_index: usize,
}
impl<'a> SplitChunk<'a> {
    fn new(
        chunk: &'a str,
        orig_index: usize,
    ) -> SplitChunk {
        SplitChunk {
            chunk,
            orig_index,
        }
    }
}

pub(crate) struct SplitWhitespace<'a> {
    input_string: &'a str,
    current_index: usize,
}
impl<'a> SplitWhitespace<'a> {
    fn new(
        input_string: &'a str,
        current_index: usize,
    ) -> SplitWhitespace {
        SplitWhitespace {
            input_string,
            current_index,
        }
    }
}
impl<'a> std::iter::Iterator for SplitWhitespace<'a> {
    type Item = SplitChunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.input_string.len() {
            return None;
        }

        // find the first index that is whitespace
        let mut first_whitespace_index_opt: Option<usize> = None;
        for (i, c) in self.input_string[self.current_index..].char_indices() {
            if c.is_whitespace() {
                first_whitespace_index_opt = Some(self.current_index + i);
                break;
            }
        }
        let first_whitespace_index = first_whitespace_index_opt
            .unwrap_or(self.input_string.len());

        // prepare the output slice
        let out_slice = &self.input_string[self.current_index..first_whitespace_index];
        let out_chunk = SplitChunk::new(out_slice, self.current_index);

        if first_whitespace_index_opt.is_none() {
            self.current_index = first_whitespace_index;
        } else {
            // there is whitespace to skip; do that
            let mut next_non_whitespace_index_opt: Option<usize> = None;
            for (i, c) in self.input_string[first_whitespace_index..].char_indices() {
                if !c.is_whitespace() {
                    next_non_whitespace_index_opt = Some(first_whitespace_index + i);
                    break;
                }
            }

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

pub(crate) fn split_whitespace<'a>(string: &'a str) -> SplitWhitespace<'a> {
    SplitWhitespace {
        input_string: string,
        current_index: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_whitespace() {
        let mut iter = split_whitespace("foo");
        assert_eq!(Some(SplitChunk::new("foo", 0)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_simple_whitespace() {
        let mut iter = split_whitespace("the quick brown fox");
        assert_eq!(Some(SplitChunk::new("the", 0)), iter.next());
        assert_eq!(Some(SplitChunk::new("quick", 4)), iter.next());
        assert_eq!(Some(SplitChunk::new("brown", 10)), iter.next());
        assert_eq!(Some(SplitChunk::new("fox", 16)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_mixed_whitespace() {
        let mut iter = split_whitespace("the\tquick brown  \t  fox");
        assert_eq!(Some(SplitChunk::new("the", 0)), iter.next());
        assert_eq!(Some(SplitChunk::new("quick", 4)), iter.next());
        assert_eq!(Some(SplitChunk::new("brown", 10)), iter.next());
        assert_eq!(Some(SplitChunk::new("fox", 20)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_leading_whitespace() {
        let mut iter = split_whitespace("  the\tquick brown \t fox");
        assert_eq!(Some(SplitChunk::new("", 0)), iter.next());
        assert_eq!(Some(SplitChunk::new("the", 2)), iter.next());
        assert_eq!(Some(SplitChunk::new("quick", 6)), iter.next());
        assert_eq!(Some(SplitChunk::new("brown", 12)), iter.next());
        assert_eq!(Some(SplitChunk::new("fox", 20)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_trailing_whitespace() {
        let mut iter = split_whitespace("the\tquick brown \t fox  ");
        assert_eq!(Some(SplitChunk::new("the", 0)), iter.next());
        assert_eq!(Some(SplitChunk::new("quick", 4)), iter.next());
        assert_eq!(Some(SplitChunk::new("brown", 10)), iter.next());
        assert_eq!(Some(SplitChunk::new("fox", 18)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_leading_trailing_whitespace() {
        let mut iter = split_whitespace("  the\tquick brown \t fox  ");
        assert_eq!(Some(SplitChunk::new("", 0)), iter.next());
        assert_eq!(Some(SplitChunk::new("the", 2)), iter.next());
        assert_eq!(Some(SplitChunk::new("quick", 6)), iter.next());
        assert_eq!(Some(SplitChunk::new("brown", 12)), iter.next());
        assert_eq!(Some(SplitChunk::new("fox", 20)), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next());
    }
}
