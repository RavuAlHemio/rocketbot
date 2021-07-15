*Usage:* `{cpfx}quote [{lopfx}any|{lopfx}bad] [{sopfx}r] [SUBSTRING]`

Randomly chooses a quote from all quotes containing the given substring and outputs it. If no substring is given, a quote is chosen randomly from the pool of all quotes.

By default, only "good" quotes, i.e. quotes whose sum of votes is above a given threshold, are output. This threshold is configurable by the bot operator. The option `{lopfx}bad` can be used to only output quotes that do not meet this criterion, and the option `{lopfx}any` can be used to pick from all available quotes, independent of their vote sum.

The `{sopfx}r` option additionally displays the rating of the quote given by the user who issued the command.

Once a quote has been obtained using `{cpfx}quote` or a similar command, the commands `{cpfx}upquote`/`{cpfx}uq` and `{cpfx}downquote`/`{cpfx}dq` can be used to vote on it.
