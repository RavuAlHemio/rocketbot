*Usage:* `{cpfx}checkdigit TYPE VALUE`

Calculates the check digit for `VALUE`, interpreting it as a value of type `TYPE`.

Some check digit types require, while most types recommend, to enter `VALUE` using `#` as a placeholder for the check digit.

Supported `TYPE`s are:

* `luhn`, `creditcard`, `cc` or `uic`: check digit calculated according to the Luhn algorithm, including credit card numbers and UIC rolling stock numbers
* `atsvnr`: Austrian social security number (Sozialversicherungsnummer)
* `czrodc`: Czech birth number (rodné číslo)
* `iban`: International Bank Account Number

Examples:

* `{cpfx}checkdigit cc 4111 1111 1111 111#`
* `{cpfx}checkdigit uic 73 81 84-90 101-#`
* `{cpfx}checkdigit atsvnr 782# 28 07 55`
* `{cpfx}checkdigit czrodc 010101/001#`
* `{cpfx}checkdigit iban GB## WEST 1234 5698 7654 32`
