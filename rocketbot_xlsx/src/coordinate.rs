use std::fmt;
use std::str::FromStr;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CoordinateError {
    Empty,
    DoesNotStartWithColumn,
    RowDoesNotFollowColumn,
    TrailingJunk,
    ColumnOutOfRange,
    RowOutOfRange,
}
impl fmt::Display for CoordinateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "input empty"),
            Self::DoesNotStartWithColumn => write!(f, "input does not start with column"),
            Self::RowDoesNotFollowColumn => write!(f, "row does not follow column in input"),
            Self::TrailingJunk => write!(f, "trailing junk in input"),
            Self::ColumnOutOfRange => write!(f, "column value out of range"),
            Self::RowOutOfRange => write!(f, "row value out of range"),
        }
    }
}
impl std::error::Error for CoordinateError {}


#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExcelCoordinate {
    pub row: u16,
    pub column: u32,
}
impl ExcelCoordinate {
    pub fn new(row: u16, column: u32) -> Self {
        Self { row, column }
    }
}
impl fmt::Display for ExcelCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // column: 0 = A, 1 = B, ..., 25 = Z, 26 = AA, 27 = AB, ...
        let mut column_letters = Vec::new();
        let mut column = self.column + 1;
        while column > 0 {
            let column_letter = char::from_u32(('A' as u32) + (column - 1) % 26)
                .unwrap();
            column_letters.push(column_letter);
            column = (column - 1) / 26;
        }
        column_letters.reverse();
        for letter in column_letters {
            write!(f, "{}", letter)?;
        }

        // row: simply the index + 1
        write!(f, "{}", self.row + 1)
    }
}
impl fmt::Debug for ExcelCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl FromStr for ExcelCoordinate {
    type Err = CoordinateError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 0 {
            // ""
            return Err(CoordinateError::Empty);
        }

        if !s.starts_with(|c| c >= 'A' && c <= 'Z') {
            // e.g. "21A" or "21" or "@"
            return Err(CoordinateError::DoesNotStartWithColumn);
        }
        let first_non_az = match s.find(|c| c < 'A' || c > 'Z') {
            Some(i) => i,
            None => {
                // e.g. "A" or "AZ"
                return Err(CoordinateError::RowDoesNotFollowColumn);
            },
        };
        if !s[first_non_az..].starts_with(|c| c >= '0' && c <= '9') {
            // e.g. "AZ@"
            return Err(CoordinateError::RowDoesNotFollowColumn);
        }
        if s[first_non_az..].find(|c| c < '0' || c > '9').is_some() {
            // e.g. "AZ9A" or "AZ9@"
            return Err(CoordinateError::TrailingJunk);
        }

        let (letters, numbers) = s.split_at(first_non_az);
        assert!(letters.len() > 0);
        assert!(numbers.len() > 0);
        assert!(letters.chars().all(|c| c >= 'A' && c <= 'Z'));
        assert!(numbers.chars().all(|c| c >= '0' && c <= '9'));

        let mut column: u32 = 0;
        for c in letters.chars() {
            column = column.checked_mul(26)
                .ok_or(CoordinateError::ColumnOutOfRange)?;
            column = column.checked_add((c as u32) - ('A' as u32) + 1)
                .ok_or(CoordinateError::ColumnOutOfRange)?;
        }
        column -= 1;

        let row_number: u16 = numbers.parse()
            .map_err(|_| CoordinateError::RowOutOfRange)?;
        if row_number == 0 {
            return Err(CoordinateError::RowOutOfRange);
        }
        let row = row_number - 1;

        Ok(Self::new(row, column))
    }
}


#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{CoordinateError, ExcelCoordinate};

    fn test_fmt_coord(row: u16, column: u32, expected_name: &str) {
        let coord = ExcelCoordinate::new(row, column);
        assert_eq!(&coord.to_string(), expected_name);
    }

    fn test_parse_coord(expected_row: u16, expected_column: u32, name: &str) {
        let coord: ExcelCoordinate = name.parse().unwrap();
        assert_eq!((coord.row, coord.column), (expected_row, expected_column));
    }

    #[test]
    pub fn test_format_column_coordinates() {
        test_fmt_coord(0, 0, "A1");
        test_fmt_coord(0, 1, "B1");
        test_fmt_coord(0, 2, "C1");
        test_fmt_coord(0, 3, "D1");
        // ...
        test_fmt_coord(0, 24, "Y1");
        test_fmt_coord(0, 25, "Z1");
        test_fmt_coord(0, 26, "AA1");
        test_fmt_coord(0, 27, "AB1");
        test_fmt_coord(0, 28, "AC1");
        // ...
        test_fmt_coord(0, 51, "AZ1");
        test_fmt_coord(0, 52, "BA1");
        test_fmt_coord(0, 53, "BB1");
        test_fmt_coord(0, 54, "BC1");
        // ...
        test_fmt_coord(0, 674, "YY1");
        test_fmt_coord(0, 675, "YZ1");
        test_fmt_coord(0, 676, "ZA1");
        // ...
        test_fmt_coord(0, 700, "ZY1");
        test_fmt_coord(0, 701, "ZZ1");
        test_fmt_coord(0, 702, "AAA1");
        test_fmt_coord(0, 703, "AAB1");
        test_fmt_coord(0, 704, "AAC1");
        // ...
        test_fmt_coord(0, 1376, "AZY1");
        test_fmt_coord(0, 1377, "AZZ1");
        test_fmt_coord(0, 1378, "BAA1");
        test_fmt_coord(0, 1379, "BAB1");
        // ...
        test_fmt_coord(0, 18276, "ZZY1");
        test_fmt_coord(0, 18277, "ZZZ1");
        test_fmt_coord(0, 18278, "AAAA1");
        test_fmt_coord(0, 18279, "AAAB1");
    }

    #[test]
    pub fn test_format_row_coordinates() {
        test_fmt_coord(0, 0, "A1");
        test_fmt_coord(1, 0, "A2");
        test_fmt_coord(2, 0, "A3");
    }

    #[test]
    pub fn test_parse_column_coordinates() {
        test_parse_coord(0, 0, "A1");
        test_parse_coord(0, 1, "B1");
        test_parse_coord(0, 2, "C1");
        test_parse_coord(0, 3, "D1");
        // ...
        test_parse_coord(0, 24, "Y1");
        test_parse_coord(0, 25, "Z1");
        test_parse_coord(0, 26, "AA1");
        test_parse_coord(0, 27, "AB1");
        test_parse_coord(0, 28, "AC1");
        // ...
        test_parse_coord(0, 51, "AZ1");
        test_parse_coord(0, 52, "BA1");
        test_parse_coord(0, 53, "BB1");
        test_parse_coord(0, 54, "BC1");
        // ...
        test_parse_coord(0, 674, "YY1");
        test_parse_coord(0, 675, "YZ1");
        test_parse_coord(0, 676, "ZA1");
        // ...
        test_parse_coord(0, 700, "ZY1");
        test_parse_coord(0, 701, "ZZ1");
        test_parse_coord(0, 702, "AAA1");
        test_parse_coord(0, 703, "AAB1");
        test_parse_coord(0, 704, "AAC1");
        // ...
        test_parse_coord(0, 1376, "AZY1");
        test_parse_coord(0, 1377, "AZZ1");
        test_parse_coord(0, 1378, "BAA1");
        test_parse_coord(0, 1379, "BAB1");
        // ...
        test_parse_coord(0, 18276, "ZZY1");
        test_parse_coord(0, 18277, "ZZZ1");
        test_parse_coord(0, 18278, "AAAA1");
        test_parse_coord(0, 18279, "AAAB1");

        assert_eq!(ExcelCoordinate::from_str(""), Err(CoordinateError::Empty));
        assert_eq!(ExcelCoordinate::from_str(" "), Err(CoordinateError::DoesNotStartWithColumn));
        assert_eq!(ExcelCoordinate::from_str("3"), Err(CoordinateError::DoesNotStartWithColumn));
        assert_eq!(ExcelCoordinate::from_str("@"), Err(CoordinateError::DoesNotStartWithColumn));
        assert_eq!(ExcelCoordinate::from_str("A"), Err(CoordinateError::RowDoesNotFollowColumn));
        assert_eq!(ExcelCoordinate::from_str("A@"), Err(CoordinateError::RowDoesNotFollowColumn));
        assert_eq!(ExcelCoordinate::from_str("A3A"), Err(CoordinateError::TrailingJunk));
        assert_eq!(ExcelCoordinate::from_str("A3@"), Err(CoordinateError::TrailingJunk));
        assert_eq!(ExcelCoordinate::from_str("ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ1"), Err(CoordinateError::ColumnOutOfRange));
        assert_eq!(ExcelCoordinate::from_str("A9999999999999999999999999999999999999999"), Err(CoordinateError::RowOutOfRange));
        assert_eq!(ExcelCoordinate::from_str("A0"), Err(CoordinateError::RowOutOfRange));
    }
}
