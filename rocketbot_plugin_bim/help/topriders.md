*Usage:* `{cpfx}topriders [{sopfx}y|{lopfx}last-year|{sopfx}m|{lopfx}last-month|{sopfx}w|{lopfx}last-week|{sopfx}d|{lopfx}last-day] [{sopfx}c COMPANY|{lopfx}company=COMPANY] [VEHICLE]`

Returns the most active riders.

With `{sopfx}c COMPANY` or `{lopfx}company=COMPANY`, only counts vehicles of a given company. With `VEHICLE`, shows the riders with the most rides of a specific vehicle.

When calculating the number of unique vehicles, only the first vehicle in the ride is considered. This ensures that the uniqueness percentage does not go beyond 100% and allows some comparability between rides with coupled vehicles.

Normally considers all rides "since the beginning of time". This can be limited:
* With `{sopfx}y` or `{lopfx}last-year`, only considers rides in the last 366 days.
* With `{sopfx}m` or `{lopfx}last-month`, only considers rides in the last 31 days.
* With `{sopfx}w` or `{lopfx}last-week`, only considers rides in the last 7 days.
* With `{sopfx}d` or `{lopfx}last-day`, only considers rides the last day (24 hours).
