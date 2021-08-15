*Usage:* `{cpfx}paper PAPER`

Displays the (unrounded) ISO 216 paper size given by `PAPER`. `PAPER` consists of a series letter (`A`, `B` or `C`) and an integer specifying the number of times the base size (0) is halved.

Deviating from ISO 216 and related standards, sizes larger than the base size are not given by the number of tiled base-size pages necessary to reach the required area (e.g. `2A0`, `4A0`) but a negative integer (e.g. `A-1` and `A-2`, corresponding to `2A0` and `4A0` respectively).
