*Usage:* `{cpfx}ean13 DIGITS`

Encodes the given sequence of digits as an EAN-13 barcode.

Digits must be ASCII-encoded. The generator accepts two formats:

* 12 digits. The 13th digit (check digit) is calculated and appended to the generated barcode.

* 13 digits. The final digit is assumed to be the correct check digit and included in the generated barcode.
