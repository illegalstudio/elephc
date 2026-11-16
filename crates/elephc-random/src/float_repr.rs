//! Purpose:
//! Formats a `double` exactly like php-src's `%.*H` conversion prints it.
//!
//! Called from:
//! - `elephc_random_float_repr`, the C-ABI helper the random prelude uses for its
//!   lossy seed-conversion deprecation messages.
//!
//! Key details:
//! - php-src passes precision `-1` to `zend_gcvt`, which prints the SHORTEST decimal form
//!   that round-trips and switches to uppercase exponential notation outside
//!   `[-4, 17)` decimal-point positions. Rust's `{:e}` supplies the same shortest digits, so
//!   this only re-implements zend_gcvt's layout rules over them.
//! - Infinity and NaN spell `INF` / `NAN` (with a sign for infinity), matching php-src.

/// Returns the digits and decimal-point position of the shortest round-trip form.
///
/// `digits` has no sign, decimal point, or trailing zeros; the value is
/// `0.<digits> * 10^decpt`, as `zend_dtoa` reports it.
fn shortest_digits(value: f64) -> (String, i32) {
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("Rust's exponential formatting always writes an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("Rust's exponential formatting writes an integer exponent");
    let mut digits: String = mantissa.chars().filter(|character| *character != '.').collect();
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    (digits, exponent + 1)
}

/// Formats `value` the way php-src prints a float in `%.*H` diagnostics.
pub fn php_float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NAN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-INF" } else { "INF" }.to_string();
    }
    let negative = value.is_sign_negative();
    let magnitude = value.abs();
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if magnitude == 0.0 {
        out.push('0');
        return out;
    }
    let (digits, decpt) = shortest_digits(magnitude);
    const SHORTEST_PRECISION: i32 = 17;
    if (decpt >= 0 && decpt > SHORTEST_PRECISION) || decpt < -3 {
        let exponent = decpt - 1;
        let (sign, exponent) = if exponent < 0 {
            ('-', -exponent)
        } else {
            ('+', exponent)
        };
        out.push(digits.as_bytes()[0] as char);
        out.push('.');
        if digits.len() > 1 {
            out.push_str(&digits[1..]);
        } else {
            out.push('0');
        }
        out.push('E');
        out.push(sign);
        out.push_str(&exponent.to_string());
    } else if decpt < 0 {
        out.push_str("0.");
        for _ in 0..-decpt {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        for index in 0..decpt as usize {
            out.push(digits.as_bytes().get(index).copied().unwrap_or(b'0') as char);
        }
        if (decpt as usize) < digits.len() {
            if decpt == 0 {
                out.push('0');
            }
            out.push('.');
            out.push_str(&digits[decpt as usize..]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::php_float_repr;

    /// Verifies the formatter against `printf("%.*H", -1, $value)` from PHP 8.5.10.
    #[test]
    fn shortest_float_repr_matches_php_gcvt() {
        for (value, expected) in [
            (1.5, "1.5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e-7, "1.0E-7"),
            (1e20, "1.0E+20"),
            (1e16, "10000000000000000"),
            (1e16 + 2.0, "10000000000000002"),
            (0.0001, "0.0001"),
            (0.00001, "1.0E-5"),
            (f64::MAX, "1.7976931348623157E+308"),
            (-0.5, "-0.5"),
            (123_456_789.25, "123456789.25"),
            (0.0, "0"),
            (-0.0, "-0"),
            (f64::NAN, "NAN"),
            (f64::INFINITY, "INF"),
            (f64::NEG_INFINITY, "-INF"),
        ] {
            assert_eq!(php_float_repr(value), expected, "{value:?}");
        }
    }
}
