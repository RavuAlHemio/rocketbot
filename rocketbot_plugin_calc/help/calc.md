*Usage:* `{cpfx}calc EXPRESSION`

Simplifies the given arithmetic expression and outputs the result.

The following standard binary operators are supported: `a+b` (add), `a-b` (subtract), `a*b` (multiply), `a/b` (divide), `a//b` (integer division), `a%b` (division remainder) and `a**b` (exponentiate). Exponentiation is right-associative; the others are left-associative.

The following bitwise binary operators are supported: `a&b` (and), `a|b` (or), `a^b` (xor).

The following unary operators are supported: `-a` (negate) and `a!` (factorial).

The library also contains a selection of common mathematical functions (especially trigonometry) and constants (mainly `pi` and `e`).

Calculation with units is supported as well. A unit can be attached to a number using `#`, the abbreviation of the unit and the optional exponent (which is otherwise assumed to be 1). A product of multiple units can be expressed by attaching multiple units in sequence, e.g. `1#kg#m#s-2` for \(1\frac{\text{kg}\cdot\text{m}}{\text{s}^2}\).

The built-in function `coerce` can be used to convert a value in one unit to a different, compatible unit, e.g. `coerce(1#N, 0#lbf)`. The numeric value of the second argument is ignored; only its units are used by the coercion process.
