*Usage:* `{cpfx}rotfind [OPTIONS] STRING`

Performs all 25 letter-rotation ciphers on `STRING` and outputs those that form a word. Only letters `A` through `Z` as well as `a` through `z` are rotated.

For example, `{cpfx}rotfind envy` finds `13: rail`.

`OPTIONS` are:

* `{sopfx}a`/`{lopfx}any`: Lists the results of all 25 letter-rotation ciphers, whether or not they form a word.
