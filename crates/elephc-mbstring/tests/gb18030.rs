//! Purpose:
//! Cross-checks the entire GB18030 four-byte address space and Unicode-range encoder inputs.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test gb18030`.
//!
//! Key details:
//! - Both the original and 2022 revision are checked against independent PHP hashes.
//! - Unassigned addresses and prefixes consumed as two separate invalid units remain covered.

use elephc_mbstring::encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding};
use sha2::{Digest, Sha256};

/// Reconstructs the deterministic four-byte corpus without consulting codec mapping tables.
fn address(mut pointer: u32) -> [u8; 4] {
    let fourth = (pointer % 10) as u8 + 0x30;
    pointer /= 10;
    let third = (pointer % 126) as u8 + 0x81;
    pointer /= 126;
    [(pointer / 10) as u8 + 0x81, (pointer % 10) as u8 + 0x30, third, fourth]
}

/// Verifies every four-byte address and every Unicode-range codepoint for both revisions.
#[test]
fn gb18030_complete_ranges_match_php() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/gb18030.json")).unwrap();
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    for (name, expected) in fixture["encodings"].as_object().unwrap() {
        let encoding = Encoding::lookup(name.as_bytes()).unwrap();
        let mut decode_hash = Sha256::new();
        for pointer in 0..126 * 10 * 126 * 10 {
            let decoded = encoding.decode(&address(pointer));
            decode_hash.update([u8::from(decoded.is_valid())]);
            let output = UnicodeEncoding::Ucs4Be.encode(&decoded.points, substitute);
            decode_hash.update((output.len() as u32).to_le_bytes());
            decode_hash.update(output);
        }
        assert_eq!(format!("{:x}", decode_hash.finalize()), expected["decode"].as_str().unwrap(), "{name} decode");
        let mut encode_hash = Sha256::new();
        for code in 0..=0x10ffff {
            let output = encoding.encode(&[code], substitute);
            encode_hash.update((output.len() as u32).to_le_bytes());
            encode_hash.update(output);
        }
        assert_eq!(format!("{:x}", encode_hash.finalize()), expected["encode"].as_str().unwrap(), "{name} encode");
    }
}
