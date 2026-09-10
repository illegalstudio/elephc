//! Purpose:
//! Compares mbregex settings and the actual native Oniguruma provider with pinned PHP observations.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test regex`; native cases require explicit ignored-test selection.
//!
//! Key details:
//! - The native test uses a managed prefix or Oniguruma 6.9.10 development files and retains loaded code.
//! - PHP fixtures retain exact byte strings, warning text, empty groups, and duplicate names.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{error::MbError, regex::{install_provider, Captures, Limits, Options, Regex, RegexEncoding, RegexError}};
use serde_json::{json, Value};

#[path = "support/regex_provider.rs"]
mod regex_provider;
use regex_provider::provider;

/// Calls the actual settings ABI and copies its byte payload before releasing the result twice.
fn settings_call(operation: elephc_builtin_contract::RuntimeBuiltinId, value: Option<&[u8]>) -> (u64, i64, Vec<u8>) {
    use elephc_builtin_contract::mbstring_abi::*;
    use elephc_mbstring::abi::{elephc_mbstring_call_v1, elephc_mbstring_release_v1};
    let arguments: Vec<_> = value.map(MbArgV1::string).into_iter().collect();
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), arguments.as_ptr(), arguments.len() as u64, &mut result); }
    let bytes = if result.len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize).to_vec() } };
    let output = (result.kind, result.value, bytes);
    assert_eq!(result.diagnostics_len, 0);
    unsafe { elephc_mbstring_release_v1(&mut result); elephc_mbstring_release_v1(&mut result); }
    output
}

/// Replays every pinned PHP option and encoding observation through the public settings dispatch boundary.
#[test]
fn regex_settings_abi_match_php() {
    use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::*};
    let mut count = 0;
    for case in cases() {
        let operation = match case["op"].as_str().unwrap() {
            "encoding" => RuntimeBuiltinId::MbRegexEncoding,
            "options" => RuntimeBuiltinId::MbRegexSetOptions,
            _ => continue,
        };
        settings_call(RuntimeBuiltinId::MbRegexEncoding, Some(b"UTF-8"));
        settings_call(RuntimeBuiltinId::MbRegexSetOptions, Some(b"pr"));
        let baseline = settings_call(operation, None);
        let result = settings_call(operation, Some(&bytes(&case["value"])));
        if case.get("error").is_some() {
            assert_eq!(result.0, RESULT_VALUE_ERROR, "{case}");
            assert_eq!(json!(["ValueError", hex(&result.2)]), case["error"], "{case}");
            assert_eq!(settings_call(operation, None), baseline, "{case}");
        } else {
            if operation == RuntimeBuiltinId::MbRegexEncoding {
                assert_eq!(result, (RESULT_BOOL, 1, vec![]), "{case}");
            } else { assert_eq!(result, baseline, "{case}"); }
            assert_eq!(settings_call(operation, None), (RESULT_STRING, 0, case["result"].as_str().unwrap().as_bytes().to_vec()), "{case}");
        }
        count += 1;
    }
    assert_eq!(count, 4051);
}

/// Replays pinned anchored matches through the exported C ABI and its diagnostic ownership boundary.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn regex_match_abi_matches_php() {
    use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::*};
    use elephc_mbstring::abi::{elephc_mbstring_call_v1, elephc_mbstring_release_v1,
        elephc_mbstring_regex_provider_v1, elephc_mbstring_regex_available_v1};
    let table = provider();
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&table) }, 0);
    assert_eq!(elephc_mbstring_regex_available_v1(), 1);
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(std::ptr::null()) }, 1);
    for header in [[0_u32, 112], [1, 8]] {
        assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(header.as_ptr().cast()) }, 1);
    }
    let mut count = 0;
    for case in cases().into_iter().filter(|case| case["op"] == "match") {
        settings_call(RuntimeBuiltinId::MbRegexEncoding, Some(case["encoding"].as_str().unwrap().as_bytes()));
        let pattern = bytes(&case["pattern"]);
        let subject = bytes(&case["subject"]);
        let options = case["options"].as_str().unwrap().as_bytes();
        let arguments = [MbArgV1::string(&pattern), MbArgV1::string(&subject), MbArgV1::string(options)];
        let mut result = MbResultV1::default();
        unsafe { elephc_mbstring_call_v1(RuntimeBuiltinId::MbEregMatch.as_u32(), arguments.as_ptr(), 3, &mut result); }
        assert_eq!(result.kind, RESULT_BOOL, "{case}");
        assert_eq!(json!(result.value != 0), case["result"][0], "{case}");
        let expected: Vec<u8> = case["warnings"].as_array().unwrap().iter()
            .flat_map(|warning| [b"Warning: ".to_vec(), bytes(warning), vec![b'\n']].concat()).collect();
        let diagnostics = if result.diagnostics_len == 0 { &[][..] } else {
            unsafe { std::slice::from_raw_parts(result.diagnostics, result.diagnostics_len as usize) }
        };
        assert_eq!(diagnostics, expected, "{case}");
        unsafe { elephc_mbstring_release_v1(&mut result); elephc_mbstring_release_v1(&mut result); }
        count += 1;
    }
    assert_eq!(count, 2114);
}

/// Decodes the fixture's lossless hexadecimal byte representation.
fn bytes(value: &Value) -> Vec<u8> {
    value.as_str().unwrap().as_bytes().chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
}

/// Encodes exact PHP output bytes without requiring UTF-8.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Reads the independent settings and matching corpus, enforcing its complete recorded size.
fn cases() -> Vec<Value> {
    let reader = flate2::read::GzDecoder::new(include_bytes!("fixtures/regex.jsonl.gz").as_slice());
    let cases: Vec<Value> = BufReader::new(reader).lines().map(|line| serde_json::from_str(&line.unwrap()).unwrap()).collect();
    assert_eq!(cases.len(), 8279);
    cases
}

/// Preserves option normalization, first-invalid-byte diagnostics, alias acceptance, and canonical names.
#[test]
fn regex_settings_match_php() {
    for case in cases() {
        if case["op"] == "options" {
            match Options::parse(&bytes(&case["value"]), false) {
                Ok(options) => assert_eq!(json!(options.as_string()), case["result"], "{case}"),
                Err(error) => {
                    let message = match error { MbError::Value(text) => text.into_bytes(), MbError::ValueBytes(bytes) => bytes, other => panic!("{other:?}") };
                    assert_eq!(json!(["ValueError", hex(&message)]), case["error"], "{case}");
                }
            }
        } else if case["op"] == "encoding" {
            match RegexEncoding::lookup(&bytes(&case["value"])) {
                Some(encoding) => assert_eq!(json!(encoding.name()), case["result"], "{case}"),
                None => assert!(case.get("error").is_some(), "{case}"),
            }
        }
    }
    assert_eq!(Options::default().as_string(), "pr");
    assert_eq!(Options::parse(b"r", true).unwrap().as_string(), "ir");
    assert!(!RegexEncoding::lookup(b"UTF-16BE").unwrap().is_valid(b"a"));
}

/// Distinguishes native defaults from zero and unsigned boundaries in PHP's two limit-setting paths.
#[test]
fn regex_limits_preserve_php_boundaries() {
    for value in [-1, 0, 1, u32::MAX as i64, u32::MAX as i64 + 1] {
        let search = Limits::from_ini(value, value, false);
        let anchored = Limits::from_ini(value, value, true);
        assert_eq!(search.stack, u32::try_from(value).ok());
        assert_eq!(search.retry, search.stack);
        assert_eq!(anchored.stack, u32::try_from(value).ok().filter(|&value| value > 0 && value < u32::MAX));
        assert_eq!(anchored.retry, anchored.stack);
    }
}

/// Converts retained captures to PHP's register array, where empty and unmatched groups both become false.
fn php_groups(captures: Captures, subject: &[u8]) -> Vec<Value> {
    let value = |index: usize| match &captures.groups[index] {
        Some(range) if !range.is_empty() => json!(hex(&subject[range.clone()])),
        _ => json!(false),
    };
    let mut groups = (0..captures.groups.len()).map(|index| json!([index, value(index)])).collect::<Vec<_>>();
    for (name, index) in &captures.names { groups.push(json!([hex(name), value(*index)])); }
    groups
}

/// Uses real Oniguruma to compare anchored/search semantics, every syntax, Unicode, errors, and capture metadata.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn regex_native_provider_matches_php() {
    let provider = provider();
    assert!(provider.is_complete());
    assert!(unsafe { install_provider(provider) });
    assert_eq!(regex_provider::guarded_subjects(), 0);
    assert!(unsafe { install_provider(provider) });
    let mut invalid = provider;
    invalid.free_region = None;
    assert!(!unsafe { install_provider(invalid) });
    invalid = provider;
    invalid.free_region = provider.free_regex;
    assert!(invalid.is_complete());
    assert!(!unsafe { install_provider(invalid) });
    let mut argument_errors = 0;
    for case in cases() {
        let anchored = case["op"] == "match";
        if !anchored && case["op"] != "search" { continue; }
        if case.get("error").is_some() {
            // The native transport accepts empty regexes; mb_ereg's PHP argument adapter must reject them.
            assert!(!anchored && case["pattern"] == "" && case["error"][0] == "ValueError", "{case}");
            argument_errors += 1;
            continue;
        }
        let encoding = RegexEncoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
        let options = Options::parse(case["options"].as_str().unwrap().as_bytes(), false).unwrap();
        let pattern = bytes(&case["pattern"]);
        let subject = bytes(&case["subject"]);
        let mut warnings = Vec::new();
        let mut groups = Vec::new();
        let matched = if !encoding.is_valid(&subject) { false } else {
            match Regex::compile(&pattern, encoding, options) {
                Ok(regex) => match regex.search(&subject, 0, anchored, Limits::from_ini(100_000, 1_000_000, anchored)).unwrap_or_else(|error| panic!("{case}: {error:?}")) {
                    Some(captures) => { if !anchored { groups = php_groups(captures, &subject); } true },
                    None => false,
                },
                Err(RegexError::Pattern(message)) => {
                    let mut warning = if anchored { b"mb_ereg_match(): ".to_vec() } else { b"mb_ereg(): ".to_vec() };
                    warning.extend(message);
                    warnings.push(hex(&warning));
                    false
                }
                Err(error) => panic!("{case}: {error:?}"),
            }
        };
        assert_eq!(json!([matched, groups]), case["result"], "{case}");
        assert_eq!(json!(warnings), case["warnings"], "{case}");
    }
    assert_eq!(argument_errors, 130);
    let regex = Regex::compile(b"(?<x>a)|(?<x>b)", RegexEncoding::default(), Options::default()).unwrap();
    assert!(!regex.numbered_backrefs().unwrap());
    let plain = Regex::compile(b"(a)", RegexEncoding::default(), Options::default()).unwrap();
    assert!(plain.numbered_backrefs().unwrap());
    let sjis = RegexEncoding::lookup(b"SJIS").unwrap();
    let windows = RegexEncoding::lookup(b"SJIS-WIN").unwrap();
    let cached = Regex::compile(b".", sjis, Options::default()).unwrap();
    assert!(!sjis.is_valid(b"\xfa\x40") && windows.is_valid(b"\xfa\x40"));
    // A reused native pattern must not retain the previous alias's PHP validation policy.
    let matched = cached.search(b"\xfa\x40", 0, true, Limits::from_ini(100_000, 1_000_000, true)).unwrap().unwrap();
    assert_eq!(matched.groups, [Some(0..2)]);
    for _ in 0..1000 { assert!(regex.search(b"b", 0, false, Limits::from_ini(100_000, 1_000_000, false)).unwrap().is_some()); }
}
