use std::fmt::{self, Write};

use askama;
use unicode_normalization::char::{decompose_compatible, is_combining_mark};


pub(crate) trait Copifiable<T> {
    fn copify(&self) -> T;
}
impl<T: Copy> Copifiable<T> for T {
    fn copify(&self) -> T { *self }
}
impl<T: Copy> Copifiable<T> for &T {
    fn copify(&self) -> T { **self }
}
impl<T: Copy> Copifiable<T> for &&T {
    fn copify(&self) -> T { ***self }
}


pub(crate) fn unref<T: Copy>(value: &&T) -> askama::Result<T> {
    Ok(**value)
}
pub(crate) fn percentify<V: Copifiable<f64>>(value: V) -> askama::Result<String> {
    Ok(format!("{:.2}%", value.copify() * 100.0))
}
pub(crate) fn refify<T>(value: &T) -> askama::Result<&T> {
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ColorError {
    InvalidMinColor,
    InvalidMaxColor,
}
impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMinColor => write!(f, "invalid min_color"),
            Self::InvalidMaxColor => write!(f, "invalid max_color"),
        }
    }
}
impl std::error::Error for ColorError {
}
impl From<ColorError> for askama::Error {
    fn from(ce: ColorError) -> Self { askama::Error::Custom(Box::new(ce)) }
}

fn hex_color_to_color(mut hex_color: &str) -> Option<(f64, f64, f64)> {
    hex_color = hex_color.strip_prefix('#').unwrap_or(hex_color);

    if hex_color.len() == 6 {
        let r_str = hex_color.get(0..2)?;
        let g_str = hex_color.get(2..4)?;
        let b_str = hex_color.get(4..6)?;

        let r_byte = u8::from_str_radix(r_str, 16).ok()?;
        let g_byte = u8::from_str_radix(g_str, 16).ok()?;
        let b_byte = u8::from_str_radix(b_str, 16).ok()?;

        let r_f64 = (r_byte as f64) / 255.0;
        let g_f64 = (g_byte as f64) / 255.0;
        let b_f64 = (b_byte as f64) / 255.0;

        Some((r_f64, g_f64, b_f64))
    } else if hex_color.len() == 3 {
        // xyz -> xxyyzz
        let mut hex_bytes = hex_color.bytes();
        let r_char = hex_bytes.next()?;
        let g_char = hex_bytes.next()?;
        let b_char = hex_bytes.next()?;

        let buf = [r_char, r_char, g_char, g_char, b_char, b_char];
        let buf_str = std::str::from_utf8(&buf).ok()?;

        hex_color_to_color(buf_str)
    } else {
        None
    }
}

fn color_to_hex_color(mut rgb: (f64, f64, f64)) -> String {
    rgb = (
        rgb.0.clamp(0.0, 1.0),
        rgb.1.clamp(0.0, 1.0),
        rgb.2.clamp(0.0, 1.0),
    );

    let r_byte = (rgb.0 * 255.0).round() as u8;
    let g_byte = (rgb.1 * 255.0).round() as u8;
    let b_byte = (rgb.2 * 255.0).round() as u8;

    format!("#{:02X}{:02X}{:02X}", r_byte, g_byte, b_byte)
}

pub(crate) fn mix_color<V: Copifiable<i64>, N: Copifiable<i64>, X: Copifiable<i64>>(value_c: V, min_value_c: N, max_value_c: X, min_color: &str, max_color: &str) -> askama::Result<String> {
    let min_color = hex_color_to_color(min_color)
        .ok_or_else(|| ColorError::InvalidMinColor)?;
    let max_color = hex_color_to_color(max_color)
        .ok_or_else(|| ColorError::InvalidMaxColor)?;

    let value = value_c.copify();
    let min_value = min_value_c.copify();
    let max_value = max_value_c.copify();

    if value < min_value {
        return Ok(color_to_hex_color(min_color));
    }
    if value > max_value {
        return Ok(color_to_hex_color(max_color));
    }

    let value_f64 = value as f64;
    let min_value_f64 = min_value as f64;
    let max_value_f64 = max_value as f64;

    // lerp
    let value_pos = (value_f64 - min_value_f64) / (max_value_f64 - min_value_f64);
    let my_color = (
        min_color.0 + value_pos * (max_color.0 - min_color.0),
        min_color.1 + value_pos * (max_color.1 - min_color.1),
        min_color.2 + value_pos * (max_color.2 - min_color.2),
    );

    Ok(color_to_hex_color(my_color))
}

pub(crate) fn or_empty<'a>(string: &'a Option<String>) -> askama::Result<&'a str> {
    Ok(
        string
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("")
    )
}

pub(crate) fn slugify(string: &str) -> askama::Result<String> {
    let mut ret = String::new();
    for c in string.chars() {
        if c.is_alphanumeric() {
            decompose_compatible(c, |dc| {
                if !is_combining_mark(dc) {
                    ret.push(dc);
                }
            });
        } else {
            ret.push('-');
        }
    }
    Ok(ret)
}

pub(crate) fn encode_query_parameter(string: &str) -> askama::Result<String> {
    let mut ret = String::with_capacity(string.len());
    for b in string.bytes() {
        if b == b' ' {
            ret.push('+');
        } else {
            let can_verbatim =
                b == b'*'
                || b == b'-'
                || b == b'.'
                || (b >= b'0' && b <= b'9')
                || (b >= b'A' && b <= b'Z')
                || b == b'_'
                || (b >= b'a' && b <= b'z')
            ;
            if can_verbatim {
                ret.push(char::from_u32(b.into()).unwrap());
            } else {
                write!(ret, "%{:02X}", b).unwrap();
            }
        }
    }
    Ok(ret)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn ts(input: &str, slug: &str) {
        let slugified = slugify(input).unwrap();
        assert_eq!(&slugified, slug);
    }

    #[test]
    fn test_slugify() {
        ts("", "");
        ts("abcdef", "abcdef");
        ts("T1F", "T1F");
        ts("4023/4024/4124", "4023-4024-4124");
        ts("MAN (Lion's City) NL273 T2", "MAN--Lion-s-City--NL273-T2");
    }
}
