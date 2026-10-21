//! Purpose:
//! Convert a complete php/php_binary session into one serialized array so decoding
//! preserves a shared reference table and rejects truncated session records.
//!
//! Called from:
//! - The generated `__elephc_session_decode` prelude through the byte C ABI.
//!
//! Key details:
//! - The synthetic root adds one reference-table entry; R/r IDs are shifted by one.
//! - String contents, class names and custom serialized payloads remain opaque bytes.
//! - Returned bytes use the existing owned publication buffer, never input storage.

use super::{input_bytes, parse_digits, publish_bytes, skip_serialized_value};

/// Moves reference IDs past the synthetic array root without inspecting opaque payloads.
fn shifted_references(data: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(data.len());
    let mut position = 0;
    while position < data.len() {
        match data[position] {
            b's' | b'C' => {
                let end = skip_serialized_value(data, position);
                if end == position {
                    return None;
                }
                result.extend_from_slice(&data[position..end]);
                position = end;
            }
            b'O' => {
                let start = position + 2;
                let digits = data.get(start..)?.iter().position(|byte| *byte == b':')? + start;
                let length = parse_digits(data, start, digits)?;
                let end = digits.checked_add(length)?.checked_add(4)?;
                result.extend_from_slice(data.get(position..end)?);
                position = end;
            }
            b'R' | b'r' => {
                let start = position + 2;
                let end = data.get(start..)?.iter().position(|byte| *byte == b';')? + start;
                if !data[start..end].iter().all(u8::is_ascii_digit) {
                    return None;
                }
                let reference = parse_digits(data, start, end)?;
                if reference == 0 {
                    return None;
                }
                result.push(data[position]);
                result.extend_from_slice(format!(":{};", reference.checked_add(1)?).as_bytes());
                position = end + 1;
            }
            byte => {
                result.push(byte);
                position += 1;
            }
        }
    }
    Some(result)
}

/// Frames every complete entry and rejects any malformed suffix instead of accepting a prefix.
fn decode_array(data: &[u8], binary: bool) -> Option<Vec<u8>> {
    let mut position = 0;
    let mut count = 0;
    let mut body = Vec::new();
    while position < data.len() {
        let (start, end) = if binary {
            let start = position + 1;
            (start, start.checked_add((data[position] & 0x7f) as usize)?)
        } else {
            let end = data[position..].iter().position(|byte| *byte == b'|')? + position;
            (position, end)
        };
        let key = data.get(start..end)?;
        let value_start = end + usize::from(!binary);
        let value_end = skip_serialized_value(data, value_start);
        if value_end == value_start {
            return None;
        }
        body.extend_from_slice(format!("s:{}:\"", key.len()).as_bytes());
        body.extend_from_slice(key);
        body.extend_from_slice(b"\";");
        body.extend_from_slice(&shifted_references(data.get(value_start..value_end)?)?);
        count += 1;
        position = value_end;
    }
    let mut result = format!("a:{count}:{{").into_bytes();
    result.extend_from_slice(&body);
    result.push(b'}');
    Some(result)
}

/// Publishes one complete array encoding, or an empty buffer to report invalid framing.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_decode_array_bytes(
    data_ptr: *const u8,
    data_len: i64,
    binary: i64,
) -> i64 {
    let value = decode_array(input_bytes(data_ptr, data_len), binary != 0);
    publish_bytes(value.as_deref().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-entry references must share a table without rewriting reference-shaped strings.
    #[test]
    fn references_shift_only_outside_opaque_values() {
        let input = b"a|a:1:{i:0;i:1;}b|R:1;c|s:4:\"R:1;\";";
        assert_eq!(decode_array(input, false).unwrap(),
            b"a:3:{s:1:\"a\";a:1:{i:0;i:1;}s:1:\"b\";R:2;s:1:\"c\";s:4:\"R:1;\";}");
        assert_eq!(decode_array(b"a|O:4:\"Test\":1:{s:1:\"x\";R:1;}", false).unwrap(),
            b"a:1:{s:1:\"a\";O:4:\"Test\":1:{s:1:\"x\";R:2;}}");
    }

    /// Every incomplete record must fail, even when it follows a valid entry.
    #[test]
    fn incomplete_session_suffix_is_rejected() {
        for data in [b"broken".as_slice(), b"x|", b"x|i:1;broken", b"x|R:0;"] {
            assert!(decode_array(data, false).is_none(), "{data:?}");
        }
        assert_eq!(decode_array(b"", false).unwrap(), b"a:0:{}");
        assert!(decode_array(b"\x01xi:1;\x05ab", true).is_none());
    }

    /// Binary keys include embedded NULs and use php-src's masked length prefix.
    #[test]
    fn binary_framing_preserves_keys_and_reference_identity() {
        assert_eq!(decode_array(b"\x03a\0bb:0;\x81zr:1;", true).unwrap(),
            b"a:2:{s:3:\"a\0b\";b:0;s:1:\"z\";r:2;}");
    }
}
