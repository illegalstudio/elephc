//! Purpose:
//! Checks every Unicode scalar against PHP's four mobile UTF-8 conversion variants.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test mobile_utf8`.
//!
//! Key details:
//! - Decoder and encoder hashes are independent, preserving non-bijective carrier mappings.
//! - Corpus and output lengths delimit every scalar conversion unambiguously.

use elephc_mbstring::encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding};
use sha2::{Digest, Sha256};

/// Verifies ordinary scalar fallback, carrier PUA overrides, and unsupported standalone flags.
#[test]
fn mobile_utf8_scalar_conversions_match_php() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/mobile_utf8.json")).unwrap();
    let substitute = Substitute { mode: SubstituteMode::None, ..Substitute::default() };
    for (name, expected) in fixture["encodings"].as_object().unwrap() {
        let encoding = Encoding::lookup(name.as_bytes()).unwrap();
        let mut decode_hash = Sha256::new();
        let mut encode_hash = Sha256::new();
        for code in 0..=0x10ffff {
            if (0xd800..=0xdfff).contains(&code) { continue; }
            let raw = UnicodeEncoding::Utf8.encode(&[code], substitute);
            let decoded = encoding.decode(&raw);
            let decoded = UnicodeEncoding::Ucs4Be.encode(&decoded.points, substitute);
            decode_hash.update((decoded.len() as u32).to_le_bytes());
            decode_hash.update(decoded);
            let encoded = encoding.encode(&[code], substitute);
            encode_hash.update((encoded.len() as u32).to_le_bytes());
            encode_hash.update(encoded);
        }
        assert_eq!(format!("{:x}", decode_hash.finalize()), expected["decode"].as_str().unwrap(), "{name} decode");
        assert_eq!(format!("{:x}", encode_hash.finalize()), expected["encode"].as_str().unwrap(), "{name} encode");
    }
}
