*Usage:* `{cpfx}bimcost [{sopfx}y|{lopfx}last-year|{sopfx}m|{lopfx}last-month|{sopfx}w|{lopfx}last-week|{sopfx}d|{lopfx}last-day]`

Returns the sum of all ticket prices in the given time period. Only considers the rides of the user who issued the command.

The sum is calculated over the prices of the actual tickets subtracted from the prices of regular tickets. This allows for long-term ticket amortization calculations.

Normally considers all rides "since the beginning of time". This can be limited:
* With `{sopfx}y` or `{lopfx}last-year`, only considers rides in the last 366 days.
* With `{sopfx}m` or `{lopfx}last-month`, only considers rides in the last 31 days.
* With `{sopfx}w` or `{lopfx}last-week`, only considers rides in the last 7 days.
* With `{sopfx}d` or `{lopfx}last-day`, only considers rides the last day (24 hours).
