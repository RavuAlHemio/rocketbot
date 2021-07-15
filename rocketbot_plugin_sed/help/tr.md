*Usage:* `tr/OLD/NEW/[d|r]`

Replaces each character in the `OLD` specification by the corresponding character in the `NEW` specification.

Characters can be specified either as single characters, e.g. `a`, or as character ranges, e.g. `a-f`. To specify a literal `-` character, place it at the beginning of the specification (e.g. `tr/-abcd/?wxyz/`), at the end of the specification (e.g. `tr/abcd-/wxyz?/`), or right after another range (e.g. `tr/a-d-w-z/w-z?a-d/`).

Normally, both `OLD` and `NEW` must encompass the same number of characters; otherwise, the command fails. The flags `d` and `r` can modify this behavior: when `d` (_delete_) is specified and `OLD` contains more characters than `NEW`, `OLD` is truncated at its end until the lengths match; when `r` (_repeat_) is specified and `OLD` contains more characters than `NEW`, the last character in `NEW` is repeated until the lengths match. For example, `tr/a-z/w-z/d` replaces characters `a` though `d` with `w` through `z`, respectively, and `tr/eiouy/a/r` replaces any occurrence of the letters `eiouy` with the letter `a`.

As an example, `tr/A-Za-z/N-ZA-Mn-za-m/` performs a ROT13 replacement.

Different ASCII symbols can be used as delimiters instead of `/`. This is useful if the pattern or replacement contains a `/`.
