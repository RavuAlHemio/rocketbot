*Usage:* `x/ONE/OTHER/`

Exchanges the first match of the regular expression `ONE` with the first match of the regular expression `OTHER`.

If the matches overlap, the match for `OTHER` is ignored and a later match for `OTHER` is taken instead.

Different ASCII symbols can be used as delimiters instead of `/`. This is useful if `ONE` or `OTHER` contains a `/`.
