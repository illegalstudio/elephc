//! Purpose:
//! Reads compact generated codec mapping records without alignment assumptions.
//!
//! Called from:
//! - The legacy multibyte and mobile UTF-8 codec implementations.
//!
//! Key details:
//! - Index records are sorted little-endian key/offset pairs.

/// Finds a codepoint's payload offset in a sorted array of key/offset records.
pub(super) fn lookup(index: &[u8], key: u32) -> Option<u32> {
    let (mut low, mut high) = (0, index.len() / 8);
    while low < high {
        let mid = low + (high - low) / 2;
        match word(index, mid * 8).cmp(&key) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Some(word(index, mid * 8 + 4)),
        }
    }
    None
}

/// Reads one generated little-endian word without alignment assumptions.
pub(super) fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("generated codec word"))
}
