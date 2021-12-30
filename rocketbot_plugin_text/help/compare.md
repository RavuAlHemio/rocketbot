*Usage:* `{cpfx}compare STRING1 STRING2`

Outputs information about how much `STRING1` and `STRING2` differ.

If the strings contain spaces, standard command string tokenization rules apply: strings in quotation marks (`"`) are considered one argument, quotation marks can be escaped using backslashes (`\`), and backslashes can be doubled to escape them.

The following metrics are calculated:

* `<`, `==` or `>`: standard case-sensitive character-wise comparison
* `L`: Levenshtein distance
* `D-L`: Damerau-Levenshtein distance
* `OSA`: Optimal String Alignment distance
* `H`: Hamming distance (only works for strings of the same length; `!H` appears for strings with differing length)
* `J`: Jaro similarity
* `J-W`: Jaro-Winkler similarity
* `S-D`: Sørensen-Dice similarity
* `SCS`: both strings use the same set of characters (or `!SCS` if not)
