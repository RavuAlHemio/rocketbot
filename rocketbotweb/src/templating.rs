use std::collections::HashMap;

use serde_json::Value as JsonValue;
use tera::Tera;


fn percentify(value: &JsonValue, _args: &HashMap<String, JsonValue>) -> tera::Result<JsonValue> {
    if let Some(f) = value.as_f64() {
        Ok(JsonValue::String(format!("{:.2}%", f * 100.0)))
    } else {
        Err(tera::Error::msg("attempted to percentify non-numeric value"))
    }
}

fn maybe_escape(value: &JsonValue, _args: &HashMap<String, JsonValue>) -> tera::Result<JsonValue> {
    if let Some(s) = value.as_str() {
        Ok(JsonValue::String(tera::escape_html(s)))
    } else if value.is_null() {
        Ok(JsonValue::String(String::new()))
    } else {
        Err(tera::Error::msg("attempted to escape non-string non-null value"))
    }
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

fn color_to_hex_color(mut rgb: (f64, f64, f64)) -> JsonValue {
    rgb = (
        rgb.0.clamp(0.0, 1.0),
        rgb.1.clamp(0.0, 1.0),
        rgb.2.clamp(0.0, 1.0),
    );

    let r_byte = (rgb.0 * 255.0).round() as u8;
    let g_byte = (rgb.1 * 255.0).round() as u8;
    let b_byte = (rgb.2 * 255.0).round() as u8;

    JsonValue::String(format!("#{:02X}{:02X}{:02X}", r_byte, g_byte, b_byte))
}

fn get_arg<'a>(args: &'a HashMap<String, JsonValue>, key: &str, func: &str) -> tera::Result<&'a JsonValue> {
    match args.get(key) {
        Some(a) => Ok(a),
        None => Err(tera::Error::msg(format!("missing {} in {}", key, func))),
    }
}

fn get_str_arg<'a>(args: &'a HashMap<String, JsonValue>, key: &str, func: &str) -> tera::Result<&'a str> {
    let json = get_arg(args, key, func)?;
    match json.as_str() {
        Some(s) => Ok(s),
        None => Err(tera::Error::msg(format!("{} in {} not a string", key, func))),
    }
}

fn get_f64_arg(args: &HashMap<String, JsonValue>, key: &str, func: &str) -> tera::Result<f64> {
    let json = get_arg(args, key, func)?;
    match json.as_f64() {
        Some(f) => Ok(f),
        None => Err(tera::Error::msg(format!("{} in {} not an f64", key, func))),
    }
}

fn mix_color(args: &HashMap<String, JsonValue>) -> tera::Result<JsonValue> {
    let min_value = get_f64_arg(args, "min_value", "mix_color")?;
    let max_value = get_f64_arg(args, "max_value", "mix_color")?;
    let value = get_f64_arg(args, "value", "mix_color")?;

    let min_color_str = get_str_arg(args, "min_color", "mix_color")?;
    let max_color_str = get_str_arg(args, "max_color", "mix_color")?;

    let min_color = match hex_color_to_color(min_color_str) {
        Some(mc) => mc,
        None => return Err(tera::Error::msg("invalid min_color in mix_color")),
    };
    let max_color = match hex_color_to_color(max_color_str) {
        Some(mc) => mc,
        None => return Err(tera::Error::msg("invalid max_color in mix_color")),
    };

    if value < min_value {
        return Ok(color_to_hex_color(min_color));
    }
    if value > max_value {
        return Ok(color_to_hex_color(max_color));
    }

    // lerp
    let value_pos = (value - min_value) / (max_value - min_value);
    let my_color = (
        min_color.0 + value_pos * (max_color.0 - min_color.0),
        min_color.1 + value_pos * (max_color.1 - min_color.1),
        min_color.2 + value_pos * (max_color.2 - min_color.2),
    );

    Ok(color_to_hex_color(my_color))
}

pub(crate) fn augment_tera(tera: &mut Tera) {
    tera.register_filter("percentify", percentify);
    tera.register_filter("maybe_escape", maybe_escape);
    tera.register_function("mix_color", mix_color);
}
