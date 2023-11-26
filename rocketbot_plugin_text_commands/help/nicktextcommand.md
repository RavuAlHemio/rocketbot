*Usage:* `{cpfx}{cmd} [NICKNAME]` or `{cpfx}{cmd} {sopfx}r|{lopfx}random [{sopfx}b|{lopfx}also-bots]`

Responds with one of a set of responses preconfigured by the bot operator, replacing a nickname placeholder within the response text thus:

* If a nickname is specified, inserts this nickname.
* If the `{sopfx}r` or the `{lopfx}random` option is given, picks a user at random from the user list of the channel. This excludes bots by default unless `{sopfx}b` or `{lopfx}also-bots` is given.
* Otherwise, inserts the sender's nickname.
