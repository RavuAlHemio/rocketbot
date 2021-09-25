*Usage:* `{cpfx}progress [OPTIONS] TEXT`

Annotates percentages in `TEXT` with progress bars.

Percentages detected by this command follow the `[-]<INTEGER>%[<STARTCHAR>[<ENDCHAR>]]`. Only percentages between -200% and 200% are allowed. `STARTCHAR` and `ENDCHAR` optionally specify the starting and ending character of the progress bar if it has at least two segments; otherwise, they are taken to be `=` by default.

Other customization options are available via the following `OPTIONS`:

* `-f TEXT`/`--foreground TEXT`: One or more characters repeated as the body of the progress bar. `=` by default.
* `-b TEXT`/`--background TEXT`: One or more characters repeated as the empty part of the box. ` ` by default.
* `-s TEXT`/`--start-bar TEXT`: Characters to be used as the beginning of the progress bar. None by default (which immediately uses the foreground). Overridden by `STARTCHAR`, if specified.
* `-e TEXT`/`--end-bar TEXT`: Characters to be used as the end of the progress bar. None by default (which immediately uses the foreground). Overridden by `ENDCHAR`, if specified.
* `-S TEXT`/`--start-box TEXT`: Characters to be used as the frame at the beginning of the box. `[` by default.
* `-E TEXT`/`--end-box TEXT`: Characters to be used as the frame at the end of the box. `]` by default.
