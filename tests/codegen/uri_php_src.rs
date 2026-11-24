//! Purpose:
//! Runs the Rust-native table of php-src `ext/uri` compliance cases against Elephc's
//! `Uri\Rfc3986\Uri` and `Uri\WhatWg\Url`, comparing each case's output with php-src's
//! recorded result.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext test_uri_php_src_bundle`.
//!
//! Key details:
//! - `cases.rs` embeds the PHP sources and expectations; this harness never reads `.phpt`
//!   files at run time.
//! - Safe cases are bundled (one compile per chunk) because each case would otherwise pay a
//!   full frontend, codegen, assemble and link cycle. Cases that inspect object identities,
//!   serialize, redeclare helpers, or assert compile-time fatal text stay isolated; the
//!   `zend_test` cases stay isolated too because each injects the same helper declaration.
//! - Wildcard expectations use php-src run-tests wildcards; `%r...%r` passes raw regex text.
//! - OOM cases run with their recorded heap capacity and compare the PHP fatal diagnostic; `zend_test` cases receive the local `zend_test_uri_parser()` stand-in.

use crate::support::*;
use regex::Regex;

mod cases;
use cases::{UriCase, CASES};

/// One case prepared for execution, with its `zend_test` helper injected when needed.
struct PreparedCase<'a> {
    /// The embedded source and expectation metadata.
    case: &'a UriCase,
    /// The source actually compiled for this run.
    source: String,
}

/// Runs the embedded php-src `ext/uri` matrix, bundling safe cases and isolating
/// identity-, serialization-, redeclaration-, OOM-, and compile-failure-sensitive ones.
#[test]
fn test_uri_php_src_bundle() {
    let mut failures = Vec::new();
    let mut fatal = Vec::new();
    let mut oom = Vec::new();
    let mut isolated = Vec::new();
    let mut bundled = Vec::new();

    for case in CASES {
        let source = if case.zend_test {
            case.source
                .replacen("<?php", &format!("<?php{ZEND_TEST_URI_PARSER}"), 1)
        } else {
            case.source.to_string()
        };
        let prepared = PreparedCase { case, source };
        if case.compile_failure {
            fatal.push(prepared);
        } else if case.heap_size.is_some() {
            oom.push(prepared);
        } else if case.isolate || case.zend_test {
            // The three `zend_test` cases each receive the same helper declaration, so they
            // cannot share a bundle's declaration namespace.
            isolated.push(prepared);
        } else {
            bundled.push(prepared);
        }
    }

    // Compile-time fatal cases: compare the semantic diagnostic (Elephc's compile-error
    // formatting is not PHP's runtime fatal format, but the class and rule must agree).
    for prepared in fatal {
        let diagnostic = compile_expect_type_error(&prepared.source);
        let expected_class = prepared
            .case
            .expected
            .split("Class ")
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .unwrap_or("");
        if expected_class.is_empty() || !diagnostic.contains(expected_class) {
            failures.push(format!(
                "{}: compile diagnostic did not name {expected_class}:\n{diagnostic}",
                prepared.case.name
            ));
        }
        if !diagnostic.contains("cannot extend final class") {
            failures.push(format!(
                "{}: compile diagnostic lacked the final-class rule:\n{diagnostic}",
                prepared.case.name
            ));
        }
    }

    for prepared in oom {
        let heap_size = prepared
            .case
            .heap_size
            .expect("OOM case carries a memory_limit");
        let output = compile_and_run_capture_with_heap_size(&prepared.source, heap_size);
        if output.success {
            failures.push(format!(
                "{}: expected a heap-exhaustion fatal, but the program exited successfully:\n{}",
                prepared.case.name, output.stdout
            ));
            continue;
        }
        let actual = if output.stderr.is_empty() {
            output.stdout
        } else {
            output.stderr
        };
        if !matches_expected(&actual, prepared.case.expected, prepared.case.expectf) {
            failures.push(failure_message(prepared.case, &actual));
        }
    }

    for prepared in isolated {
        let actual = compile_and_run(&prepared.source);
        if !matches_expected(&actual, prepared.case.expected, prepared.case.expectf) {
            failures.push(failure_message(prepared.case, &actual));
        }
    }

    for chunk in bundled.chunks(20) {
        let cases = chunk
            .iter()
            .map(|prepared| BundleCase {
                name: prepared.case.name,
                source: &prepared.source,
                expected: prepared.case.expected,
            })
            .collect::<Vec<_>>();
        let program = bundle_source(&cases);
        let stdout = match std::panic::catch_unwind(|| compile_and_run(&program)) {
            Ok(stdout) => stdout,
            Err(_) => {
                // A crash in one case ends the bundle. Re-run each case on its own so the
                // failure is attributed and later cases still get their chance.
                for prepared in chunk {
                    match std::panic::catch_unwind(|| compile_and_run(&prepared.source)) {
                        Ok(actual) => {
                            if !matches_expected(
                                &actual,
                                prepared.case.expected,
                                prepared.case.expectf,
                            ) {
                                failures.push(failure_message(prepared.case, &actual));
                            }
                        }
                        Err(_) => failures.push(format!(
                            "{}: program exited unsuccessfully (see panic above)",
                            prepared.case.name
                        )),
                    }
                }
                continue;
            }
        };
        let outputs = match split_bundle_output(&cases, &stdout) {
            Ok(outputs) => outputs,
            Err(error) => {
                failures.push(format!("bundle failed before splitting:\n{error}"));
                for prepared in chunk {
                    match std::panic::catch_unwind(|| compile_and_run(&prepared.source)) {
                        Ok(actual) => {
                            if !matches_expected(
                                &actual,
                                prepared.case.expected,
                                prepared.case.expectf,
                            ) {
                                failures.push(failure_message(prepared.case, &actual));
                            }
                        }
                        Err(_) => failures.push(format!(
                            "{}: program exited unsuccessfully (see panic above)",
                            prepared.case.name
                        )),
                    }
                }
                continue;
            }
        };
        for (prepared, actual) in chunk.iter().zip(outputs) {
            if !matches_expected(&actual, prepared.case.expected, prepared.case.expectf) {
                failures.push(failure_message(prepared.case, &actual));
            }
        }
    }

    let total = CASES.len();
    eprintln!(
        "uri cases: {} passed, {} failed",
        total.saturating_sub(failures.len()),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} URI case failures:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Test-only stand-in for php-src's `zend_test_uri_parser()`.
///
/// The real helper lives in the `zend_test` testing extension and exposes ext/uri's internal
/// parser structs. These cases exercise that struct through the same public surface, so the
/// harness supplies an equivalent built from `Uri\\Rfc3986\\Uri` and `Uri\\WhatWg\\Url`.
const ZEND_TEST_URI_PARSER: &str = r#"
function zend_test_uri_parser(string $uri, string $parser): array
{
    if ($parser === "Uri\\WhatWg\\Url") {
        $url = new Uri\WhatWg\Url($uri);
        $components = [
            "scheme" => $url->getScheme(),
            "username" => $url->getUsername(),
            "password" => $url->getPassword(),
            "host" => $url->getAsciiHost(),
            "port" => $url->getPort(),
            "path" => $url->getPath(),
            "query" => $url->getQuery(),
            "fragment" => $url->getFragment(),
        ];
        return ["normalized" => $components, "raw" => $components, "struct" => $components];
    }
    $rfc = new Uri\Rfc3986\Uri($uri);
    $normalized = [
        "scheme" => $rfc->getScheme(),
        "username" => $rfc->getUsername(),
        "password" => $rfc->getPassword(),
        "host" => $rfc->getHost(),
        "port" => $rfc->getPort(),
        "path" => $rfc->getPath(),
        "query" => $rfc->getQuery(),
        "fragment" => $rfc->getFragment(),
    ];
    if ($parser === "parse_url") {
        $parsed = \parse_url($uri);
        $raw = [
            "scheme" => $parsed["scheme"] ?? null,
            "username" => $parsed["user"] ?? null,
            "password" => $parsed["pass"] ?? null,
            "host" => $parsed["host"] ?? null,
            "port" => $parsed["port"] ?? null,
            "path" => $parsed["path"] ?? null,
            "query" => $parsed["query"] ?? null,
            "fragment" => $parsed["fragment"] ?? null,
        ];
        return ["normalized" => $normalized, "raw" => $raw, "struct" => $raw];
    }
    $raw = [
        "scheme" => $rfc->getRawScheme(),
        "username" => $rfc->getRawUsername(),
        "password" => $rfc->getRawPassword(),
        "host" => $rfc->getRawHost(),
        "port" => $rfc->getPort(),
        "path" => $rfc->getRawPath(),
        "query" => $rfc->getRawQuery(),
        "fragment" => $rfc->getRawFragment(),
    ];
    return ["normalized" => $normalized, "raw" => $raw, "struct" => $raw];
}
"#;

/// Matches one case's actual output against its expectation.
fn matches_expected(actual: &str, expected: &str, expectf: bool) -> bool {
    let actual = actual.replace("\r\n", "\n");
    let expected = expected.replace("\r\n", "\n");
    if !expectf {
        return actual == expected;
    }
    let pattern = expectf_pattern(&expected);
    Regex::new(&pattern)
        .map(|regex| regex.is_match(&actual))
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
            Some('i') => pattern.push_str("[+-]?[0-9]+"),
            Some('s') => pattern.push_str("[^\\r\\n]+"),
            Some('S') => pattern.push_str("[^\\r\\n]*"),
            Some('a') => pattern.push_str("(?s:.+)"),
            Some('A') => pattern.push_str("(?s:.*)"),
            Some('w') => pattern.push_str("\\s*"),
            Some('f') => pattern.push_str("[+-]?[0-9.]+(?:[Ee][+-]?[0-9]+)?"),
            Some('x') => pattern.push_str("[0-9a-fA-F]+"),
            Some('o') => pattern.push_str("[0-7]+"),
            Some('c') => pattern.push('.'),
            Some('e') => pattern.push_str("[/\\\\]"),
            Some(other) => {
                pattern.push_str(&regex::escape(&format!("%{other}")));
            }
            None => pattern.push_str("%"),
        }
    }
    pattern.push_str("\\z");
    pattern
}

/// Builds a failure message with the expected and actual output.
fn failure_message(case: &UriCase, actual: &str) -> String {
    format!(
        "{}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
        case.name, case.expected, actual
    )
}
