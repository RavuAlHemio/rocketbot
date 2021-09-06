*Usage:* `{cpfx}nines <NUMBER>` or `{cpfx}nines <NUMBER> <NUMBER>s` or `{cpfx}nines <NUMBER>%`

Displays the given uptime as a percentage as well as the allowed amount of downtime per day, week, month and year.

As `{cpfx}nines <NUMBER>`, the uptime is interpreted as a _number of nines_, e.g. `{cpfx}nines 5` stands for _five nines_ or 99.999%. As `{cpfx}nines <NUMBER> <NUMBER>s`, a different digit than a nine can be specified, e.g. `{cpfx}nines 9 5s` stands for _nine fives_ or 55.5555555%. As `{cpfx}nines <NUMBER>%`, the uptime percentage is interpreted directly as such, e.g. `{cpfx}nines 99.999%` is taken to mean 99.999%.
