*Usage:* `{cpfx}quoteuser [{lopfx}any|{lopfx}bad] [{sopfx}R] [{sopfx}c] USER [SUBSTRING]`

Randomly chooses a quote from all quotes related to the given user and containing the given substring and outputs it. If no substring is given, a quote is chosen randomly from the pool of all quotes related to the given user.

A quote is considered to be related to the given user if either of these options is true:

* The quote was added using `{cpfx}remember` and the user originally posted the message that was remembered as a quote.

* The quote was added using `{cpfx}addquote` by that user.

By default, only "good" quotes, i.e. quotes whose sum of votes is above a given threshold, are output. This threshold is configurable by the bot operator. The option `{lopfx}bad` can be used to only output quotes that do not meet this criterion, and the option `{lopfx}any` can be used to pick from all available quotes, independent of their vote sum.

The `{sopfx}R` option hides the rating of the quote given by the user who issued the command.

The `{sopfx}c` option forces case-sensitive matching for `USER` and `SUBSTRING`. Otherwise, they are matched case-insensitively.

Once a quote has been obtained using `{cpfx}quoteuser` or a similar command, the commands `{cpfx}upquote`/`{cpfx}uq` and `{cpfx}downquote`/`{cpfx}dq` can be used to vote on it.
