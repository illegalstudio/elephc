//! Purpose:
//! Implements PHP INI boolean, replacement-codepoint, and signed quantity parsing.
//!
//! Called from:
//! - The shared mbstring INI handlers for flags, substitution, and regex limits.
//!
//! Key details:
//! - Quantity diagnostics preserve escaped binary bytes and PHP's overflow/suffix ordering.
//! - Replacement values follow the baseline C strtol conversion and retain all 32 payload bits.

/// Recognizes the whitespace accepted by Zend's quantity parser and C numeric conversion.
fn space(byte: u8) -> bool { matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12) }

/// Reads one ASCII digit if it belongs to the selected integer base.
fn digit(byte: u8, base: u32) -> Option<u64> {
    let value = match byte { b'0'..=b'9' => byte - b'0', b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10, _ => return None };
    (u32::from(value) < base).then_some(u64::from(value))
}

/// Consumes unsigned digits, saturating at ULONG_MAX while still finding the complete digit run.
fn unsigned(input: &[u8], mut position: usize, base: u32) -> (u64, usize, bool) {
    let mut value = 0_u64;
    let mut overflow = false;
    while let Some(next) = input.get(position).and_then(|&byte| digit(byte, base)) {
        if !overflow {
            match value.checked_mul(base as u64).and_then(|value| value.checked_add(next)) {
                Some(next) => value = next,
                None => { overflow = true; value = u64::MAX; },
            }
        }
        position += 1;
    }
    (value, position, overflow)
}

/// Returns a C signed-long conversion and its end pointer, including baseline binary prefixes.
fn signed_prefix(input: &[u8], auto_base: bool) -> (i64, usize) {
    let mut position = 0;
    while input.get(position).is_some_and(|&byte| space(byte)) { position += 1; }
    let negative = input.get(position) == Some(&b'-');
    if input.get(position).is_some_and(|byte| matches!(byte, b'+' | b'-')) { position += 1; }
    let mut base = 10;
    if auto_base && input.get(position) == Some(&b'0') {
        base = 8;
        if let Some(&prefix) = input.get(position + 1) {
            let candidate = match prefix { b'x' | b'X' => 16, b'b' | b'B' => 2, _ => 0 };
            if candidate != 0 && input.get(position + 2).is_some_and(|&byte| digit(byte, candidate).is_some()) {
                base = candidate;
                position += 2;
            }
        }
    }
    let (magnitude, end, overflow) = unsigned(input, position, base);
    if end == position { return (0, 0); }
    let limit = i64::MAX as u64 + u64::from(negative);
    let value = if overflow || magnitude > limit { if negative { i64::MIN } else { i64::MAX } }
        else if negative { magnitude.wrapping_neg() as i64 } else { magnitude as i64 };
    (value, end)
}

/// Parses a substitution code only when C strtol ends at the first NUL terminator.
pub(super) fn substitute(input: &[u8]) -> Option<u32> {
    let input = input.split(|&byte| byte == 0).next().unwrap_or_default();
    let (value, end) = signed_prefix(input, true);
    (end == input.len()).then_some(value as u32)
}

/// Applies Zend's three textual true values followed by C atoi's signed-int conversion.
pub(super) fn boolean(input: &[u8]) -> bool {
    input.eq_ignore_ascii_case(b"true") || input.eq_ignore_ascii_case(b"yes") || input.eq_ignore_ascii_case(b"on")
        || signed_prefix(input, false).0 as i32 != 0
}

/// Makes control and non-ASCII bytes visible exactly as Zend's escaped quantity diagnostics do.
fn escaped(input: &[u8]) -> String {
    let mut output = String::new();
    for &byte in input {
        match byte {
            b'\n' => output.push_str("\\n"), b'\r' => output.push_str("\\r"), b'\t' => output.push_str("\\t"),
            12 => output.push_str("\\f"), 11 => output.push_str("\\v"), 27 => output.push_str("\\e"),
            b'\\' => output.push_str("\\\\"), 32..=126 => output.push(char::from(byte)),
            _ => output.push_str(&format!("\\x{byte:02X}")),
        }
    }
    output
}

/// Detects a second C numeric prefix that Zend forbids after its explicit 0x/0o/0b prefix.
fn repeated_prefix(input: &[u8], position: usize, end: usize, base: u32) -> bool {
    if position >= end { return true; }
    if space(input[position]) || matches!(input[position], b'+' | b'-') { return true; }
    input[position] == b'0' && position + 1 < end && match input[position + 1] {
        b'x' | b'X' | b'o' | b'O' => true, b'b' | b'B' => base != 16, _ => false,
    }
}

/// Parses a signed Zend INI quantity while preserving suffix precedence over overflow warnings.
pub(in crate::state) fn quantity(input: &[u8]) -> (i64, Option<Vec<u8>>) {
    let mut start = 0;
    let mut end = input.len();
    while start < end && space(input[start]) { start += 1; }
    while end > start && space(input[end - 1]) { end -= 1; }
    if start == end { return (0, None); }
    let invalid = escaped(input);
    let no_digits = |reason: &str| (0, Some(format!("Invalid quantity \"{invalid}\": {reason}, interpreting as \"0\" for backwards compatibility").into_bytes()));
    let negative = input[start] == b'-';
    if matches!(input[start], b'+' | b'-') { start += 1; }
    if start == end || !input[start].is_ascii_digit() { return no_digits("no valid leading digits"); }
    let mut base = if input[start] == b'0' { 8 } else { 10 };
    if input[start] == b'0' && (start + 1 == end || !input[start + 1].is_ascii_digit()) {
        if start + 1 == end { return (0, None); }
        match input[start + 1] {
            b'g' | b'G' | b'm' | b'M' | b'k' | b'K' => {},
            prefix @ (b'x' | b'X' | b'o' | b'O' | b'b' | b'B') => {
                base = match prefix { b'x' | b'X' => 16, b'o' | b'O' => 8, _ => 2 };
                start += 2;
                if repeated_prefix(input, start, end, base) { return no_digits("no digits after base prefix"); }
            },
            byte => {
                let mut warning = b"Invalid prefix \"0".to_vec();
                warning.push(byte);
                warning.extend_from_slice(b"\", interpreting as \"0\" for backwards compatibility");
                return (0, Some(warning));
            },
        }
    }
    let (mut value, mut cursor, mut overflow) = unsigned(input, start, base);
    if cursor == start { return no_digits("no valid leading digits"); }
    if !overflow {
        if negative && value == (1_u64 << 63) { value = value.wrapping_neg(); }
        else if (value as i64) < 0 { overflow = true; }
        else if negative { value = value.wrapping_neg(); }
    }
    while cursor < end && space(input[cursor]) { cursor += 1; }
    if cursor != end {
        let prefix = escaped(&input[..cursor]);
        let suffix = escaped(&input[end - 1..end]);
        let shift = match input[end - 1] { b'G' | b'g' => 30, b'M' | b'm' => 20, b'K' | b'k' => 10,
            _ => return (value as i64, Some(format!("Invalid quantity \"{invalid}\": unknown multiplier \"{suffix}\", interpreting as \"{prefix}\" for backwards compatibility").into_bytes())),
        };
        overflow |= (value as i64).checked_mul(1_i64 << shift).is_none();
        value = value.wrapping_shl(shift);
        if cursor != end - 1 {
            return (value as i64, Some(format!("Invalid quantity \"{invalid}\", interpreting as \"{prefix}{suffix}\" for backwards compatibility").into_bytes()));
        }
    }
    (value as i64, overflow.then(|| format!("Invalid quantity \"{invalid}\": value is out of range, using overflow result for backwards compatibility").into_bytes()))
}

/// Parses Core display_errors through textual modes and PHP's unsigned-byte C integer conversion.
pub(in crate::state) fn display_errors(input: &[u8]) -> u8 {
    if [b"on".as_slice(), b"yes", b"true", b"stdout"].iter().any(|name| input.eq_ignore_ascii_case(name)) { return 1; }
    if input.eq_ignore_ascii_case(b"stderr") { return 2; }
    let mode = signed_prefix(input, false).0 as u8;
    if mode > 2 { 1 } else { mode }
}
