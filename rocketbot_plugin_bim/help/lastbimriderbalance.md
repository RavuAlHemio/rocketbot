*Usage:* `{cpfx}lastbimriderbalance [{sopfx}y|{lopfx}last-year|{sopfx}m|{lopfx}last-month|{sopfx}w|{lopfx}last-week|{sopfx}d|{lopfx}last-day]`

Outputs the last-rider balance for all participating riders.

The last-rider balance is increased by 1 if a rider rides a vehicle that was previously ridden by a different rider, and decreased by 1 if a different rider rides a vehicle that was previously ridden by this rider. For performance reasons, being the first rider in a vehicle overall does not have an effect on the last-rider balance.

Normally considers all rides "since the beginning of time". This can be limited:
* With `{sopfx}y` or `{lopfx}last-year`, only considers rides in the last 366 days.
* With `{sopfx}m` or `{lopfx}last-month`, only considers rides in the last 31 days.
* With `{sopfx}w` or `{lopfx}last-week`, only considers rides in the last 7 days.
* With `{sopfx}d` or `{lopfx}last-day`, only considers rides the last day (24 hours).
