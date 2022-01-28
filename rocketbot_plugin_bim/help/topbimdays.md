*Usage:* `{cpfx}topbimdays [-y|--last-year|-m|--last-month|-w|--last-week]`

Returns the days with the most vehicles ridden.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.

Note that, to reflect common practice with municipal public transit operators, it is assumed that a day starts at 04:00 and not midnight. Rides on or after midnight and before 04:00 are counted towards the previous day.
