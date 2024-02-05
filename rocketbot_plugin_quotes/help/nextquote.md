*Usage:* `{cpfx}nextquote [{lopfx}any|{lopfx}bad] [{sopfx}R]`

Chooses the next entry from a pre-shuffled list of all quotes.

While `{cpfx}quote` picks a single quote at random, which may lead to the same quote being displayed multiple times in short order, `{cpfx}nextquote` ensures that all other quotes have been displayed before the same quote is shown again.

Note that the pre-shuffled list is reshuffled anytime the bot is restarted or a new quote is added.

By default, only "good" quotes, i.e. quotes whose sum of votes is above a given threshold, are output. This threshold is configurable by the bot operator. The option `{lopfx}bad` can be used to only output quotes that do not meet this criterion, and the option `{lopfx}any` can be used to pick from all available quotes, independent of their vote sum.

The `{sopfx}R` option hides the rating of the quote given by the user who issued the command.

Once a quote has been obtained using `{cpfx}nextquote` or a similar command, the commands `{cpfx}upquote`/`{cpfx}uq` and `{cpfx}downquote`/`{cpfx}dq` can be used to vote on it.
