*Usage:* `{cpfx}topbimlines [-y|--last-year|-m|--last-month|-w|--last-week|-d|--last-day] [RIDER]`

Returns the lines with the most vehicles ridden. If a `RIDER` is given, only considers rides by that rider.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
* With `-d` or `--last-day`, only considers rides the last day (24 hours).

Note that, to reflect common practice with municipal public transit operators, it is assumed that a day starts at 04:00 and not midnight. Rides on or after midnight and before 04:00 are counted towards the previous day.
