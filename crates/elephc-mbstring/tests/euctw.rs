//! Purpose:
//! Verifies every EUC-TW plane suffix and every Unicode-range reverse encoding input.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test euctw`.
//!
//! Key details:
//! - PHP hashes include malformed row/cell consumption and the older plane-14 mappings.

use elephc_mbstring::encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding};
use sha2::{Digest, Sha256};

/// Compares all three accepted planes and the complete encoder range to PHP's independent hashes.
#[test]
fn euctw_planes_and_encoder_match_php() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/euctw.json")).unwrap();
    let encoding = Encoding::lookup(b"EUC-TW").unwrap();
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    let mut decode_hash = Sha256::new();
    for plane in [0xa1, 0xa2, 0xae] {
        for pair in 0..=65535u32 {
            let decoded = encoding.decode(&[0x8e, plane, (pair >> 8) as u8, pair as u8]);
            decode_hash.update([u8::from(decoded.is_valid())]);
            let output = UnicodeEncoding::Ucs4Be.encode(&decoded.points, substitute);
            decode_hash.update((output.len() as u32).to_le_bytes());
            decode_hash.update(output);
        }
    }
    assert_eq!(format!("{:x}", decode_hash.finalize()), fixture["decode"].as_str().unwrap());
    let mut encode_hash = Sha256::new();
    for code in 0..=0x10ffff {
        let output = encoding.encode(&[code], substitute);
        encode_hash.update((output.len() as u32).to_le_bytes());
        encode_hash.update(output);
    }
    assert_eq!(format!("{:x}", encode_hash.finalize()), fixture["encode"].as_str().unwrap());
}
