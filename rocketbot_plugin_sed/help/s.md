*Usage:* `s/OLD/NEW/[FLAGS]`

Searches for the regular expression `OLD` in the recent channel messages, takes the most recent message found, and replaces the first occurrence of `OLD` with `NEW`.

The format of the regex pattern `OLD` is bound by the feature set of the [Rust `regex` module](https://docs.rs/regex/).

`NEW` can contain references to the full match using `\0` as well as references to parenthesized capture groups 1 through 9 using `\1` through `\9`.

The following optional `FLAGS` are supported:

* a number (starting at `0`) specifies that only that occurrence, counted from the beginning of the string, should be replaced; a negative number specifies that only that occurrence counted from the *end* of the string should be replaced.
* `g` specifies that all occurrences should be replaced, not just a single one. When used in conjunction with a number, replacements start at the occurrence specified by the number and continue to the end of the string.
* `i` specifies case-insensitive matching for `OLD`.
* `x` ignores whitespace and allows line comments (starting with `#`) in `OLD`.

Different ASCII symbols can be used as delimiters instead of `/`. This is useful if the pattern or replacement contains a `/`.
