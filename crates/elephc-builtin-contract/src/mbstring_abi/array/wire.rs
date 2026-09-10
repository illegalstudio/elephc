//! Purpose:
//! Encodes and validates the version-one mbstring array graph byte format.
//!
//! Called from:
//! - ArrayGraph::encode/decode at native, eval, and shared-engine boundaries.
//!
//! Key details:
//! - All words are little-endian u64; no host pointers, padding, or alignment is exported.
//! - A root/count header precedes nodes, each with an entry count and ordered key/value cells.
//! - Cells contain tag/payload words followed immediately by string bytes when applicable.
//! - Counts are bounded by remaining bytes before allocation; complete framing is required.

use super::{ArrayGraph, Key, Value};
use crate::mbstring_abi::{ARG_NULL, ARG_INT, ARG_STRING, ARG_BOOL, ARG_ARRAY};

/// Raw IEEE-754 bits, copied without floating-point evaluation or canonicalization.
const FLOAT: u64 = 5;
/// Object/resource values rejected by recursive PHP mbstring consumers.
const UNSUPPORTED: u64 = 6;

impl ArrayGraph {
    /// Packs a validated graph into one allocation without walking recursively through aliases.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        word(&mut bytes, self.root as u64);
        word(&mut bytes, self.arrays.len() as u64);
        for entries in &self.arrays {
            word(&mut bytes, entries.len() as u64);
            for (key, value) in entries {
                match key {
                    Key::Int(value) => cell(&mut bytes, ARG_INT, *value as u64),
                    Key::String(value) => string(&mut bytes, value),
                }
                match value {
                    Value::Null => cell(&mut bytes, ARG_NULL, 0),
                    Value::Bool(value) => cell(&mut bytes, ARG_BOOL, *value as u64),
                    Value::Int(value) => cell(&mut bytes, ARG_INT, *value as u64),
                    Value::Float(value) => cell(&mut bytes, FLOAT, *value),
                    Value::String(value) => string(&mut bytes, value),
                    Value::Array(value) => cell(&mut bytes, ARG_ARRAY, *value as u64),
                    Value::Unsupported => cell(&mut bytes, UNSUPPORTED, 0),
                }
            }
        }
        bytes
    }

    /// Rejects truncated, noncanonical, dangling, duplicate-key, or trailing wire data.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader(bytes);
        let root = usize::try_from(reader.word()?).ok()?;
        let count = usize::try_from(reader.word()?).ok()?;
        if count > reader.0.len() / 8 || root >= count { return None; }
        let mut arrays = Vec::with_capacity(count);
        for _ in 0..count {
            let count = usize::try_from(reader.word()?).ok()?;
            if count > reader.0.len() / 32 { return None; }
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let key = match reader.value()? {
                    Value::Int(value) => Key::Int(value),
                    Value::String(value) => Key::String(value),
                    _ => return None,
                };
                entries.push((key, reader.value()?));
            }
            arrays.push(entries);
        }
        if !reader.0.is_empty() { return None; }
        Self::new(root, arrays)
    }
}

/// Appends one unaligned little-endian word to the owned wire buffer.
fn word(bytes: &mut Vec<u8>, value: u64) { bytes.extend_from_slice(&value.to_le_bytes()); }

/// Appends the two-word header shared by every key and value cell.
fn cell(bytes: &mut Vec<u8>, tag: u64, value: u64) { word(bytes, tag); word(bytes, value); }

/// Appends a binary string with an exact byte length and no terminator or padding.
fn string(bytes: &mut Vec<u8>, value: &[u8]) {
    cell(bytes, ARG_STRING, value.len() as u64);
    bytes.extend_from_slice(value);
}

/// A remaining byte range, consumed only after each complete field has been validated.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    /// Reads one word without assuming pointer alignment or native endianness.
    fn word(&mut self) -> Option<u64> {
        let value = u64::from_le_bytes(self.0.get(..8)?.try_into().ok()?);
        self.0 = &self.0[8..];
        Some(value)
    }

    /// Reads one scalar or graph reference, checking reserved payloads and string lengths.
    fn value(&mut self) -> Option<Value> {
        let tag = self.word()?;
        let payload = self.word()?;
        Some(match tag {
            ARG_NULL if payload == 0 => Value::Null,
            ARG_INT => Value::Int(payload as i64),
            ARG_STRING => {
                let length = usize::try_from(payload).ok()?;
                let bytes = self.0.get(..length)?.to_vec();
                self.0 = &self.0[length..];
                Value::String(bytes)
            }
            ARG_BOOL if payload <= 1 => Value::Bool(payload != 0),
            ARG_ARRAY => Value::Array(usize::try_from(payload).ok()?),
            FLOAT => Value::Float(payload),
            UNSUPPORTED if payload == 0 => Value::Unsupported,
            _ => return None,
        })
    }
}
