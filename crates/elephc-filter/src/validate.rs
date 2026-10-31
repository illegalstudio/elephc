//! Purpose:
//! Ports php-src `ext/filter/logical_filters.c` validation rules exactly.
//!
//! Called from:
//! - `crate::elephc_filter_apply` for the scalar validation filter ids.
//!
//! Key details:
//! - The parsers reject the same inputs as php-src, including overflow, leading-zero IPv4
//!   octets, malformed IPv6 compression and RFC 6890 special-purpose ranges.
//! - Integer validation uses signed 64-bit checked arithmetic on every supported target.

/// FILTER_FLAG_ALLOW_OCTAL.
const ALLOW_OCTAL: i64 = 0x0001;
/// FILTER_FLAG_ALLOW_HEX.
const ALLOW_HEX: i64 = 0x0002;
/// FILTER_FLAG_ALLOW_THOUSAND.
const ALLOW_THOUSAND: i64 = 0x2000;
/// FILTER_FLAG_HOSTNAME.
const HOSTNAME: i64 = 0x100000;
/// FILTER_FLAG_IPV4.
const IPV4: i64 = 0x00100000;
/// FILTER_FLAG_IPV6.
const IPV6: i64 = 0x00200000;
/// FILTER_FLAG_NO_RES_RANGE.
const NO_RES_RANGE: i64 = 0x00400000;
/// FILTER_FLAG_NO_PRIV_RANGE.
const NO_PRIV_RANGE: i64 = 0x00800000;
/// FILTER_FLAG_GLOBAL_RANGE.
const GLOBAL_RANGE: i64 = 0x20000000;

/// Trims PHP's default filter whitespace (not form feed) from both ends.
pub(crate) fn trim_default(mut value: &[u8]) -> &[u8] {
    while let Some((first, rest)) = value.split_first() {
        if matches!(first, b' ' | b'\t' | b'\r' | 0x0b | b'\n') { value = rest; } else { break; }
    }
    while let Some((last, rest)) = value.split_last() {
        if matches!(last, b' ' | b'\t' | b'\r' | 0x0b | b'\n') { value = rest; } else { break; }
    }
    value
}

/// Parses one decimal integer with php-src's sign, leading-zero and overflow rules.
pub(crate) fn int(value: &[u8], flags: i64) -> Option<i64> {
    let value = trim_default(value);
    if value.is_empty() { return None; }
    let mut index = 0usize;
    let mut negative = false;
    match value[0] {
        b'-' => { negative = true; index = 1; }
        b'+' => index = 1,
        _ => {}
    }
    if index < value.len() && value[index] == b'0' && index + 1 == value.len() { return Some(0); }
    if index >= value.len() || !(b'1'..=b'9').contains(&value[index]) {
        // The leading-zero branch only reaches here for 0x/0o or a longer run.
        if index < value.len() && value[index] == b'0' {
            return if index == 0 { prefixed_int(&value[index..], flags) } else { None };
        }
        return None;
    }
    let mut result = i64::from(value[index] - b'0');
    if negative { result = -result; }
    index += 1;
    while index < value.len() {
        let digit = value[index].wrapping_sub(b'0');
        if digit > 9 { return None; }
        let digit = i64::from(digit);
        result = if negative {
            result.checked_mul(10)?.checked_sub(digit)?
        } else {
            result.checked_mul(10)?.checked_add(digit)?
        };
        index += 1;
    }
    Some(result)
}

/// Parses `0x` hex or `0o`/bare octal input after the leading zero.
fn prefixed_int(value: &[u8], flags: i64) -> Option<i64> {
    // `value` starts with the leading zero consumed by php_filter_int.
    let rest = &value[1..];
    if flags & ALLOW_HEX != 0 && matches!(rest.first(), Some(b'x' | b'X')) {
        let digits = &rest[1..];
        if digits.is_empty() { return None; }
        return parse_radix(digits, 16);
    }
    if flags & ALLOW_OCTAL != 0 {
        let digits = match rest.first() {
            Some(b'o' | b'O') => {
                let digits = &rest[1..];
                if digits.is_empty() { return None; }
                digits
            }
            _ => rest,
        };
        return parse_radix(digits, 8);
    }
    if rest.is_empty() { Some(0) } else { None }
}

/// Parses unsigned digits in one radix, rejecting values above `i64::MAX`.
fn parse_radix(digits: &[u8], radix: u64) -> Option<i64> {
    let mut value = 0u64;
    for &byte in digits {
        let digit = match byte {
            b'0'..=b'9' => u64::from(byte - b'0'),
            b'a'..=b'f' => u64::from(byte - b'a') + 10,
            b'A'..=b'F' => u64::from(byte - b'A') + 10,
            _ => return None,
        };
        if digit >= radix { return None; }
        value = value.checked_mul(radix)?.checked_add(digit)?;
    }
    Some(value as i64)
}

/// Ports `php_filter_boolean`'s case-insensitive truth table.
pub(crate) fn boolean(value: &[u8]) -> Option<bool> {
    let value = trim_default(value);
    let lower = value.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    match lower.as_slice() {
        b"" | b"0" | b"no" | b"off" | b"false" => Some(false),
        b"1" | b"yes" | b"on" | b"true" => Some(true),
        _ => None,
    }
}

/// Ports `php_filter_float`'s separator, exponent, overflow and underflow checks.
pub(crate) fn float(
    value: &[u8],
    flags: i64,
    decimal: Option<u8>,
    thousand: Option<&[u8]>,
) -> Option<f64> {
    let value = trim_default(value);
    let decimal = decimal.unwrap_or(b'.');
    let decimal = if decimal.is_ascii() { decimal } else { b'.' };
    let thousand: &[u8] = thousand.unwrap_or(b"',.");
    let mut normalized = Vec::with_capacity(value.len());
    let mut index = 0usize;
    if let Some(sign @ (b'+' | b'-')) = value.first().copied() {
        normalized.push(sign);
        index = 1;
    }
    let mut first = true;
    loop {
        let mut digits = 0usize;
        while index < value.len() && value[index].is_ascii_digit() {
            normalized.push(value[index]);
            index += 1;
            digits += 1;
        }
        if index == value.len() || value[index] == decimal || matches!(value[index], b'e' | b'E') {
            if !first && digits != 3 { return None; }
            if index < value.len() && value[index] == decimal {
                normalized.push(b'.');
                index += 1;
                while index < value.len() && value[index].is_ascii_digit() {
                    normalized.push(value[index]);
                    index += 1;
                }
            }
            if index < value.len() && matches!(value[index], b'e' | b'E') {
                normalized.push(b'e');
                index += 1;
                if index < value.len() && matches!(value[index], b'+' | b'-') {
                    normalized.push(value[index]);
                    index += 1;
                }
                while index < value.len() && value[index].is_ascii_digit() {
                    normalized.push(value[index]);
                    index += 1;
                }
            }
            break;
        }
        if flags & ALLOW_THOUSAND != 0 && thousand.contains(&value[index]) {
            if first {
                if !(1..=3).contains(&digits) { return None; }
            } else if digits != 3 {
                return None;
            }
            first = false;
            index += 1;
        } else {
            return None;
        }
    }
    if index != value.len() { return None; }
    let text = std::str::from_utf8(&normalized).ok()?;
    let parsed = text.parse::<f64>().ok()?;
    if !parsed.is_finite() { return None; }
    let has_scientific = normalized.contains(&b'e');
    let has_fraction = normalized.contains(&b'.');
    if !has_scientific && !has_fraction {
        // php-src keeps the integer path when the normalized text fits zend_long; the result
        // is always returned as a double, but the range comparison must use the exact integer.
        if let Ok(integer) = text.parse::<i64>() {
            let as_double = integer as f64;
            if as_double == 0.0 && text.len() > 1 && text.bytes().any(|byte| (b'1'..=b'9').contains(&byte)) {
                return None;
            }
            return Some(as_double);
        }
    }
    if parsed == 0.0 && normalized.len() > 1 && normalized.iter().any(|byte| (b'1'..=b'9').contains(byte)) {
        return None;
    }
    Some(parsed)
}

/// Returns whether one byte is ASCII alphanumeric.
fn isalnum(byte: u8) -> bool { byte.is_ascii_alphanumeric() }

/// Ports `php_filter_validate_domain_ex`, including hostname mode and label limits.
pub(crate) fn domain(value: &[u8], flags: i64) -> bool {
    let hostname = flags & HOSTNAME != 0;
    let mut end = value.len();
    if end > 0 && value[end - 1] == b'.' { end -= 1; }
    if end > 253 { return false; }
    if value.is_empty() { return !hostname; }
    if value.get(0) == Some(&b'.') || (hostname && value.first().is_some_and(|byte| !isalnum(*byte))) {
        return false;
    }
    let mut index = 0usize;
    let mut label = 1u8;
    while index < end {
        let byte = value[index];
        if byte == b'.' {
            let next = value.get(index + 1).copied();
            let previous_is_alnum = index > 0 && isalnum(value[index - 1]);
            if next == Some(b'.')
                || (hostname
                    && (!previous_is_alnum || next.is_none_or(|next| !isalnum(next))))
            {
                return false;
            }
            label = 1;
        } else {
            // php-src tests the byte after the label edge against the full string's NUL, so a
            // trailing hyphen is accepted only when a trailing dot follows it.
            let at_end = index + 1 == value.len();
            if label > 63 || (hostname && (byte != b'-' || at_end) && !isalnum(byte)) {
                return false;
            }
            label += 1;
        }
        index += 1;
    }
    true
}

/// Parses one IPv4 address, rejecting leading zeroes and out-of-range octets.
fn ipv4(value: &[u8]) -> Option<[i32; 4]> {
    let mut octets = [0i32; 4];
    let mut count = 0usize;
    let mut index = 0usize;
    while index < value.len() {
        if !value[index].is_ascii_digit() { return None; }
        let leading_zero = value[index] == b'0';
        let mut digits = 1usize;
        let mut number = i32::from(value[index] - b'0');
        index += 1;
        while index < value.len() && value[index].is_ascii_digit() {
            number = number * 10 + i32::from(value[index] - b'0');
            index += 1;
            digits += 1;
            if number > 255 || digits > 3 { return None; }
        }
        if leading_zero && (number != 0 || digits > 1) { return None; }
        octets[count] = number;
        count += 1;
        if count == 4 { return (index == value.len()).then_some(octets); }
        if index >= value.len() || value[index] != b'.' { return None; }
        index += 1;
    }
    None
}

/// Ports `_php_filter_validate_ipv6`, including embedded IPv4 and `::` expansion.
pub(crate) fn ipv6(value: &[u8], mut ip: Option<&mut [i32; 8]>) -> bool {
    if !value.contains(&b':') { return false; }
    let mut end = value.len();
    let mut blocks = 0usize;
    let mut compressed = None;
    let mut embedded_ipv4 = None;
    if let Some(dot) = value.iter().position(|byte| *byte == b'.') {
        let mut start = dot;
        while start > 0 && value[start - 1] != b':' { start -= 1; }
        let parsed = match ipv4(&value[start..]) { Some(parsed) => parsed, None => return false };
        end = start;
        if end < 2 { return false; }
        if value[end - 2] != b':' { end -= 1; }
        blocks = 2;
        embedded_ipv4 = Some(parsed);
    }
    let mut index = 0usize;
    while index < end {
        if value[index] == b':' {
            index += 1;
            if index >= end { return false; }
            if value[index] == b':' {
                if compressed.is_some() { return false; }
                if let Some(ip) = ip.as_deref_mut() {
                    if blocks < 8 { ip[blocks] = -1; }
                }
                compressed = Some(blocks);
                blocks += 1;
                index += 1;
                if index == end {
                    if blocks > 8 { return false; }
                    break;
                }
            } else if index == 1 {
                return false;
            }
        }
        let mut number = 0u32;
        let mut digits = 0usize;
        while index < end {
            let digit = match value[index] {
                b'0'..=b'9' => u32::from(value[index] - b'0'),
                b'a'..=b'f' => u32::from(value[index] - b'a') + 10,
                b'A'..=b'F' => u32::from(value[index] - b'A') + 10,
                _ => break,
            };
            number = number.wrapping_mul(16).wrapping_add(digit);
            digits += 1;
            index += 1;
        }
        if let Some(ip) = ip.as_deref_mut() {
            if blocks < 8 { ip[blocks] = number as i32; }
        }
        if !(1..=4).contains(&digits) { return false; }
        blocks += 1;
        if blocks > 8 { return false; }
    }
    if let Some(ip) = ip.as_deref_mut() {
        if let Some(ip4) = embedded_ipv4 {
            for slot in ip.iter_mut() { *slot = 0; }
            ip[5] = 0xffff;
            ip[6] = 256 * ip4[0] + ip4[1];
            ip[7] = 256 * ip4[2] + ip4[3];
        } else if let Some(position) = compressed {
            if blocks <= 8 {
                let offset = 8 - blocks;
                for cursor in (position + offset + 1..8).rev() {
                    ip[cursor] = ip[cursor - offset];
                }
                for cursor in (position..=position + offset).rev() {
                    ip[cursor] = 0;
                }
            }
        }
    }
    (compressed.is_some() && blocks <= 8) || blocks == 8
}

/// Returns php-src's RFC 6890 status flags for one IPv4 address.
fn ipv4_status(ip: &[i32; 8]) -> Option<(bool, bool, bool)> {
    let mut global = false;
    let mut reserved = false;
    let mut private = false;
    if ip[0] == 0 { reserved = true; }
    else if ip[0] == 10 { private = true; }
    else if ip[0] == 100 && (64..=127).contains(&ip[1]) { }
    else if ip[0] == 127 { reserved = true; }
    else if ip[0] == 169 && ip[1] == 254 { reserved = true; }
    else if ip[0] == 172 && (16..=31).contains(&ip[1]) { private = true; }
    else if ip[0] == 192 && ip[1] == 0 && ip[2] == 0 { }
    else if ip[0] == 192 && ip[1] == 0 && ip[2] == 2 { }
    else if ip[0] == 192 && ip[1] == 88 && ip[2] == 99 { global = true; }
    else if ip[0] == 192 && ip[1] == 168 { private = true; }
    else if ip[0] == 198 && (18..=19).contains(&ip[1]) { }
    else if ip[0] == 198 && ip[1] == 51 && ip[2] == 100 { }
    else if ip[0] == 203 && ip[1] == 0 && ip[2] == 113 { }
    else if ip[0] >= 240 && ip[1] <= 255 { reserved = true; }
    else { return None; }
    Some((global, reserved, private))
}

/// Returns php-src's RFC 6890 status flags for one IPv6 address.
fn ipv6_status(ip: &[i32; 8]) -> Option<(bool, bool, bool)> {
    let mut global = false;
    let mut reserved = false;
    let mut private = false;
    if *ip == [0; 8] { reserved = true; }
    else if ip[..7] == [0; 7] && ip[7] == 1 { reserved = true; }
    else if ip[0] == 0x0064 && ip[1] == 0xff9b { global = true; }
    else if ip[..5] == [0; 5] && ip[5] == 0xffff { reserved = true; }
    else if ip[0] == 0x0100 && ip[1..4] == [0; 3] { }
    else if ip[0] == 0x2001 && ip[1] == 0x0000 { }
    else if ip[0] == 0x2001 && ip[1] <= 0x01ff { }
    else if ip[0] == 0x2001 && ip[1] == 0x0002 && ip[2] == 0 { }
    else if ip[0] == 0x2001 && ip[1] == 0x0db8 { }
    else if ip[0] == 0x2001 && (0x0010..=0x001f).contains(&ip[1]) { }
    else if ip[0] == 0x2002 { }
    else if (0xfc00..=0xfdff).contains(&ip[0]) { private = true; }
    else if (0xfe80..=0xfebf).contains(&ip[0]) { reserved = true; }
    else { return None; }
    Some((global, reserved, private))
}

/// Ports `php_filter_validate_ip` for either address family and range flags.
pub(crate) fn ip(value: &[u8], flags: i64) -> bool {
    let mode6 = value.contains(&b':');
    let mode4 = value.contains(&b'.');
    if !mode6 && !mode4 { return false; }
    if flags & IPV4 != 0 && flags & IPV6 != 0 {
        // Both families accepted; the delimiter picks the parser.
    } else if flags & IPV4 != 0 && mode6 {
        return false;
    } else if flags & IPV6 != 0 && !mode6 {
        return false;
    }
    let status = if mode6 {
        let mut parsed = [0i32; 8];
        if !ipv6(value, Some(&mut parsed)) { return false; }
        ipv6_status(&parsed)
    } else {
        let Some(octets) = ipv4(value) else { return false; };
        let mut parsed = [0i32; 8];
        parsed[..4].copy_from_slice(&octets);
        ipv4_status(&parsed)
    };
    let Some((global, reserved, private)) = status else { return true; };
    if flags & GLOBAL_RANGE != 0 && !global { return false; }
    if flags & NO_PRIV_RANGE != 0 && private { return false; }
    if flags & NO_RES_RANGE != 0 && reserved { return false; }
    true
}

/// Ports `php_filter_validate_mac` for EUI-64, hyphen and colon formats.
pub(crate) fn mac(value: &[u8], separator_set: bool, separator: &[u8]) -> bool {
    let (tokens, length, expected) = match value.len() {
        14 => (3usize, 4usize, b'.'),
        17 if value[2] == b'-' => (6, 2, b'-'),
        17 if value[2] == b':' => (6, 2, b':'),
        _ => return false,
    };
    if separator_set && separator.first().copied() != Some(expected) { return false; }
    for token in 0..tokens {
        let offset = token * (length + 1);
        if token + 1 < tokens && value[offset + length] != expected { return false; }
        if parse_radix(&value[offset..offset + length], 16).is_none() { return false; }
    }
    true
}
