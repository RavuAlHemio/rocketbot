*Usage:* `{cpfx}calc EXPRESSION`

Simplifies the given arithmetic expression and outputs the result.

The following standard binary operators are supported: `a+b` (add), `a-b` (subtract), `a*b` (multiply), `a/b` (divide), `a//b` (integer division), `a%b` (division remainder) and `a**b` (exponentiate). Exponentiation is right-associative; the others are left-associative.

The following bitwise binary operators are supported: `a&b` (and), `a|b` (or), `a^b` (xor).

The following unary operators are supported: `-a` (negate) and `a!` (factorial).

The library also contains a selection of common mathematical functions (especially trigonometry) and constants (mainly `pi` and `e`).
