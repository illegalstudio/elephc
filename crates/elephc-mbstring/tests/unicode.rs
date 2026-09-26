//! Purpose:
//! Cross-checks the shared Unicode engine against independently captured PHP results.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test unicode`.
//!
//! Key details:
//! - Oracle hashes cover every Unicode scalar, including unassigned codepoints.
//! - Context fixtures exercise title boundaries, sigma, and decoder chunk boundaries.

use elephc_mbstring::unicode::{character_width, convert_case, CaseMode};
use sha2::{Digest, Sha256};

/// Decodes the checked-in PHP oracle without consulting implementation tables.
fn oracle() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/unicode.json")).expect("PHP oracle JSON")
}

/// Converts valid scalar output to UTF-8 for a byte-exact comparison with PHP.
fn utf8(points: &[u32]) -> String {
    points.iter().map(|&point| char::from_u32(point).expect("Unicode scalar")).collect()
}

/// Verifies all eight case tables and widths for every one of 1,112,064 scalars.
#[test]
fn unicode_scalar_mappings_match_php() {
    let expected = oracle();
    let mut hashes: [Sha256; 8] = std::array::from_fn(|_| Sha256::new());
    let mut widths = Sha256::new();
    let mut count = 0;
    for code in (0..=0x10ffff).filter(|code| !(0xd800..=0xdfff).contains(code)) {
        count += 1;
        for (index, hash) in hashes.iter_mut().enumerate() {
            let mode = CaseMode::from_php(index as i64).expect("case mode");
            let output = utf8(&convert_case(&[code], mode, false));
            hash.update((output.len() as u32).to_le_bytes());
            hash.update(output.as_bytes());
        }
        widths.update([character_width(code) as u8]);
    }
    assert_eq!(count, expected["scalar_count"].as_u64().unwrap());
    for (index, hash) in hashes.into_iter().enumerate() {
        assert_eq!(format!("{:x}", hash.finalize()), expected["case_sha256"][index],
            "PHP case mode {index}");
    }
    assert_eq!(format!("{:x}", widths.finalize()), expected["width_sha256"]);
}

/// Verifies contextual title, sigma, combining-mark, and chunk-boundary behavior.
#[test]
fn unicode_contextual_case_matches_php() {
    for case in oracle()["contexts"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let points: Vec<u32> = input.chars().map(u32::from).collect();
        for mode in 0..8 {
            let output = convert_case(&points, CaseMode::from_php(mode).unwrap(), false);
            assert_eq!(utf8(&output), case["expected"][mode as usize],
                "PHP case mode {mode}, input {input:?}");
        }
    }
}

/// Keeps ISO-8859-9's Turkish rules isolated from other encodings and locales.
#[test]
fn unicode_turkish_rules_and_invalid_markers() {
    use CaseMode::*;
    let input = [0x49, 0x69, 0x130, 0x131];
    assert_eq!(utf8(&convert_case(&input, Upper, true)), "IİİI");
    assert_eq!(utf8(&convert_case(&input, Lower, true)), "ıiiı");
    assert_eq!(utf8(&convert_case(&input, Fold, true)), "ıiiı");
    for mode in 0..8 {
        let output = convert_case(&[0xffffffff, 0x61], CaseMode::from_php(mode).unwrap(), false);
        assert_eq!(output[0], 0xffffffff);
    }
    assert_eq!(character_width(0), 1);
    assert_eq!(character_width(0x301), 1);
    assert_eq!(character_width(0xffffffff), 1);
    assert_eq!(CaseMode::from_php(-1), None);
    assert_eq!(CaseMode::from_php(8), None);
}
