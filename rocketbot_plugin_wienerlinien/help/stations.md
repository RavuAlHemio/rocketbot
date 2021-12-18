*Usage:* `{cpfx}stations STATION`

Outputs public transport stations matching `STATION`. A station matches if its name contains `STATION` or if it is the closest station according to Damerau-Levenshtein. Station numbers can be used as input to the `{cpfx}dep` command (as long as the `{sopfx}s` or `{lopfx}search` flag is *not* provided).
