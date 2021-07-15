*Usage:* `{cpfx}roll DICE [DICE...]`

Rolls one or more dice and outputs the results.

A dice specification has the following structure:

    [COUNT]d<SIDES>[MUL][ADDSUB]

Apart from `SIDES`, all parameters are optional; a minimal dice specification is `d2`, which is equivalent to a single coin (two-sided die).

`COUNT` is an integer specifying the number of same-type dice being thrown. `MUL` is a multiplication expression with which the roll is multiplied. `ADDSUB` is an addition or subtraction expression which is added to or subtracted from the result of the multiplication (or the roll itself if no multiplication expression is given). Basically, the default `COUNT` is 1, the default `MULDIV` is `*1` and the default `ADDSUB` is `+0`.

As a more complicated example, `4d6*-10+3` rolls four six-sided dice, multiplies the result by minus 10 and adds 3. The possible results for a single die are therefore -7, -17, -27, -37, -47, or -57.
