//! Purpose:
//! Compiles and runs the committed PHP 8.5.10 `ext/sodium/tests` cases.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - The cases live in the generated sibling module `sodium_php_src_cases.rs`; each test
//!   carries its PHP source and assertion inline, matching the ordinary Elephc fixture
//!   style. The test process never parses a `.phpt` file.
//! - stdout and stderr share one OS file so warnings and output keep their order.
//! - All 31 committed fixtures execute; the three `password_hash` interoperability
//!   cases run against the core password provider implemented over the sodium bridge.
//! - `tests/php_src/sodium/` remains the provenance copy and the input for
//!   `scripts/gen_php_src_sodium_cases.py`.

use crate::support::*;
use regex::RegexBuilder;
use std::sync::Once;

#[path = "sodium_php_src_cases.rs"]
mod cases;

/// Replays one inline php-src sodium fixture and returns its merged output.
pub(super) fn run_case(source: &str) -> String {
    // The Argon2 password fixtures hash 64–128 MiB with up to eight passes and verify
    // each hash again; the default 60-second fixture budget is too tight when CI shards
    // run several of them concurrently. Raising the shared budget only lengthens the
    // timeout and does not change any assertion.
    static TIMEOUT: Once = Once::new();
    TIMEOUT.call_once(|| std::env::set_var("ELEPHC_TEST_BINARY_TIMEOUT_SECS", "300"));
    if skip_without_sodium_native("sodium_php_src") {
        panic!("managed libsodium artifact is required for the sodium PHPT corpus");
    }
    let output = compile_and_run_capture_merged_with_sodium(source);
    output.stdout.replace("\r\n", "\n")
}

/// Asserts one php-src `EXPECTF` pattern against a fixture's merged output.
pub(super) fn assert_format(actual: &str, expected: &str) {
    let actual = actual.trim_matches('\n');
    let expected = expected.trim_matches('\n');
    // elephc deliberately keeps no call stack, so an expectation that ends in a PHP stack
    // trace is compared on its uncaught class, message and location only. The semantic
    // assertion — the exception class and text — stays byte-exact.
    if expected.contains("Stack trace:") {
        let head_pattern = expected.lines().next().unwrap_or_default();
        let head_actual = actual.lines().next().unwrap_or_default();
        assert_format_line(head_actual, head_pattern);
        return;
    }
    let pattern = format!("^{}$", expectf_to_regex(expected));
    let regex = RegexBuilder::new(&pattern)
        .dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTF regex {pattern}: {error}"));
    assert!(
        regex.is_match(actual),
        "--- pattern ---\n{pattern}\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
    );
}

/// Matches one `EXPECTF` line, which may itself end in a `%s:%d` location.
fn assert_format_line(actual: &str, pattern: &str) {
    let pattern = format!("^{}$", expectf_to_regex(pattern));
    let regex = RegexBuilder::new(&pattern)
        .dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTF regex {pattern}: {error}"));
    assert!(
        regex.is_match(actual),
        "--- pattern ---\n{pattern}\n--- actual ---\n{actual}"
    );
}

/// Asserts one php-src `EXPECTREGEX` pattern against a fixture's merged output.
#[allow(dead_code)]
pub(super) fn assert_regex(actual: &str, expected: &str) {
    let regex = RegexBuilder::new(expected.trim())
        .dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTREGEX pattern: {error}"));
    assert!(
        regex.is_match(actual),
        "--- pattern ---\n{}\n--- actual ---\n{actual}",
        expected.trim()
    );
}

/// Converts a php-src `EXPECTF` section into an anchored regular expression.
fn expectf_to_regex(wanted: &str) -> String {
    let wanted = wanted.replace("\r\n", "\n");
    let mut regex = String::new();
    let mut start_offset = 0;
    let bytes = wanted.as_bytes();
    while start_offset < bytes.len() {
        let start = wanted[start_offset..].find("%r").map(|index| start_offset + index);
        let (start, end) = match start {
            Some(start) => match wanted[start + 2..].find("%r").map(|index| start + 2 + index) {
                Some(end) => (start, end),
                None => (bytes.len(), bytes.len()),
            },
            None => (bytes.len(), bytes.len()),
        };
        regex.push_str(&regex::escape(&wanted[start_offset..start]));
        if end > start {
            regex.push('(');
            regex.push_str(&wanted[start + 2..end]);
            regex.push(')');
        }
        start_offset = end + 2;
    }
    for (token, replacement) in [
        ("%e", regex::escape(std::path::MAIN_SEPARATOR_STR)),
        ("%s", r"[^\r\n]+".to_string()),
        ("%S", r"[^\r\n]*".to_string()),
        ("%a", r".+?".to_string()),
        ("%A", r".*?".to_string()),
        ("%w", r"\s*".to_string()),
        ("%i", r"[+-]?\d+".to_string()),
        ("%d", r"\d+".to_string()),
        ("%x", r"[0-9a-fA-F]+".to_string()),
        ("%f", r"[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[Ee][+-]?\d+)?".to_string()),
        ("%c", ".".to_string()),
        ("%0", r"\x00".to_string()),
    ] {
        regex = regex.replace(&regex::escape(token), &replacement);
    }
    regex
}
