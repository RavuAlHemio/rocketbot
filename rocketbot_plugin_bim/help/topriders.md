*Usage:* `{cpfx}topriders [-y|--last-year|-m|--last-month|-w|--last-week]`

Returns the most active riders.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
