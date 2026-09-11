//! Purpose:
//! Cross-checks contextual text operations across the implemented encoding families.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test text`.
//!
//! Key details:
//! - Fixtures retain original encoded bytes, including embedded NUL and emoji escapes.
//! - Long inputs cross decoder batch boundaries under every case mode.

use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, text, unicode::CaseMode};

/// Decodes hexadecimal fixture bytes without interpreting their text encoding.
fn bytes(value: &serde_json::Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    (0..hex.len()).step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Compares length, width, validity, scrubbing, and all eight contextual case modes to PHP.
#[test]
fn encoded_text_operations_match_php() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!("fixtures/text.json")).unwrap();
    let substitution = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["encoding"].as_str().unwrap();
        let encoding = Encoding::lookup(name.as_bytes()).unwrap();
        let input = bytes(&case["input"]);
        assert_eq!(encoding.strlen(&input) as u64, case["length"].as_u64().unwrap(), "case {index} {name} length");
        assert_eq!(text::strwidth(&input, encoding) as u64, case["width"].as_u64().unwrap(), "case {index} {name} width");
        assert_eq!(encoding.decode(&input).is_valid(), case["valid"].as_bool().unwrap(), "case {index} {name} validity");
        assert_eq!(text::scrub(&input, encoding, substitution), bytes(&case["scrub"]), "case {index} {name} scrub");
        for mode in 0..8 {
            let converted = text::convert_case(&input, CaseMode::from_php(mode).unwrap(), encoding, substitution);
            assert_eq!(converted, bytes(&case["case"][mode as usize]), "case {index} {name} mode {mode}");
        }
    }
}
