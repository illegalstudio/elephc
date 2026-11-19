//! Purpose:
//! Runs the php-src 8.6 `Time\Duration` PHPT corpus as one bundled codegen test.
//!
//! Called from:
//! - `cargo test --test codegen_tests time_duration_php_src`.
//!
//! Key details:
//! - `time_duration_php_src_cases.rs` is generated from the vendored PHPT files under
//!   `tests/php_src/ext/date/time/duration`; regenerate it with
//!   `scripts/gen_php_src_time_duration_cases.py`.
//! - The shared `helper.inc` declarations are the bundle's first case, so every fixture
//!   sees the same `f`, `negate`, `negate_all`, `n`, and `fc` helpers php-src ships.
//! - Only php-src's 64-bit expectations are represented; elephc has no 32-bit target.
//! - Fixtures that need the internal `__unserialize` property loader or engine-only
//!   reflection methods are listed in the generator's exclusion table with their reason.

#[path = "time_duration_php_src_cases.rs"]
mod cases;

use cases::{DurationPhptCase, CASES, HELPERS};

use crate::support::{bundle_source, compile_and_run, split_bundle_output, BundleCase};

/// Converts the generated table into bundle cases.
fn bundle_cases() -> Vec<BundleCase<'static>> {
    let mut bundle = Vec::with_capacity(CASES.len() + 1);
    bundle.push(BundleCase {
        name: "helpers",
        source: HELPERS,
        expected: "",
    });
    bundle.extend(CASES.iter().map(case_to_bundle));
    bundle
}

/// Converts one generated `Time\Duration` fixture into a bundle case.
fn case_to_bundle(case: &DurationPhptCase) -> BundleCase<'static> {
    BundleCase {
        name: case.name,
        source: case.source,
        expected: case.expected,
    }
}

/// Compares one bundle case the way php-src's `run-tests.php` does: both the recorded
/// output and the `--EXPECT--` section are trimmed before the byte comparison.
fn compare_case(case: &BundleCase<'_>, actual: &str) {
    assert_eq!(
        actual.trim(),
        case.expected.trim(),
        "bundle case `{}` printed:\n{actual}\nexpected:\n{}",
        case.name,
        case.expected
    );
}

/// Compiles the whole `Time\Duration` corpus once and compares each fixture's stdout.
#[test]
fn test_time_duration_php_src_bundle() {
    let cases = bundle_cases();
    let stdout = compile_and_run(&bundle_source(&cases));
    let outputs = split_bundle_output(&cases, &stdout).expect("bundle output");
    for (case, actual) in cases.iter().zip(outputs) {
        compare_case(case, &actual);
    }
}

/// Verifies the generated table keeps php-src's provenance and skips 32-bit variants.
#[test]
fn test_time_duration_php_src_cases_are_complete() {
    assert!(CASES.len() >= 25, "unexpectedly small corpus: {}", CASES.len());
    assert!(CASES.iter().all(|case| case.phpt.ends_with(".phpt")));
    assert!(CASES.iter().all(|case| !case.phpt.contains("_32")));
    assert!(
        CASES
            .iter()
            .any(|case| case.name == "duration_fromIso8601DurationString_64"),
        "the 64-bit ISO-8601 boundary case must stay in the corpus"
    );
}
