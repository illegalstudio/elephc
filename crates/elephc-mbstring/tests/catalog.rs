//! Purpose:
//! Verifies canonical encodings, aliases, MIME names, and the complete PHP scope fixture.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test catalog`.
//!
//! Key details:
//! - Codec behavior is checked separately from metadata and public surface completeness.
//! - Surface names must agree with the repository's independent PHP baseline.

use elephc_mbstring::encoding::Encoding;

/// Reads the independent PHP reflection snapshot used to audit the extension surface.
fn surface() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../scripts/mbstring/php_surface.json")).unwrap()
}

/// Covers every canonical encoding and alias, including casing and unknown-name rejection.
#[test]
fn encoding_catalog_matches_php() {
    let surface = surface();
    let actual: Vec<_> = Encoding::all().map(|encoding| encoding.name()).collect();
    assert_eq!(serde_json::to_value(actual).unwrap(), surface["encoding_order"]);
    for (name, expected) in surface["encodings"].as_object().unwrap() {
        let encoding = Encoding::lookup(name.as_bytes()).expect("canonical encoding");
        assert_eq!(encoding.name(), name);
        assert_eq!(serde_json::to_value(encoding.aliases()).unwrap(), expected["aliases"]);
        assert_eq!(encoding.mime_name(), expected["mime"].as_str());
        assert_eq!(encoding.supports_ord_chr(), expected["supports_ord_chr"].as_bool().unwrap());
        assert_eq!(encoding.supports_detection(), expected["supports_detection"].as_bool().unwrap());
        for alias in encoding.aliases() {
            assert_eq!(Encoding::lookup(alias.to_ascii_uppercase().as_bytes()), Some(encoding));
            assert_eq!(Encoding::lookup(alias.to_ascii_lowercase().as_bytes()), Some(encoding));
        }
    }
    for invalid in [b"".as_slice(), b"UTF-8\0", b" UTF-8", b"utf-8 ", b"not-an-encoding"] {
        assert_eq!(Encoding::lookup(invalid), None);
    }
}

/// Resolves MIME-only names and shared MIME labels with PHP's canonical-name precedence.
#[test]
fn encoding_lookup_precedence_matches_php() {
    let cases: serde_json::Value = serde_json::from_str(include_str!("fixtures/encoding_lookup.json")).unwrap();
    for (name, expected) in cases.as_object().unwrap() {
        assert_eq!(Encoding::lookup(name.as_bytes()).map(Encoding::name), expected.as_str(), "{name}");
    }
}

/// Checks the values and ownership of all nine public mbstring constants against PHP reflection.
#[test]
fn mbstring_constant_values_match_php() {
    use elephc_builtin_contract::{constants, ConstValue, PhpModule};
    let actual: serde_json::Map<String, serde_json::Value> = constants().iter()
        .filter(|entry| entry.module == PhpModule::Mbstring)
        .map(|entry| (entry.name.to_owned(), match entry.value {
            ConstValue::Int(value) => serde_json::json!(value),
            ConstValue::Str(value) => serde_json::json!(value),
            _ => panic!("unexpected mbstring constant {}", entry.name),
        })).collect();
    assert_eq!(serde_json::Value::Object(actual), surface()["constants"]);
}

/// Keeps the intended function surface equal to the project's PHP compatibility baseline.
#[test]
fn mbstring_scope_matches_repository_baseline() {
    let surface = surface();
    let baseline: serde_json::Value = serde_json::from_str(
        include_str!("../../../scripts/docs/php_baseline.json")).unwrap();
    assert_eq!(surface["php_version"], baseline["php_version"]);
    let functions: Vec<_> = baseline["functions"].as_object().unwrap().iter()
        .filter(|(_, module)| *module == "mbstring").map(|(name, _)| name).collect();
    let captured: Vec<_> = surface["functions"].as_object().unwrap().keys().collect();
    assert_eq!(captured, functions);
}
