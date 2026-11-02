//! Purpose:
//! Shares PHP Reflection's scalar dump formatting between AOT metadata and eval.
//!
//! Called from:
//! - The compiler's callable dump renderer and Magician's Reflection formatter.
//!
//! Key details:
//! - Reflection uses 14 significant decimal digits and bytewise printable string escapes.
//! - `union_member_rank` is the one table both engines order a union's members with, so the
//!   dump, the `ReflectionUnionType` object and `getTypes()` agree with each other (#1118).
//! - These helpers depend on neither backend nor runtime cells.

/// Formats a float using PHP Reflection's default precision, exponent range and spelling.
pub fn float(value: f64) -> String {
    if value.is_nan() { return "NAN".into(); }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-INF" } else { "INF" }.into();
    }
    // Round before choosing the notation: rounding can carry into the next exponent.
    let rounded = format!("{:.13e}", value.abs());
    let (mantissa, exponent) = rounded.split_once('e').expect("scientific float");
    let exponent: i32 = exponent.parse().expect("decimal exponent");
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if !(-4..14).contains(&exponent) {
        let fraction = if digits.len() > 1 { &digits[1..] } else { "0" };
        return format!("{sign}{}.{fraction}E{exponent:+}", &digits[..1]);
    }
    let decimal = exponent + 1;
    if decimal <= 0 {
        return format!("{sign}0.{}{digits}", "0".repeat((-decimal) as usize));
    }
    let decimal = decimal as usize;
    if decimal >= digits.len() {
        format!("{sign}{digits}{}.0", "0".repeat(decimal - digits.len()))
    } else {
        format!("{sign}{}.{}", &digits[..decimal], &digits[decimal..])
    }
}

/// Quotes a string using PHP Reflection's bytewise escapes, preserving literal quote marks.
pub fn string(value: &[u8]) -> String {
    let mut result = String::from("'");
    for &byte in value {
        match byte {
            b'\t' => result.push_str("\\t"), b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"), 0x0b => result.push_str("\\v"),
            0x0c => result.push_str("\\f"), 0x1b => result.push_str("\\e"),
            b'\\' => result.push_str("\\\\"), 0x20..=0x7e => result.push(byte as char),
            _ => result.push_str(&format!("\\x{byte:02X}")),
        }
    }
    result.push('\'');
    result
}

/// Renders one named type, including PHP's expansion of the iterable alias.
pub fn named_type(name: &str, allows_null: bool) -> String {
    if name == "iterable" {
        return if allows_null { "Traversable|array|null" } else { "Traversable|array" }.into();
    }
    if allows_null && !matches!(name, "mixed" | "null") {
        format!("?{name}")
    } else {
        name.to_string()
    }
}

/// Sorts a union in PHP's type-mask order while preserving the order of class names.
pub fn union(mut names: Vec<String>, allows_null: bool) -> String {
    if let Some(index) = names.iter().position(|name| name == "iterable") {
        names.splice(index..=index, ["Traversable".into(), "array".into()]);
    }
    if allows_null && !names.iter().any(|name| name == "null") { names.push("null".into()); }
    names.sort_by_key(|name| union_member_rank(name));
    names.dedup();
    names.join("|")
}

/// Ranks one union member name the way PHP orders them when printing a union.
///
/// PHP renders a union from its internal type mask, which has a fixed order, so `int|string`
/// prints as `string|int`. Derived by measurement against PHP 8.5.10:
///
/// | declared | printed |
/// | --- | --- |
/// | `int\|string` | `string\|int` |
/// | `bool\|array\|string` | `array\|string\|bool` |
/// | `C\|int\|string` | `C\|string\|int` |
/// | `int\|C\|I` | `C\|I\|int` |
/// | `false\|int` | `int\|false` |
/// | `array\|object\|string` | `object\|array\|string` |
/// | `C\|float\|int` | `C\|int\|float` |
///
/// Class names come first and keep THEIR declared order among themselves, which a stable sort on
/// this rank preserves because they all share rank 0. The compiler sorts its `ReflectionUnionType`
/// metadata with this rank, and so does the interpreter for reflection inside `eval()`, so the
/// type object and `getTypes()` answer in the same order as the dumps rendered by [`union`].
pub fn union_member_rank(name: &str) -> u8 {
    match name {
        "static" => 1,
        "callable" => 2,
        "object" => 3,
        "array" => 4,
        "iterable" => 5,
        "string" => 6,
        "int" => 7,
        "float" => 8,
        "bool" => 9,
        "false" => 10,
        "true" => 11,
        "null" => 12,
        // A class, interface or enum name, or a type this table does not know: PHP prints those
        // ahead of the built-ins, in the order they were written.
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    /// Rounding controls the notation boundary and preserves negative zero and non-finite values.
    #[test]
    fn reflection_float_precision_and_boundaries() {
        for (value, expected) in [
            (0.30000000000000004, "0.3"), (1e14, "1.0E+14"),
            (1e13, "10000000000000.0"), (1e-4, "0.0001"), (1e-5, "1.0E-5"),
            (99999999999999.9, "1.0E+14"), (0.00009999999999999999, "0.0001"),
            (-0.0, "-0.0"), (f64::INFINITY, "INF"), (f64::NEG_INFINITY, "-INF"),
            (f64::NAN, "NAN"),
        ] {
            assert_eq!(super::float(value), expected, "{value:?}");
        }
    }

    /// Control bytes, UTF-8 and quotes follow PHP's printable dump representation.
    #[test]
    fn reflection_string_escapes() {
        assert_eq!(super::string(b"\xff\0\n\t\\'"), "'\\xFF\\x00\\n\\t\\\\''");
        assert_eq!(super::string("é".as_bytes()), "'\\xC3\\xA9'");
    }

    /// The rank table reproduces the member order PHP 8.5.10 prints for the measured unions.
    #[test]
    fn reflection_union_member_order() {
        for (declared, expected) in [
            ("int|string", "string|int"),
            ("bool|array|string", "array|string|bool"),
            ("C|int|string", "C|string|int"),
            ("int|C|I", "C|I|int"),
            ("false|int", "int|false"),
            ("array|object|string", "object|array|string"),
            ("C|float|int", "C|int|float"),
            ("iterable|int", "Traversable|array|int"),
        ] {
            let names = declared.split('|').map(str::to_string).collect();
            assert_eq!(super::union(names, false), expected, "{declared}");
        }
        assert_eq!(super::union(vec!["int".into(), "string".into()], true), "string|int|null");
    }
}
