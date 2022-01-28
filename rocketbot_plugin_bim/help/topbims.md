*Usage:* `{cpfx}topbims [--company COMPANY] [-m|--last-month|-y|--last-year|-w|--last-week]`

Returns the most-ridden vehicle(s).

`-c` or `--company` specifies the company operating that vehicle. A default company chosen by the bot operator is used if none is supplied explicitly.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
