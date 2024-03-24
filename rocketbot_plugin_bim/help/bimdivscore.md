*Usage:* `{cpfx}bimdivscore [{sopfx}n|{lopfx}sort-by-number] [{sopfx}y|{lopfx}last-year|{sopfx}m|{lopfx}last-month|{sopfx}w|{lopfx}last-week|{sopfx}d|{lopfx}last-day]`

Outputs the divisibility score for all participating riders.

If both vehicle and line contain only one sequence of digits (optionally neighboring or surrounded by non-digits) each, they are taken as numbers. If the vehicle number is divisible by the line number, the line number is added to the divisibility score.

The output is sorted by the rider's username by default. Passing `{sopfx}n` or `{lopfx}sort-by-number` sorts it by the balance in descending order instead.

Normally considers all rides "since the beginning of time". This can be limited:
* With `{sopfx}y` or `{lopfx}last-year`, only considers rides in the last 366 days.
* With `{sopfx}m` or `{lopfx}last-month`, only considers rides in the last 31 days.
* With `{sopfx}w` or `{lopfx}last-week`, only considers rides in the last 7 days.
* With `{sopfx}d` or `{lopfx}last-day`, only considers rides the last day (24 hours).
