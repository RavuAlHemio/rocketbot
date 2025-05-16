//! The EAN-13 barcode symbology.

use std::fmt;


/// The L encoding for digits 0-9.
///
/// The L and G encodings are used to encode the left group of six digits. Additionally, the exact
/// pattern of whether an L or G was used for a specific digit is used to encode the leftmost (lone)
/// digit.
const L_DIGITS: [[bool; 7]; 10] = [
    [false, false, false, true,  true,  false, true],
    [false, false, true,  true,  false, false, true],
    [false, false, true,  false, false, true,  true],
    [false, true,  true,  true,  true,  false, true],
    [false, true,  false, false, false, true,  true],
    [false, true,  true,  false, false, false, true],
    [false, true,  false, true,  true,  true,  true],
    [false, true,  true,  true,  false, true,  true],
    [false, true,  true,  false, true,  true,  true],
    [false, false, false, true,  false, true,  true],
];

/// The G encoding for digits 0-9.
///
/// The L and G encodings are used to encode the left group of six digits. Additionally, the exact
/// pattern of whether an L or G was used for a specific digit is used to encode the leftmost (lone)
/// digit.
const G_DIGITS: [[bool; 7]; 10] = [
    [false, true,  false, false, true,  true,  true],
    [false, true,  true,  false, false, true,  true],
    [false, false, true,  true,  false, true,  true],
    [false, true,  false, false, false, false, true],
    [false, false, true,  true,  true,  false, true],
    [false, true,  true,  true,  false, false, true],
    [false, false, false, false, true,  false, true],
    [false, false, true,  false, false, false, true],
    [false, false, false, true,  false, false, true],
    [false, false, true,  false, true,  true,  true],
];

/// The R encoding for digits 0-9.
///
/// The R encoding is used to decode the right group of six digits.
const R_DIGITS: [[bool; 7]; 10] = [
    [true,  true,  true,  false, false, true,  false],
    [true,  true,  false, false, true,  true,  false],
    [true,  true,  false, true,  true,  false, false],
    [true,  false, false, false, false, true,  false],
    [true,  false, true,  true,  true,  false, false],
    [true,  false, false, true,  true,  true,  false],
    [true,  false, true,  false, false, false, false],
    [true,  false, false, false, true,  false, false],
    [true,  false, false, true,  false, false, false],
    [true,  true,  true,  false, true,  false, false],
];

/// The L-or-G decision table for encoding the leftmost (lone) digit.
///
/// When encoding the left group of six digits, the value of the leftmost (lone) digit is used to
/// pick the row in this table. Then, for each of the six digits in the left group, the
/// corresponding entry in the row is consulted to decide which encoding to use for that digit. If
/// the entry is `true`, the digit is encoded using the G encoding; if the entry is `false`, the L
/// encoding is used.
const FIRST_DIGIT_USE_G: [[bool; 6]; 10] = [
    [false, false, false, false, false, false],
    [false, false, true,  false, true,  true],
    [false, false, true,  true,  false, true],
    [false, false, true,  true,  true,  false],
    [false, true,  false, false, true,  true],
    [false, true,  true,  false, false, true],
    [false, true,  true,  true,  false, false],
    [false, true,  false, true,  false, true],
    [false, true,  false, true,  true,  false],
    [false, true,  true,  false, true,  false],
];


/// A single decimal digit to encode in a barcode.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Digit(u8);
impl Digit {
    pub const fn as_u8(&self) -> u8 { self.0 }
    pub const fn try_from_u8(value: u8) -> Option<Self> {
        if value < 10 {
            Some(Self(value))
        } else {
            None
        }
    }
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }
}
impl From<Digit> for u8 {
    fn from(value: Digit) -> Self {
        value.as_u8()
    }
}
impl TryFrom<u8> for Digit {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from_u8(value)
            .ok_or(value)
    }
}
impl fmt::Debug for Digit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl fmt::Display for Digit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}


/// Calculates the EAN-13 check digit.
pub fn calculate_check_digit(digits: [Digit; 12]) -> Digit {
    // weights are alternating 3, 1, ... from the right
    // since we always have 12 digits, the leftmost digit starts at 1
    let mut weighted_sum: u8 = 0;
    for (i, digit) in digits.into_iter().enumerate() {
        let weighted_value = if i % 2 == 0 {
            u8::from(digit) * 1
        } else {
            u8::from(digit) * 3
        };

        // even in the worst case (all digits 9), the result (216) fits into u8
        weighted_sum += weighted_value;
    }

    let check_digit_value = (10 - (weighted_sum % 10)) % 10;
    Digit::try_from(check_digit_value).unwrap()
}


/// Encodes an EAN-13 bar code.
///
/// The final (check) digit is not checked and assumed to be valid.
///
/// The barcode has the following pattern:
/// 1. start marker `101` (3 areas, 0..=2)
/// 2. six digits with seven areas each (42 areas, 3..=44)
/// 3. center marker `01010` (5 areas, 45..=49)
/// 4. six digits with seven areas each (42 areas, 50..=91)
/// 5. end marker `101` (3 areas, 92..=94)
pub fn encode_ean_13(digits: [Digit; 13]) -> [bool; 95] {
    let mut ret = [false; 95];

    // encode constant sections
    ret[0] = true;
    ret[1] = false;
    ret[2] = true;

    ret[45] = false;
    ret[46] = true;
    ret[47] = false;
    ret[48] = true;
    ret[49] = false;

    ret[92] = true;
    ret[93] = false;
    ret[94] = true;

    // encode left section
    let first_digit = digits[0];
    let use_g_pattern = FIRST_DIGIT_USE_G[first_digit.as_usize()];

    let left_block = &digits[1..7];
    for ((i, digit), &use_g) in left_block.iter().enumerate().zip(use_g_pattern.iter()) {
        let offset = 3 + i * 7;
        let pattern = if use_g { &G_DIGITS } else { &L_DIGITS };
        for (ret_bit, pattern_bit) in ret[offset..offset+7].iter_mut().zip(pattern[digit.as_usize()].iter()) {
            *ret_bit = *pattern_bit;
        }
    }

    // encode right section (more straightforward)
    let right_block = &digits[7..13];
    for (i, digit) in right_block.iter().enumerate() {
        let offset = 50 + i * 7;
        for (ret_bit, pattern_bit) in ret[offset..offset+7].iter_mut().zip(R_DIGITS[digit.as_usize()].iter()) {
            *ret_bit = *pattern_bit;
        }
    }

    ret
}

#[cfg(test)]
mod tests {
    use super::{Digit, encode_ean_13};

    #[test]
    fn test_encode_ean_13() {
        // Wikipedia example
        let raw_digits: [u8; 13] = [
            4,
            0, 0, 3, 9, 9, 4,
            1, 5, 5, 4, 8, 6,
        ];
        let mut digits = [Digit::default(); 13];
        for (digit, raw_digit) in digits.iter_mut().zip(raw_digits.iter()) {
            *digit = Digit::try_from_u8(*raw_digit).unwrap();
        }

        let encoded = encode_ean_13(digits);
        const B: bool = true;
        const W: bool = false;
        let raw_expected = [
            // start marker
            B, W, B,

            // left digits
            W, W, W, B, B, W, B, // 0, L-code
            W, B, W, W, B, B, B, // 0, G-code
            W, B, B, B, B, W, B, // 3, L-code
            W, W, W, B, W, B, B, // 9, L-code
            W, W, B, W, B, B, B, // 9, G-code
            W, W, B, B, B, W, B, // 4, G-code

            // middle marker
            W, B, W, B, W,

            // right digits (all R-code)
            B, B, W, W, B, B, W, // 1
            B, W, W, B, B, B, W, // 5
            B, W, W, B, B, B, W, // 5
            B, W, B, B, B, W, W, // 4
            B, W, W, B, W, W, W, // 8
            B, W, B, W, W, W, W, // 6

            // end marker
            B, W, B,
        ];

        assert_eq!(encoded, raw_expected);
    }
}
