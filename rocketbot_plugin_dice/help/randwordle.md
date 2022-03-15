*Usage:* `{cpfx}randwordle [OPTIONS]`

Generates a random Wordle guess series, consisting of squares denoting wrong (white, ⬜), misplaced (yellow, 🟨) and correct (green, 🟩) guesses.

The following `OPTIONS` are available:

* `{sopfx}s|{lopfx}squares NUMBER`: Sets the number of squares per guess. The default is 5.
* `{sopfx}l|{lopfx}length NUMBER`: Sets the maximum number of guesses. The default is 6.
* `{sopfx}d|{lopfx}dark`: Displays incorrectly guessed fields as black (⬛) instead of white squares (⬜).
* `{sopfx}p|{lopfx}purple`: Displays misplaced fields as purple (🟪) instead of yellow squares (🟨).

Presets:

* Wordle in light mode: `{cpfx}randwordle`
* Wordle in dark mode: `{cpfx}randwordle -d`
* Nerdle: `{cpfx}randwordle -dp -s 8`
