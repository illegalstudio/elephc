//! Purpose:
//! Verifies Unicode codecs against exhaustive short-input and seeded PHP fixtures.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test codecs`.
//!
//! Key details:
//! - Independently captured hashes include validity, length, decoding, scrubbing, and case.
//! - Inputs cover every one- and two-byte sequence, BOMs, truncation, and raw UCS values.

use elephc_mbstring::encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding};
use elephc_mbstring::{text, unicode::CaseMode};
use sha2::{Digest, Sha256};

/// Reconstructs the deterministic byte corpus used by the PHP oracle generator.
fn inputs() -> Vec<Vec<u8>> {
    let mut inputs = vec![Vec::new()];
    inputs.extend((0..=255).map(|byte| vec![byte as u8]));
    inputs.extend((0..=65535).map(|pair| (pair as u16).to_be_bytes().to_vec()));
    let mut state = 42u32;
    for sample in 0..1024 {
        let mut input = Vec::new();
        for _ in 0..3 + sample % 33 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            input.push((state >> 24) as u8);
        }
        inputs.push(input);
    }
    for hex in [
        "fffe4100", "feff0041", "0000feff00000041", "fffe000041000000",
        "0000d800", "00110000", "ffffffff", "d8004100", "d8000041",
        "eda080", "f0908080", "f4908080", "f0808080", "e08080",
    ] {
        inputs.push((0..hex.len()).step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect());
    }
    inputs
}

/// Hashes every observable result with its byte length to prevent boundary ambiguity.
fn check_codec(encoding: Encoding, input: &[u8], hash: &mut Sha256) {
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    let decoded = encoding.decode(input);
    hash.update((encoding.strlen(input) as u32).to_le_bytes());
    hash.update([u8::from(decoded.is_valid())]);
    for output in [
        text::convert_encoding(input, encoding, Encoding::lookup(b"UCS-4BE").unwrap(), substitute),
        text::scrub(input, encoding, substitute),
        text::convert_case(input, CaseMode::Upper, encoding, substitute),
    ] {
        hash.update((output.len() as u32).to_le_bytes());
        hash.update(output);
    }
}

/// Compares decoding, validity, length, replacement, and uppercase for all core codecs.
#[test]
fn unicode_codecs_match_php() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!("fixtures/codecs.json")).unwrap();
    let inputs = inputs();
    assert_eq!(inputs.len() as u64, oracle["input_count"].as_u64().unwrap());
    let mut mismatches = Vec::new();
    for (name, expected) in oracle["sha256"].as_object().unwrap() {
        let encoding = Encoding::lookup(name.as_bytes()).unwrap();
        let mut hash = Sha256::new();
        for input in &inputs {
            check_codec(encoding, input, &mut hash);
        }
        let mut count = inputs.len();
        if matches!(name.as_str(), "EUC-JP" | "eucJP-win" | "EUC-JP-2004" | "CP51932") {
            for pair in 0..=65535u32 {
                check_codec(encoding, &[0x8f, (pair >> 8) as u8, pair as u8], &mut hash);
            }
            count += 65536;
        }
        assert_eq!(count as u64, oracle["input_counts"][name].as_u64().unwrap(), "{name} corpus size");
        if format!("{:x}", hash.finalize()) != expected.as_str().unwrap() {
            mismatches.push(name.as_str());
        }
    }
    assert!(mismatches.is_empty(), "PHP codec mismatches: {mismatches:?}");
}

/// Preserves input byte offsets and PHP's distinction between malformed and unmappable units.
#[test]
fn unicode_offsets_and_substitution_modes() {
    use UnicodeEncoding::*;
    let decoded = Utf8.decode(b"a\xc3\xa9\xe2\x82");
    assert_eq!(decoded.offsets, [0, 1, 3]);
    assert!(!decoded.is_valid());
    assert_eq!(Utf16.decode(&[0xff, 0xfe, 0x41, 0]).offsets, [2]);
    let long = Substitute { mode: SubstituteMode::Long, character: b'?' as u32 };
    let entity = Substitute { mode: SubstituteMode::Entity, ..long };
    assert_eq!(Ascii.encode(&[0x2603], long), b"U+2603");
    assert_eq!(Ascii.encode(&[0x2603], entity), b"&#x2603;");
    assert_eq!(Ascii.encode(&[u32::MAX], long), b"?");
    assert_eq!(Ascii.encode(&[0x2603], Substitute { mode: SubstituteMode::None, ..long }), b"");
}
