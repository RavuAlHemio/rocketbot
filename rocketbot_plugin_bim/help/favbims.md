*Usage:* `{cpfx}favbims [-m|--last-month|-y|--last-year|-w|--last-week]`

Outputs each rider's favorite (most-ridden) vehicle. In case of a draw, outputs one of those vehicles at random.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
