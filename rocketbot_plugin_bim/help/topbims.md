*Usage:* `{cpfx}topbims [--company COMPANY] [-y|--last-year|-m|--last-month|-w|--last-week|-d|--last-day]`

Returns the most-ridden vehicle(s).

`-c` or `--company` limits the results to a specific company. Otherwise, vehicles of all known companies are considered.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
* With `-d` or `--last-day`, only considers rides the last day (24 hours).
