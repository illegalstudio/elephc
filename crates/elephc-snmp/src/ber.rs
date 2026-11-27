//! Purpose:
//! Minimal BER/DER encoder and decoder for SNMP v1/v2c messages.
//!
//! Called from:
//! - `crate::client` for request encoding and response parsing.
//!
//! Key details:
//! - Only the subset SNMP needs is implemented: INTEGER, OCTET STRING, NULL,
//!   OID, SEQUENCE, and the SNMP application/context tags.
//! - Lengths use BER short/long form; OIDs use the standard 40*X+Y first
//!   subidentifier and base-128 continuation bytes.

/// Encodes a BER length.
pub(crate) fn encode_length(length: usize) -> Vec<u8> {
    if length < 0x80 {
        return vec![length as u8];
    }
    let mut bytes = Vec::new();
    let mut value = length;
    while value > 0 {
        bytes.push((value & 0xff) as u8);
        value >>= 8;
    }
    bytes.reverse();
    let mut out = vec![0x80 | bytes.len() as u8];
    out.extend_from_slice(&bytes);
    out
}

/// Wraps `body` in one BER TLV with `tag`.
pub(crate) fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 4 + body.len());
    out.push(tag);
    out.extend_from_slice(&encode_length(body.len()));
    out.extend_from_slice(body);
    out
}

/// Encodes a signed integer with minimal two's-complement bytes.
pub(crate) fn encode_integer(value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut start = 0;
    while start < 7 {
        let current = bytes[start];
        let next = bytes[start + 1];
        if (current == 0x00 && next & 0x80 == 0) || (current == 0xff && next & 0x80 != 0) {
            start += 1;
        } else {
            break;
        }
    }
    tlv(0x02, &bytes[start..])
}

/// Encodes an OID string (with or without a leading dot) into a BER OID body.
pub(crate) fn encode_oid(oid: &str) -> Option<Vec<u8>> {
    let oid = oid.strip_prefix('.').unwrap_or(oid);
    let arcs: Vec<u64> = oid
        .split('.')
        .map(|arc| arc.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if arcs.len() < 2 || arcs[0] > 2 || (arcs[0] < 2 && arcs[1] > 39) {
        return None;
    }
    let mut body = Vec::new();
    encode_base128(arcs[0] * 40 + arcs[1], &mut body);
    for arc in &arcs[2..] {
        encode_base128(*arc, &mut body);
    }
    Some(tlv(0x06, &body))
}

/// Encodes one base-128 subidentifier.
fn encode_base128(mut value: u64, out: &mut Vec<u8>) {
    let mut bytes = vec![(value & 0x7f) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push(0x80 | (value & 0x7f) as u8);
        value >>= 7;
    }
    bytes.reverse();
    out.extend_from_slice(&bytes);
}

/// Decodes a BER OID body into a dotted string with a leading dot.
pub(crate) fn decode_oid(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let mut arcs = Vec::new();
    let mut value = 0u64;
    let mut started = false;
    for &byte in body {
        if !started && byte == 0x80 {
            return None;
        }
        started = true;
        value = value.checked_shl(7)?.checked_add(u64::from(byte & 0x7f))?;
        if byte & 0x80 == 0 {
            if arcs.is_empty() {
                let (first, second) = if value < 40 {
                    (0, value)
                } else if value < 80 {
                    (1, value - 40)
                } else {
                    (2, value - 80)
                };
                arcs.push(first);
                arcs.push(second);
            } else {
                arcs.push(value);
            }
            value = 0;
            started = false;
        }
    }
    if started {
        return None;
    }
    Some(format!(
        ".{}",
        arcs.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
    ))
}

/// A borrowed TLV reader.
pub(crate) struct Reader<'a> {
    /// Remaining bytes.
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Creates a reader over `data`.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Reads one TLV, returning `(tag, body)`.
    pub(crate) fn read_tlv(&mut self) -> Option<(u8, &'a [u8])> {
        let tag = *self.data.first()?;
        let (length, header) = decode_length(self.data.get(1..)?)?;
        let start = 1 + header;
        let end = start.checked_add(length)?;
        let body = self.data.get(start..end)?;
        self.data = self.data.get(end..)?;
        Some((tag, body))
    }

    /// Returns the remaining bytes.
    pub(crate) fn remaining(&self) -> &'a [u8] {
        self.data
    }
}

/// Decodes a BER length from `data`, returning `(length, bytes consumed)`.
fn decode_length(data: &[u8]) -> Option<(usize, usize)> {
    let first = *data.first()?;
    if first & 0x80 == 0 {
        return Some((usize::from(first), 1));
    }
    let count = usize::from(first & 0x7f);
    if count == 0 || count > std::mem::size_of::<usize>() {
        return None;
    }
    let mut length = 0usize;
    for &byte in data.get(1..1 + count)? {
        length = length.checked_shl(8)?.checked_add(usize::from(byte))?;
    }
    Some((length, 1 + count))
}

/// Decodes a signed integer body.
pub(crate) fn decode_integer(body: &[u8]) -> Option<i64> {
    if body.is_empty() || body.len() > 8 {
        return None;
    }
    let mut value = if body[0] & 0x80 != 0 { -1i64 } else { 0 };
    for &byte in body {
        value = value.checked_shl(8)?.checked_add(i64::from(byte))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes and decodes a representative OID.
    #[test]
    fn oid_round_trip() {
        let encoded = encode_oid(".1.3.6.1.2.1.1.1.0").unwrap();
        let (tag, body) = Reader::new(&encoded).read_tlv().unwrap();
        assert_eq!(tag, 0x06);
        assert_eq!(decode_oid(body).as_deref(), Some(".1.3.6.1.2.1.1.1.0"));
    }

    /// Encodes/decodes positive and negative integers.
    #[test]
    fn integer_round_trip() {
        for value in [0, 1, 127, 128, 255, 256, 65535, -1, -128, -129] {
            let encoded = encode_integer(value);
            let (tag, body) = Reader::new(&encoded).read_tlv().unwrap();
            assert_eq!(tag, 0x02);
            assert_eq!(decode_integer(body), Some(value));
        }
    }

    /// Long-form lengths decode correctly.
    #[test]
    fn long_length_round_trip() {
        let body = vec![0u8; 300];
        let encoded = tlv(0x30, &body);
        let (tag, decoded) = Reader::new(&encoded).read_tlv().unwrap();
        assert_eq!(tag, 0x30);
        assert_eq!(decoded, body.as_slice());
    }
}
