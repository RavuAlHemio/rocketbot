*Usage:* `{cpfx}bimridertypes [-n|--sort-by-number] [-y|--last-year|-m|--last-month|-w|--last-week] [USERNAME]`

Returns the types of vehicles ridden by the rider with the given `USERNAME`. If none is given, outputs this information about the user who issued the command.

A rider's vehicle types are sorted alphabetically by company and type by default. Passing `-n` or `--sort-by-number` sorts them by number of rides in descending order instead.

Normally considers all rides "since the beginning of time". This can be limited:
* With `-y` or `--last-year`, only considers rides in the last 366 days.
* With `-m` or `--last-month`, only considers rides in the last 31 days.
* With `-w` or `--last-week`, only considers rides in the last 7 days.
