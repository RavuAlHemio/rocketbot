*Usage:* `{cpfx}{cmd} [{sopfx}r|{lopfx}random|NICKNAME]`

Responds with one of a set of responses preconfigured by the bot operator, replacing a nickname placeholder within the response text thus:

* If a nickname is specified, inserts this nickname.
* If the `{sopfx}r` or the `{lopfx}random` option is given, picks a user at random from the user list of the channel.
* Otherwise, inserts the sender's nickname.
