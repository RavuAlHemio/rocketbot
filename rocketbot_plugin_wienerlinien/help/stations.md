*Usage:* `{cpfx}stations STATION`

Outputs the names and numbers of public transport stations matching `STATION`. A station is matched by:
* station number (if `STATION` is a number)
* prefix match (preferring shorter names)
* substring match (preferring shorter names)
* textual similarity (Damerau-Levenshtein)

Station numbers can be used as input to the `{cpfx}dep` command (as long as the `{sopfx}s` or `{lopfx}search` flag is *not* provided).
