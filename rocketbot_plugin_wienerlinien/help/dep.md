*Usage:* `{cpfx}dep [{sopfx}l|{lopfx}line LINE]... [{sopfx}s|{lopfx}search] STATION`

Outputs public transport departures from the given `STATION`. If `{sopfx}l LINE` or `{lopfx}line LINE` is given (which can be done multiple times, e.g. `{sopfx}l 99A {sopfx}l 99B`), only departures for those lines are shown.

By default, stations are searched using the following criteria by decreasing priority:
1. station number (if `STATION` is a number)
2. prefix match (preferring shorter names)
3. substring match (preferring shorter names)
4. textual similarity (Damerau-Levenshtein)
To skip search by station number, even for fully numeric station names, pass `{sopfx}s` or `{lopfx}search`. To obtain all relevant station names and numbers, use the `{cpfx}stations` command.
