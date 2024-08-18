use std::cmp::Ordering;

use unicode_normalization::char::{decompose_compatible, is_combining_mark};


fn unicode_compatible_without_combining(s: &str) -> String {
    let mut ret = String::with_capacity(s.len());
    for c in s.chars() {
        decompose_compatible(c, |dc| {
            if !is_combining_mark(dc) {
                ret.push(dc);
            }
        })
    }
    ret
}

fn text_compare(a: &str, b: &str) -> Ordering {
    // first, compare LOWERCASE(STRIP_COMBINING(UNICODE_NFKD(string)))
    let a_uncombined_lowercase = unicode_compatible_without_combining(a).to_lowercase();
    let b_uncombined_lowercase = unicode_compatible_without_combining(b).to_lowercase();
    let delta_uncombined_lowercase = a_uncombined_lowercase.cmp(&b_uncombined_lowercase);
    if !delta_uncombined_lowercase.is_eq() {
        return delta_uncombined_lowercase;
    }

    // next, compare LOWERCASE(string)
    let a_lowercase = a.to_lowercase();
    let b_lowercase = b.to_lowercase();
    let delta_lowercase = a_lowercase.cmp(&b_lowercase);
    if !delta_lowercase.is_eq() {
        return delta_lowercase;
    }

    // finally, compare string
    a.cmp(&b)
}

pub(crate) fn sort_as_text<S: AsRef<str>>(texts: &mut [S]) {
    texts.sort_unstable_by(|a, b| text_compare(a.as_ref(), b.as_ref()))
}
