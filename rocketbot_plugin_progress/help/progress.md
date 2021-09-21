*Usage:* `{cpfx}progress TEXT`

Annotates percentages in `TEXT` with progress bars.

Percentages detected by this command follow the `[-]<INTEGER>%[<STARTCHAR>[<ENDCHAR>]]`. Only percentages between -200% and 200% are allowed. `STARTCHAR` and `ENDCHAR` optionally specify the starting and ending character of the progress bar if it has at least two segments; otherwise, they are taken to be `=` by default. The character between the starting and ending character is always `=`.
