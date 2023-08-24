*Usage:* `{cpfx}bimridertypes [{sopfx}n|{lopfx}sort-by-number] [{sopfx}y|{lopfx}last-year|{sopfx}m|{lopfx}last-month|{sopfx}w|{lopfx}last-week|{sopfx}d|{lopfx}last-day] [USERNAME]`

Returns the types of vehicles ridden by the rider with the given `USERNAME`. If none is given, outputs this information about the user who issued the command.

A rider's vehicle types are sorted alphabetically by company and type by default. Passing `{sopfx}n` or `{lopfx}sort-by-number` sorts them by number of rides in descending order instead.

Normally considers all rides "since the beginning of time". This can be limited:
* With `{sopfx}y` or `{lopfx}last-year`, only considers rides in the last 366 days.
* With `{sopfx}m` or `{lopfx}last-month`, only considers rides in the last 31 days.
* With `{sopfx}w` or `{lopfx}last-week`, only considers rides in the last 7 days.
* With `{sopfx}d` or `{lopfx}last-day`, only considers rides the last day (24 hours).
