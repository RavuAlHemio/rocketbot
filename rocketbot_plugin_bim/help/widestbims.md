*Usage:* `{cpfx}widestbims [-y|--last-year|-m|--last-month|-w|--last-week]`

Lists vehicles that have served the widest selection of riders. For fixed couplings, only outputs the first vehicle.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
