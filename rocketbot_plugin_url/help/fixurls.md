*Usage:* `{cpfx}fixurls [MESSAGE]` or `{cpfx}fixurl [MESSAGE]`

Attempts to fix URLs in `MESSAGE` such that Rocket.Chat links them properly by URL-encoding some characters that traditionally are no longer URL-encoded.

If no `MESSAGE` is given, output the last message with fixed URLs that had URLs to fix.
