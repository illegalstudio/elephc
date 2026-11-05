//! Purpose:
//! Runs the vendored PHP 8.6 `stream_errors_*` PHPT corpus against the AOT pipeline.
//!
//! Called from:
//! - `cargo test --test codegen_tests stream_errors_corpus_php86`.
//!
//! Key details:
//! - Fixtures requiring `--SKIPIF--` are not run; the corpus records them as skipped.
//! - Known non-passing fixtures are listed explicitly so the set cannot silently grow.
//! - Outputs merge stderr onto stdout, matching PHP's interleaved diagnostics.

use elephc::php_version::PhpVersion;
use regex::Regex;
use std::fs;
use std::path::Path;

use crate::support::*;

/// Fixtures that still exercise unimplemented stream-error store/handler plumbing.
const KNOWN_NON_PASSING: &[&str] = &[
    "stream_errors_deep_nesting_in_handler",
    "stream_errors_exception_mode_terminal",
    "stream_errors_handler_throws",
    "stream_errors_invalid_types",
    "stream_errors_mix_modes_storage",
    "stream_errors_wrapper_nested_operation",
];

/// Fixtures whose `--SKIPIF--` section requires a harness feature elephc does not emulate.
const HARNESS_SKIPS: &[&str] = &[
    "stream_errors_handler_outside_operation",
    "stream_errors_handler_recursion",
    "stream_errors_legacy_handler_raises_error",
    "stream_errors_operation_depth_limit",
];

/// Runs every runnable upstream fixture and asserts the known-failure set is exact.
#[test]
fn test_stream_errors_corpus_php86() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_src/ext/standard/tests/streams");
    let mut names = fs::read_dir(&root)
        .expect("stream fixture directory")
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name.starts_with("stream_errors_") && name.ends_with(".phpt")).then_some(name)
        })
        .collect::<Vec<_>>();
    names.sort();

    let mut passed = Vec::new();
    let mut failed = Vec::new();
    for name in names {
        let stem = name.trim_end_matches(".phpt");
        let text = fs::read_to_string(root.join(&name)).expect("read fixture");
        if text.contains("--SKIPIF--") || text.contains("include __DIR__") {
            assert!(
                HARNESS_SKIPS.contains(&stem),
                "{stem} gained a harness-only section; add it to HARNESS_SKIPS or support it"
            );
            continue;
        }
        let (source, expected, expectf) = parse_fixture(&text);
        let actual = match std::panic::catch_unwind(|| {
            compile_and_run_capture_merged_with_php_version(&source, PhpVersion::Php86)
        }) {
            Ok(actual) => actual,
            Err(_) => {
                assert!(
                    KNOWN_NON_PASSING.contains(&stem),
                    "{stem} fails to compile or run"
                );
                failed.push(stem.to_string());
                continue;
            }
        };
        let ok = matches_expected(&actual, &expected, expectf);
        if KNOWN_NON_PASSING.contains(&stem) {
            assert!(
                !ok,
                "{stem} now passes; remove it from KNOWN_NON_PASSING"
            );
            failed.push(stem.to_string());
        } else {
            assert!(ok, "{stem} output mismatch\n--- expected ---\n{expected}\n--- actual ---\n{actual}");
            passed.push(stem.to_string());
        }
    }

    assert_eq!(passed.len(), 8, "passing fixtures: {passed:?}");
    assert_eq!(failed.len(), 6, "known non-passing fixtures: {failed:?}");
}

/// Splits one PHPT fixture into its FILE source and EXPECT/EXPECTF expectation.
fn parse_fixture(text: &str) -> (String, String, bool) {
    let file = text
        .split_once("--FILE--\n")
        .and_then(|(_, rest)| rest.split_once("\n--EXPECT"))
        .expect("FILE section");
    let source = file.0.split("?>").next().unwrap_or(file.0).to_string();
    let (expected, expectf) = if let Some((_, rest)) = text.split_once("\n--EXPECTF--\n") {
        (rest.to_string(), true)
    } else if let Some((_, rest)) = text.split_once("\n--EXPECT--\n") {
        (rest.to_string(), false)
    } else {
        panic!("EXPECT section");
    };
    (source, expected, expectf)
}

/// Matches an expectation, translating PHP's EXPECTF wildcards when needed.
fn matches_expected(actual: &str, expected: &str, expectf: bool) -> bool {
    let actual = actual.trim_end();
    let expected = expected.trim_end();
    if !expectf {
        return actual == expected;
    }
    Regex::new(&expectf_pattern(expected))
        .map(|regex| regex.is_match(actual))
        .unwrap_or(false)
}

/// Translates PHP run-tests EXPECTF wildcards into an anchored regular expression.
fn expectf_pattern(expected: &str) -> String {
    let mut pattern = String::from("(?s)\\A");
    let mut chars = expected.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '%' {
            pattern.push_str(&regex::escape(&character.to_string()));
            continue;
        }
        match chars.next() {
            Some('r') => {
                let mut raw = String::new();
                while let Some(character) = chars.next() {
                    if character == '%' && chars.peek() == Some(&'r') {
                        chars.next();
                        break;
                    }
                    raw.push(character);
                }
                pattern.push_str(&raw);
            }
            Some('d') => pattern.push_str("[0-9]+"),
            Some('s') => pattern.push_str("[^\\r\\n]+"),
            Some('S') => pattern.push_str("[^\\r\\n]*"),
            Some('c') => pattern.push('.'),
            Some(other) => pattern.push_str(&regex::escape(&format!("%{other}"))),
            None => pattern.push('%'),
        }
    }
    pattern.push_str("\\z");
    pattern
}
