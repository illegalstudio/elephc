//! Purpose:
//! Owns the PHP session data wire formats for `--web` mode: the `php`
//! serialize-handler parser that splits `key|serialized_value` pairs and skips
//! one complete PHP `serialize()` value of any type, the `php_binary`
//! serialize-handler parser (`chr(strlen(key)).key.serialize(value)`), and the
//! C-ABI entry accessors the web prelude uses to iterate parsed entries in
//! either format.
//!
//! Called from:
//! - The compiled `--web` web prelude via the `elephc_web_session_count_entries`
//!   / `_entry_key` / `_entry_value` C-ABI symbols (php handler), and the
//!   `_count_entries_bin` / `_entry_key_bin` / `_entry_value_bin` symbols
//!   (php_binary handler), used to decode session data into `$_SESSION` on
//!   `session_start`. `php_serialize` is decoded entirely in the prelude via
//!   `unserialize()` — it needs no bridge parser.
//!
//! Key details:
//! - One process per prefork worker, single-threaded, so the `RET_STRING`
//!   return buffer (owned by `state`) is race-free across calls.
//! - `skip_serialized_value` understands all PHP serialize types (`N`, `b`,
//!   `i`, `d`, `s`, `a`, `O`, `C`) and recurses for arrays/objects; it never
//!   panics on malformed input, returning the original position instead. Both
//!   the `php` and `php_binary` parsers reuse it for the value grammar — the
//!   two formats only differ in how each entry's key is framed.

use std::ffi::{c_char, CStr};

use super::state::{input_bytes, opt_ptr, publish_bytes, set_cstr, RET_STRING};

/// Parses ASCII decimal digits at `data[start..end]` into a `usize`. Returns
/// `None` if the slice is not valid ASCII decimal or empty.
fn parse_digits(data: &[u8], start: usize, end: usize) -> Option<usize> {
    std::str::from_utf8(&data[start..end])
        .ok()?
        .parse::<usize>()
        .ok()
}

/// Skips one complete PHP serialized value starting at byte position `pos` in
/// `data`, returning the byte position immediately after the value. Understands
/// all PHP serialize types: `N`, `b`, `i`, `d`, `s`, `a`, `O`, `C`. Returns the
/// original `pos` (no advancement) on invalid or truncated input.
fn skip_serialized_value(data: &[u8], pos: usize) -> usize {
    if pos >= data.len() {
        return pos;
    }
    match data[pos] {
        b'N' => {
            // N; — null
            if pos + 1 < data.len() && data[pos + 1] == b';' {
                pos + 2
            } else {
                pos
            }
        }
        b'b' => {
            // b:0; or b:1;
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // Skip the digit (0 or 1).
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b'i' => {
            // i:<number>;
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // Optional sign.
            if p < data.len() && (data[p] == b'+' || data[p] == b'-') {
                p += 1;
            }
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b'd' => {
            // d:<float>;
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // Skip the float body until ';'.
            while p < data.len() && data[p] != b';' {
                p += 1;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b's' => {
            // s:<len>:"<bytes>";
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            let len_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(slen) = parse_digits(data, len_start, p) else {
                return pos;
            };
            // Expect :"<bytes>";
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            // Skip slen bytes (the string body).
            if p + slen > data.len() {
                return pos;
            }
            p += slen;
            // Expect ";
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b';' {
                p + 1
            } else {
                pos
            }
        }
        b'a' => {
            // a:<count>:{ key value key value ... }
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            let count_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(count) = parse_digits(data, count_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'{' {
                p += 1;
            } else {
                return pos;
            }
            // Skip count*2 serialized values (keys + values). Reject an
            // overflowing declared count and require every nested value to
            // advance from the current cursor; comparing with the outer `pos`
            // lets a truncated payload spin for the attacker-controlled count.
            let Some(item_count) = count.checked_mul(2) else {
                return pos;
            };
            for _ in 0..item_count {
                let next = skip_serialized_value(data, p);
                if next == p {
                    return pos;
                }
                p = next;
            }
            if p < data.len() && data[p] == b'}' {
                p + 1
            } else {
                pos
            }
        }
        b'O' => {
            // O:<namelen>:"<name>":<count>:{ key value ... }
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // namelen
            let nl_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(namelen) = parse_digits(data, nl_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p + namelen > data.len() {
                return pos;
            }
            p += namelen;
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // count
            let count_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(count) = parse_digits(data, count_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'{' {
                p += 1;
            } else {
                return pos;
            }
            let Some(item_count) = count.checked_mul(2) else {
                return pos;
            };
            for _ in 0..item_count {
                let next = skip_serialized_value(data, p);
                if next == p {
                    return pos;
                }
                p = next;
            }
            if p < data.len() && data[p] == b'}' {
                p + 1
            } else {
                pos
            }
        }
        b'C' => {
            // C:<namelen>:"<name>":<datalen>:{<data>}
            let mut p = pos + 1;
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // namelen
            let nl_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(namelen) = parse_digits(data, nl_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p + namelen > data.len() {
                return pos;
            }
            p += namelen;
            if p < data.len() && data[p] == b'"' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            // datalen
            let dl_start = p;
            while p < data.len() && data[p].is_ascii_digit() {
                p += 1;
            }
            let Some(datalen) = parse_digits(data, dl_start, p) else {
                return pos;
            };
            if p < data.len() && data[p] == b':' {
                p += 1;
            } else {
                return pos;
            }
            if p < data.len() && data[p] == b'{' {
                p += 1;
            } else {
                return pos;
            }
            // Skip datalen bytes.
            if p + datalen > data.len() {
                return pos;
            }
            p += datalen;
            if p < data.len() && data[p] == b'}' {
                p + 1
            } else {
                pos
            }
        }
        _ => pos, // Unknown type: no advancement.
    }
}

/// Parses the session data format (`key|serialized_value` pairs) and returns a
/// list of `(key_bytes, value_bytes)` slices. The key is everything before the
/// first `|`; the value is one complete serialized value after the `|`.
fn parse_session_entries(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut entries = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        // Find the '|' separator.
        let mut key_end = pos;
        while key_end < data.len() && data[key_end] != b'|' {
            key_end += 1;
        }
        if key_end >= data.len() {
            break; // No separator — incomplete entry.
        }
        let key = &data[pos..key_end];
        let val_start = key_end + 1;
        if val_start >= data.len() {
            break;
        }
        let val_end = skip_serialized_value(data, val_start);
        if val_end == val_start {
            break; // Could not parse the value.
        }
        let value = &data[val_start..val_end];
        entries.push((key, value));
        pos = val_end;
    }
    entries
}

/// Parses one php_binary-format session data buffer (§2.4): each entry is
/// `chr(strlen(key)).key.serialize(value)`, back to back with no separator —
/// php_binary keys are limited to 127 bytes so the 1-byte length prefix
/// cannot overflow into the following key/value data. Reuses
/// `skip_serialized_value` for the value grammar (identical to the `php`
/// handler).
fn parse_session_entries_binary(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut entries = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let key_len = data[pos] as usize;
        let key_start = pos + 1;
        let key_end = key_start + key_len;
        if key_end > data.len() {
            break; // Truncated key length prefix.
        }
        let key = &data[key_start..key_end];
        let val_start = key_end;
        let val_end = skip_serialized_value(data, val_start);
        if val_end == val_start {
            break; // Could not parse the value.
        }
        let value = &data[val_start..val_end];
        entries.push((key, value));
        pos = val_end;
    }
    entries
}

/// Returns the number of `chr(len)+key+serialize(value)` entries in a
/// binary-safe php_binary payload.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_count_entries_bin_bytes(
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    parse_session_entries_binary(input_bytes(data_ptr, data_len)).len() as i64
}

/// Publishes the key of a php_binary entry and returns its byte pointer.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_key_bin_bytes(
    data_ptr: *const u8,
    data_len: i64,
    idx: i64,
) -> i64 {
    let entries = parse_session_entries_binary(input_bytes(data_ptr, data_len));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or_else(Vec::new, |(key, _)| key.to_vec());
    publish_bytes(&bytes)
}

/// Publishes the serialized value of a php_binary entry and returns its pointer.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_value_bin_bytes(
    data_ptr: *const u8,
    data_len: i64,
    idx: i64,
) -> i64 {
    let entries = parse_session_entries_binary(input_bytes(data_ptr, data_len));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or_else(Vec::new, |(_, value)| value.to_vec());
    publish_bytes(&bytes)
}

/// Returns the number of `key|serialized_value` entries in a binary-safe payload.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_count_entries_bytes(
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    parse_session_entries(input_bytes(data_ptr, data_len)).len() as i64
}

/// Publishes the key of a `php` handler entry and returns its byte pointer.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_key_bytes(
    data_ptr: *const u8,
    data_len: i64,
    idx: i64,
) -> i64 {
    let entries = parse_session_entries(input_bytes(data_ptr, data_len));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or_else(Vec::new, |(key, _)| key.to_vec());
    publish_bytes(&bytes)
}

/// Publishes the serialized value of a `php` handler entry and returns its pointer.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_value_bytes(
    data_ptr: *const u8,
    data_len: i64,
    idx: i64,
) -> i64 {
    let entries = parse_session_entries(input_bytes(data_ptr, data_len));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or_else(Vec::new, |(_, value)| value.to_vec());
    publish_bytes(&bytes)
}

/// Returns a legacy C-string payload as bytes, or an empty slice for null.
unsafe fn legacy_input<'a>(data_ptr: *const c_char) -> &'a [u8] {
    if data_ptr.is_null() {
        &[]
    } else {
        CStr::from_ptr(data_ptr).to_bytes()
    }
}

/// Publishes a legacy textual entry through the shared NUL-terminated buffer.
unsafe fn publish_legacy(bytes: &[u8]) -> *const c_char {
    set_cstr(
        core::ptr::addr_of_mut!(RET_STRING),
        &String::from_utf8_lossy(bytes),
    );
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// Backward-compatible C-string counter for the `php_binary` format.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_count_entries_bin(data_ptr: *const c_char) -> i64 {
    parse_session_entries_binary(legacy_input(data_ptr)).len() as i64
}

/// Backward-compatible C-string key accessor for the `php_binary` format.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_key_bin(
    data_ptr: *const c_char,
    idx: i64,
) -> *const c_char {
    let entries = parse_session_entries_binary(legacy_input(data_ptr));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or(&[][..], |(key, _)| *key);
    publish_legacy(bytes)
}

/// Backward-compatible C-string value accessor for the `php_binary` format.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_value_bin(
    data_ptr: *const c_char,
    idx: i64,
) -> *const c_char {
    let entries = parse_session_entries_binary(legacy_input(data_ptr));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or(&[][..], |(_, value)| *value);
    publish_legacy(bytes)
}

/// Backward-compatible C-string counter for the `php` format.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_count_entries(data_ptr: *const c_char) -> i64 {
    parse_session_entries(legacy_input(data_ptr)).len() as i64
}

/// Backward-compatible C-string key accessor for the `php` format.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_key(
    data_ptr: *const c_char,
    idx: i64,
) -> *const c_char {
    let entries = parse_session_entries(legacy_input(data_ptr));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or(&[][..], |(key, _)| *key);
    publish_legacy(bytes)
}

/// Backward-compatible C-string value accessor for the `php` format.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_entry_value(
    data_ptr: *const c_char,
    idx: i64,
) -> *const c_char {
    let entries = parse_session_entries(legacy_input(data_ptr));
    let bytes = usize::try_from(idx)
        .ok()
        .and_then(|i| entries.get(i))
        .map_or(&[][..], |(_, value)| *value);
    publish_legacy(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::state::test_lock as lock;
    use super::*;

    /// Verifies the session format parser handles all PHP serialize types.
    #[test]
    fn skip_value_all_types() {
        // N;
        assert_eq!(skip_serialized_value(b"N;", 0), 2);
        // b:1;
        assert_eq!(skip_serialized_value(b"b:1;", 0), 4);
        // b:0;
        assert_eq!(skip_serialized_value(b"b:0;", 0), 4);
        // i:42;
        assert_eq!(skip_serialized_value(b"i:42;", 0), 5);
        // i:-7;
        assert_eq!(skip_serialized_value(b"i:-7;", 0), 5);
        // d:3.14;
        assert_eq!(skip_serialized_value(b"d:3.14;", 0), 7);
        // s:5:"hello";
        assert_eq!(skip_serialized_value(b"s:5:\"hello\";", 0), 12);
        // s:0:"";
        assert_eq!(skip_serialized_value(b"s:0:\"\";", 0), 7);
        // a:2:{i:0;s:1:"a";i:1;s:1:"b";}
        let arr = b"a:2:{i:0;s:1:\"a\";i:1;s:1:\"b\";}";
        assert_eq!(skip_serialized_value(arr, 0), arr.len());
        // O:3:"Foo":1:{s:3:"bar";i:1;}
        let obj = b"O:3:\"Foo\":1:{s:3:\"bar\";i:1;}";
        assert_eq!(skip_serialized_value(obj, 0), obj.len());
    }

    /// Verifies the parser skips a nested array correctly.
    #[test]
    fn skip_value_nested_array() {
        // a:1:{i:0;a:1:{i:0;i:1;}}
        let nested = b"a:1:{i:0;a:1:{i:0;i:1;}}";
        assert_eq!(skip_serialized_value(nested, 0), nested.len());
    }

    /// Regression: a truncated collection with an attacker-controlled element
    /// count must stop on the first non-advancing child instead of looping count times.
    #[test]
    fn truncated_collection_count_cannot_drive_unbounded_skip_work() {
        assert_eq!(skip_serialized_value(b"a:500000000:{", 0), 0);
        assert_eq!(skip_serialized_value(b"O:1:\"X\":500000000:{", 0), 0);
    }

    /// Verifies collection item-count multiplication rejects overflow before
    /// constructing the iteration range for hostile serialized input.
    #[test]
    fn collection_count_overflow_is_rejected() {
        let payload = format!("a:{}:{{", usize::MAX);
        assert_eq!(skip_serialized_value(payload.as_bytes(), 0), 0);
    }

    /// Verifies the session entry parser splits key|value pairs.
    #[test]
    fn parse_entries_basic() {
        // count|i:5;name|s:3:"Tom";
        let data = b"count|i:5;name|s:3:\"Tom\";";
        let entries = parse_session_entries(data);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, b"count");
        assert_eq!(entries[0].1, b"i:5;");
        assert_eq!(entries[1].0, b"name");
        assert_eq!(entries[1].1, b"s:3:\"Tom\";");
    }

    /// Verifies the C-ABI count/key/value functions work on real session data.
    #[test]
    fn count_key_value_via_c_abi() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let data = std::ffi::CString::new(b"count|i:5;name|s:3:\"Tom\";".to_vec()).unwrap();
            let data_ptr = data.as_ptr();
            let count = elephc_web_session_count_entries(data_ptr);
            assert_eq!(count, 2);
            let key0 = std::ffi::CStr::from_ptr(elephc_web_session_entry_key(data_ptr, 0));
            assert_eq!(key0.to_str().unwrap(), "count");
            let val0 = std::ffi::CStr::from_ptr(elephc_web_session_entry_value(data_ptr, 0));
            assert_eq!(val0.to_str().unwrap(), "i:5;");
            let key1 = std::ffi::CStr::from_ptr(elephc_web_session_entry_key(data_ptr, 1));
            assert_eq!(key1.to_str().unwrap(), "name");
            let val1 = std::ffi::CStr::from_ptr(elephc_web_session_entry_value(data_ptr, 1));
            assert_eq!(val1.to_str().unwrap(), "s:3:\"Tom\";");
        }
    }

    /// Verifies the C-ABI entry functions return empty for out-of-range index.
    #[test]
    fn entry_out_of_range_is_empty() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let data = std::ffi::CString::new(b"count|i:5;".to_vec()).unwrap();
            let data_ptr = data.as_ptr();
            let key = std::ffi::CStr::from_ptr(elephc_web_session_entry_key(data_ptr, 99));
            assert_eq!(key.to_str().unwrap(), "");
            let val = std::ffi::CStr::from_ptr(elephc_web_session_entry_value(data_ptr, 99));
            assert_eq!(val.to_str().unwrap(), "");
        }
    }

    /// Verifies the php_binary entry parser splits `chr(len)+key+value`
    /// entries (§2.4).
    #[test]
    fn parse_entries_binary_basic() {
        // chr(5)."count"."i:5;" . chr(4)."name"."s:3:\"Tom\";"
        let mut data = Vec::new();
        data.push(5u8);
        data.extend_from_slice(b"count");
        data.extend_from_slice(b"i:5;");
        data.push(4u8);
        data.extend_from_slice(b"name");
        data.extend_from_slice(b"s:3:\"Tom\";");
        let entries = parse_session_entries_binary(&data);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, b"count");
        assert_eq!(entries[0].1, b"i:5;");
        assert_eq!(entries[1].0, b"name");
        assert_eq!(entries[1].1, b"s:3:\"Tom\";");
    }

    /// Verifies the php_binary C-ABI count/key/value functions round-trip
    /// through the C-ABI boundary (§2.4, "php_binary round-trip" test item).
    #[test]
    fn php_binary_round_trip_via_c_abi() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let mut raw = Vec::new();
            raw.push(5u8);
            raw.extend_from_slice(b"count");
            raw.extend_from_slice(b"i:5;");
            raw.push(4u8);
            raw.extend_from_slice(b"name");
            raw.extend_from_slice(b"s:3:\"Tom\";");
            let data = std::ffi::CString::new(raw).unwrap();
            let data_ptr = data.as_ptr();

            assert_eq!(elephc_web_session_count_entries_bin(data_ptr), 2);
            let key0 = std::ffi::CStr::from_ptr(elephc_web_session_entry_key_bin(data_ptr, 0));
            assert_eq!(key0.to_str().unwrap(), "count");
            let val0 = std::ffi::CStr::from_ptr(elephc_web_session_entry_value_bin(data_ptr, 0));
            assert_eq!(val0.to_str().unwrap(), "i:5;");
            let key1 = std::ffi::CStr::from_ptr(elephc_web_session_entry_key_bin(data_ptr, 1));
            assert_eq!(key1.to_str().unwrap(), "name");
            let val1 = std::ffi::CStr::from_ptr(elephc_web_session_entry_value_bin(data_ptr, 1));
            assert_eq!(val1.to_str().unwrap(), "s:3:\"Tom\";");

            // Out-of-range index returns empty, matching the php-handler
            // accessors.
            let key_oob = std::ffi::CStr::from_ptr(elephc_web_session_entry_key_bin(data_ptr, 99));
            assert_eq!(key_oob.to_str().unwrap(), "");
        }
    }

    /// Verifies binary accessors preserve NUL bytes inside serialized strings.
    #[test]
    fn binary_safe_accessors_preserve_embedded_nul() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let data = b"bin|s:3:\"a\0b\";";
            assert_eq!(
                elephc_web_session_count_entries_bytes(data.as_ptr(), data.len() as i64),
                1
            );
            let pointer = elephc_web_session_entry_value_bytes(data.as_ptr(), data.len() as i64, 0);
            let length = super::super::state::elephc_web_session_data_len() as usize;
            assert_eq!(
                std::slice::from_raw_parts(pointer as *const u8, length),
                b"s:3:\"a\0b\";"
            );
        }
    }
}
