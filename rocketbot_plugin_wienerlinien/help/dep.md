*Usage:* `{cpfx}dep [{sopfx}l|{lopfx}line LINE] [{sopfx}s|{lopfx}search] STATION`

Outputs public transport departures from the given `STATION`. If `{sopfx}l LINE` or `{lopfx}line LINE` is given, only departures for that line are shown.

By default, stations are searched either by station number (if `STATION` is a number) or by textual similarity (Damerau-Levenshtein). To force a search by textual similarity, even for fully numeric station names, pass `{sopfx}s` or `{lopfx}search`. For a more exhaustive search, use the `{cpfx}stations` command.
