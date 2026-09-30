//! Purpose:
//! php's `scanf` scanner for the interpreter — the same engine the compiled backend runs as an
//! injected elephc-PHP prelude, so `eval('sscanf(...)')` and a compiled `sscanf()` cannot answer
//! differently.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting::sscanf`.
//! - `crate::interpreter::builtins::filesystem::fscanf`, which feeds it one line at a time.
//!
//! Key details:
//! - Every rule here was measured against `php -n` 8.5.6; the subset this file replaces knew only
//!   `%d`, `%f`, `%s` and `%%`, pushed each match back as the matched STRING (so `%d` gave
//!   `'77'` where php gives `77`), used `''` where php uses `NULL`, and produced a SHORTER array
//!   than the format's conversion count.
//! - `scan` returns `Ok(None)` for php's null result: the scan reached END OF INPUT before
//!   assigning anything. That is a different outcome from a failed conversion, which yields an
//!   array whose entry is `Null` — `sscanf('', '%d')` is `NULL` while `sscanf('abc', '%d')` is
//!   `[NULL]`, and `sscanf('-', '%d')` is `NULL` because the sign was consumed and the input ran
//!   out, while `sscanf('- 5', '%d')` is `[NULL]` because a space, not the end, stopped it.
//! - Scanning STOPS at the first failure; every conversion the format still carries contributes
//!   a `Null`, so the array length is a property of the FORMAT alone.
//! - The format is validated in full even after scanning stops, which is why the trailing loop
//!   re-parses specifiers it will never run: `sscanf('x', '%d%q')` raises php's
//!   `Bad scan conversion character "q"` even though `%d` already failed.

/// One value a conversion produced, in php's own types.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::interpreter) enum ScanfValue {
    /// `%d`/`%i`/`%o`/`%x`/`%X`/`%n`, and `%u` while it fits.
    Int(i64),
    /// `%e`/`%E`/`%f`/`%g`.
    Float(f64),
    /// `%s`/`%c`/`%[...]`, and `%u` past `PHP_INT_MAX`.
    Bytes(Vec<u8>),
    /// A conversion that did not match, or one the scan never reached.
    Null,
}

/// A format php refuses outright, carrying its verbatim message.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::interpreter) struct ScanfFormatError {
    /// php's own wording for this rejection.
    pub(in crate::interpreter) message: String,
}

/// php's `ULONG_MAX` in decimal, the ceiling `%u` saturates at.
const ULONG_MAX_DECIMAL: &str = "18446744073709551615";

/// Returns whether a byte is one php's scanner treats as whitespace.
fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Returns a byte's value as a base-36 digit, or 99 when it is not one.
fn digit_value(byte: u8) -> u32 {
    match byte {
        b'0'..=b'9' => u32::from(byte - b'0'),
        b'a'..=b'z' => u32::from(byte - b'a') + 10,
        b'A'..=b'Z' => u32::from(byte - b'A') + 10,
        _ => 99,
    }
}

/// Returns whether a byte is a conversion character php's `scanf` accepts.
fn is_conversion(byte: u8) -> bool {
    matches!(
        byte,
        b'c' | b'd' | b'D' | b'e' | b'E' | b'f' | b'g' | b'i' | b'n' | b'o' | b's' | b'u' | b'x'
            | b'X'
    )
}

/// Renders `2**64 - digits` in decimal, the unsigned reading php gives a negative `%u`.
///
/// The subtraction runs on decimal digits because `2**64` does not fit any integer type php
/// exposes, and the result is a STRING for exactly that reason.
fn unsigned_negative(digits: &[u8]) -> Vec<u8> {
    let minuend = b"18446744073709551616";
    let mut out = Vec::new();
    let mut borrow = 0i32;
    for index in 0..minuend.len() {
        let left = i32::from(minuend[minuend.len() - 1 - index] - b'0');
        let right = digits
            .len()
            .checked_sub(index + 1)
            .map_or(0, |position| i32::from(digits[position] - b'0'));
        let mut digit = left - right - borrow;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(b'0' + u8::try_from(digit).unwrap_or(0));
    }
    out.reverse();
    let trimmed = out.iter().position(|byte| *byte != b'0').unwrap_or(out.len() - 1);
    out[trimmed..].to_vec()
}

/// Reads a `%u` token as php does: a 64-bit UNSIGNED value saturating at `ULONG_MAX`, returned
/// as an int while it fits `i64::MAX` and as a decimal string beyond it.
fn unsigned_value(digits: &[u8], negative: bool) -> ScanfValue {
    let trimmed = digits.iter().position(|byte| *byte != b'0');
    let normalized: &[u8] = match trimmed {
        Some(start) => &digits[start..],
        None => b"0",
    };
    let saturated = normalized.len() > ULONG_MAX_DECIMAL.len()
        || (normalized.len() == ULONG_MAX_DECIMAL.len()
            && normalized > ULONG_MAX_DECIMAL.as_bytes());
    if saturated {
        return ScanfValue::Bytes(ULONG_MAX_DECIMAL.as_bytes().to_vec());
    }
    if negative {
        if normalized == b"0" {
            return ScanfValue::Int(0);
        }
        return ScanfValue::Bytes(unsigned_negative(normalized));
    }
    let int_max = i64::MAX.to_string();
    if normalized.len() < int_max.len()
        || (normalized.len() == int_max.len() && normalized <= int_max.as_bytes())
    {
        let text = String::from_utf8_lossy(normalized);
        if let Ok(value) = text.parse::<i64>() {
            return ScanfValue::Int(value);
        }
    }
    ScanfValue::Bytes(normalized.to_vec())
}

/// Scans one integer token, returning the value when the conversion matched.
fn scan_int(
    input: &[u8],
    cursor: &mut usize,
    width: usize,
    conversion: u8,
) -> Option<ScanfValue> {
    let start = *cursor;
    let mut negative = false;
    let mut signed = false;
    if matches!(input.get(*cursor), Some(b'-') | Some(b'+')) {
        signed = true;
        negative = input[*cursor] == b'-';
        *cursor += 1;
    }
    // php only detects a base prefix when NO sign was consumed, which is why
    // `sscanf('-0x10', '%i')` answers 0: it scans `-0` and stops at the `x`.
    let hex_prefix = !signed
        && input.get(*cursor) == Some(&b'0')
        && matches!(input.get(*cursor + 1), Some(b'x') | Some(b'X'))
        && input.get(*cursor + 2).is_some_and(|byte| digit_value(*byte) < 16);
    let mut base = 10u32;
    match conversion {
        b'x' | b'X' => {
            base = 16;
            if hex_prefix {
                *cursor += 2;
            }
        }
        b'o' => base = 8,
        b'i' => {
            if hex_prefix {
                base = 16;
                *cursor += 2;
            } else if input.get(*cursor) == Some(&b'0') {
                base = 8;
            }
        }
        _ => {}
    }
    let mut digits = Vec::new();
    while *cursor < input.len() {
        if width > 0 && *cursor - start >= width {
            break;
        }
        let value = digit_value(input[*cursor]);
        if value >= base {
            break;
        }
        digits.push(input[*cursor]);
        *cursor += 1;
    }
    if digits.is_empty() {
        return None;
    }
    if conversion == b'u' {
        return Some(unsigned_value(&digits, negative));
    }
    let mut magnitude: i64 = 0;
    let mut overflow = false;
    for digit in &digits {
        let value = i64::from(digit_value(*digit));
        if magnitude > (i64::MAX - value) / i64::from(base) {
            overflow = true;
            break;
        }
        magnitude = magnitude * i64::from(base) + value;
    }
    if overflow {
        return Some(ScanfValue::Int(if negative { i64::MIN } else { i64::MAX }));
    }
    Some(ScanfValue::Int(if negative { -magnitude } else { magnitude }))
}

/// Scans one float token, backing off to the longest prefix that is a number.
///
/// The back-off is what makes `sscanf('1.5e', '%f')` answer `1.5`: the `e` is consumed while
/// scanning but is not part of the last valid prefix, so the cursor rewinds to just after `1.5`.
fn scan_float(input: &[u8], cursor: &mut usize, width: usize) -> Option<ScanfValue> {
    let start = *cursor;
    let mut text = Vec::new();
    let mut best: Option<(Vec<u8>, usize)> = None;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exponent = false;
    while *cursor < input.len() {
        if width > 0 && *cursor - start >= width {
            break;
        }
        let byte = input[*cursor];
        if byte.is_ascii_digit() {
            seen_digit = true;
            text.push(byte);
            *cursor += 1;
            best = Some((text.clone(), *cursor));
            continue;
        }
        if byte == b'.' && !seen_dot && !seen_exponent {
            seen_dot = true;
            text.push(byte);
            *cursor += 1;
            if seen_digit {
                best = Some((text.clone(), *cursor));
            }
            continue;
        }
        if matches!(byte, b'e' | b'E') && seen_digit && !seen_exponent {
            seen_exponent = true;
            text.push(byte);
            *cursor += 1;
            continue;
        }
        let tail = text.last().copied();
        if matches!(byte, b'-' | b'+') && (text.is_empty() || matches!(tail, Some(b'e') | Some(b'E')))
        {
            text.push(byte);
            *cursor += 1;
            continue;
        }
        break;
    }
    let (best_text, best_end) = best?;
    *cursor = best_end;
    String::from_utf8_lossy(&best_text)
        .parse::<f64>()
        .ok()
        .map(ScanfValue::Float)
}

/// Expands a `%[...]` set body into the bytes it accepts.
fn class_members(body: &[u8]) -> Vec<u8> {
    let mut members = Vec::new();
    let mut index = 0;
    while index < body.len() {
        let byte = body[index];
        if byte == b'-' && index > 0 && index + 1 < body.len() {
            let from = body[index - 1];
            let to = body[index + 1];
            if to >= from {
                for member in from..=to {
                    members.push(member);
                }
                index += 2;
                continue;
            }
        }
        members.push(byte);
        index += 1;
    }
    members
}

/// One parsed conversion specifier: its flags and where the format continues.
struct Specifier {
    /// `*` was present, so the conversion consumes input and stores nothing.
    suppress: bool,
    /// Maximum bytes the conversion may consume, or zero for no bound.
    width: usize,
    /// The conversion character, or NUL when the format ended mid-specifier.
    conversion: u8,
    /// Expanded `%[...]` membership, when the conversion is a class.
    class: Vec<u8>,
    /// Whether a `%[^...]` class is negated.
    negated: bool,
}

/// Parses one specifier starting just after its `%`, advancing `index` past it.
fn parse_specifier(format: &[u8], index: &mut usize) -> Result<Specifier, ScanfFormatError> {
    let mut suppress = false;
    if format.get(*index) == Some(&b'*') {
        suppress = true;
        *index += 1;
    }
    let mut width = 0usize;
    while let Some(byte) = format.get(*index) {
        if !byte.is_ascii_digit() {
            break;
        }
        width = width * 10 + usize::from(byte - b'0');
        *index += 1;
    }
    if matches!(format.get(*index), Some(b'l') | Some(b'h') | Some(b'L')) {
        *index += 1;
    }
    let conversion = format.get(*index).copied().unwrap_or(0);
    *index += 1;
    let mut class = Vec::new();
    let mut negated = false;
    if conversion == b'[' {
        let mut body = Vec::new();
        if format.get(*index) == Some(&b'^') {
            negated = true;
            *index += 1;
        }
        if format.get(*index) == Some(&b']') {
            body.push(b']');
            *index += 1;
        }
        let mut closed = false;
        while *index < format.len() {
            if format[*index] == b']' {
                closed = true;
                *index += 1;
                break;
            }
            body.push(format[*index]);
            *index += 1;
        }
        if !closed {
            return Err(ScanfFormatError {
                message: "Unmatched [ in format string".to_string(),
            });
        }
        class = class_members(&body);
    } else if conversion != b'%' && !is_conversion(conversion) {
        return Err(ScanfFormatError {
            message: format!(
                "Bad scan conversion character \"{}\"",
                String::from_utf8_lossy(&[conversion])
            ),
        });
    }
    Ok(Specifier {
        suppress,
        width,
        conversion,
        class,
        negated,
    })
}

/// Scans `input` against `format`, answering php's `sscanf()` result.
///
/// `Ok(None)` is php's null result — end of input before anything was assigned. `Ok(Some(v))`
/// carries one entry per non-suppressed conversion in the whole format, matched or not.
pub(in crate::interpreter) fn scan(
    input: &[u8],
    format: &[u8],
) -> Result<Option<Vec<ScanfValue>>, ScanfFormatError> {
    let mut values = Vec::new();
    let mut cursor = 0usize;
    let mut index = 0usize;
    let mut assigned = 0usize;
    let mut at_eof = false;

    // Every failing arm `break`s, so no separate "stopped" flag is needed here: only the
    // end-of-input case is read again, to tell php's null result from an array of nulls.
    while index < format.len() {
        let byte = format[index];
        if is_space(byte) {
            index += 1;
            while cursor < input.len() && is_space(input[cursor]) {
                cursor += 1;
            }
            continue;
        }
        if byte != b'%' {
            if cursor >= input.len() {
                at_eof = true;
                break;
            }
            if input[cursor] != byte {
                break;
            }
            cursor += 1;
            index += 1;
            continue;
        }
        index += 1;
        let specifier = parse_specifier(format, &mut index)?;

        if specifier.conversion == b'%' {
            if cursor >= input.len() {
                at_eof = true;
                break;
            }
            if input[cursor] != b'%' {
                break;
            }
            cursor += 1;
            continue;
        }

        if specifier.conversion == b'n' {
            assigned += 1;
            if !specifier.suppress {
                values.push(ScanfValue::Int(i64::try_from(cursor).unwrap_or(i64::MAX)));
            }
            continue;
        }

        if specifier.conversion != b'c' && specifier.conversion != b'[' {
            while cursor < input.len() && is_space(input[cursor]) {
                cursor += 1;
            }
        }
        if cursor >= input.len() {
            at_eof = true;
            if !specifier.suppress {
                values.push(ScanfValue::Null);
            }
            break;
        }

        let scanned = match specifier.conversion {
            b's' => {
                let start = cursor;
                while cursor < input.len() && !is_space(input[cursor]) {
                    if specifier.width > 0 && cursor - start >= specifier.width {
                        break;
                    }
                    cursor += 1;
                }
                (cursor > start).then(|| ScanfValue::Bytes(input[start..cursor].to_vec()))
            }
            b'c' => {
                let take = if specifier.width > 0 { specifier.width } else { 1 };
                let start = cursor;
                while cursor < input.len() && !is_space(input[cursor]) && cursor - start < take {
                    cursor += 1;
                }
                Some(ScanfValue::Bytes(input[start..cursor].to_vec()))
            }
            b'[' => {
                let start = cursor;
                while cursor < input.len() {
                    if specifier.width > 0 && cursor - start >= specifier.width {
                        break;
                    }
                    let mut inside = specifier.class.contains(&input[cursor]);
                    if specifier.negated {
                        inside = !inside;
                    }
                    if !inside {
                        break;
                    }
                    cursor += 1;
                }
                (cursor > start).then(|| ScanfValue::Bytes(input[start..cursor].to_vec()))
            }
            b'e' | b'E' | b'f' | b'g' => scan_float(input, &mut cursor, specifier.width),
            conversion => scan_int(input, &mut cursor, specifier.width, conversion),
        };

        let Some(value) = scanned else {
            if cursor >= input.len() {
                at_eof = true;
            }
            if !specifier.suppress {
                values.push(ScanfValue::Null);
            }
            break;
        };
        assigned += 1;
        if !specifier.suppress {
            values.push(value);
        }
    }

    // php validates the WHOLE format and fills a placeholder per remaining conversion, so a
    // bad specifier past the stopping point still raises and the array length stays a property
    // of the format rather than of how far the input got.
    while index < format.len() {
        if format[index] != b'%' {
            index += 1;
            continue;
        }
        index += 1;
        let specifier = parse_specifier(format, &mut index)?;
        if specifier.conversion == b'%' {
            continue;
        }
        if !specifier.suppress {
            values.push(ScanfValue::Null);
        }
    }

    if at_eof && assigned == 0 {
        return Ok(None);
    }
    Ok(Some(values))
}
