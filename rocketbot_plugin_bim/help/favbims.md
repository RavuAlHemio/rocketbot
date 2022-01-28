*Usage:* `{cpfx}favbims [-y|--last-year|-m|--last-month|-w|--last-week] [USERNAME]`

Outputs each rider's favorite (most-ridden) vehicle. In case of a draw, outputs one of those vehicles at random. If a `USERNAME` is given, outputs the user's top ride counts and the vehicles that match each ride count.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
