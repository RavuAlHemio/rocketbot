*Usage:* `{cpfx}topbimdays [{sopfx}y|{lopfx}last-year|{sopfx}m|{lopfx}last-month|{sopfx}w|{lopfx}last-week|{sopfx}d|{lopfx}last-day] [RIDER]`

Returns the days with the most vehicles ridden. If a `RIDER` is given, only considers rides by that rider.

Normally considers all rides "since the beginning of time". This can be limited:
* With `{sopfx}y` or `{lopfx}last-year`, only considers rides in the last 366 days.
* With `{sopfx}m` or `{lopfx}last-month`, only considers rides in the last 31 days.
* With `{sopfx}w` or `{lopfx}last-week`, only considers rides in the last 7 days.
* With `{sopfx}d` or `{lopfx}last-day`, only considers rides the last day (24 hours).

Note that, to reflect common practice with municipal public transit operators, it is assumed that a day starts at 04:00 and not midnight. Rides on or after midnight and before 04:00 are counted towards the previous day.
