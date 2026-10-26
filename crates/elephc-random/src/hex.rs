//! Purpose:
//! Little-endian hex codec for engine state and CSPRNG byte payloads.
//!
//! Called from:
//! - The engine state serializers and `elephc_random_bytes_hex`.
//!
//! Key details:
//! - php-src serializes engine state with `php_random_bin2hex_le`, which prints the bytes
//!   in memory order. Every supported elephc target is little-endian, so `to_le_bytes`
//!   reproduces the same text without a target-specific byte swap.

/// Encodes raw bytes as lowercase hex.
pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Encodes a `u32` the way php-src's little-endian state dumps do.
pub fn encode_u32(value: u32) -> String {
    encode(&value.to_le_bytes())
}

/// Encodes a `u64` the way php-src's little-endian state dumps do.
pub fn encode_u64(value: u64) -> String {
    encode(&value.to_le_bytes())
}

/// Decodes an even-length lowercase or uppercase hex string, rejecting any other byte.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = nibble(pair[0])?;
        let low = nibble(pair[1])?;
        out.push((high << 4) | low);
    }
    Some(out)
}

/// Maps one ASCII hex digit to its value.
fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
