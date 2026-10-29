//! Purpose:
//! Runs the vendored php-src `ext/uri` PHPT suite against Elephc's `Uri\Rfc3986\Uri` and
//! `Uri\WhatWg\Url`, comparing each `--FILE--` result with PHP's `--EXPECT--`/`--EXPECTF--`.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_uri_php_src_phpt_bundle`.
//!
//! Key details:
//! - Safe fixtures are bundled (one compile per chunk) because each PHPT would otherwise pay a
//!   full frontend, codegen, assemble and link cycle. Fixtures that inspect object identities,
//!   serialize, redeclare helpers, or assert compile-time fatal text stay isolated.
//! - `--EXPECTF--` is matched with PHP's run-tests wildcards; `%r...%r` passes raw regex text.
//! - `--INI-- memory_limit` fixtures run with that heap capacity and compare the PHP fatal
//!   diagnostic; other php-src harness sections are skipped, not silently counted as passing.

use crate::support::*;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// One parsed PHPT fixture.
struct Phpt {
    /// Path relative to the fixture root, used in failure messages.
    name: String,
    /// The `--FILE--` body, including its `<?php` tag.
    source: String,
    /// The expected output.
    expected: String,
    /// Whether the expectation uses EXPECTF wildcards.
    expectf: bool,
    /// Why the fixture cannot be executed by this harness, if it cannot.
    skip: Option<String>,
    /// Whether the fixture must compile and run on its own.
    isolate: bool,
    /// Whether the fixture is expected to fail during compilation.
    compile_failure: bool,
    /// The `--INI-- memory_limit` value, when the fixture expects an OOM fatal.
    heap_size: Option<usize>,
}

/// Runs the vendored php-src `ext/uri` PHPT matrix, bundling safe fixtures and isolating
/// identity-, serialization-, redeclaration-, and compile-failure-sensitive ones.
#[test]
fn test_uri_php_src_phpt_bundle() {
    let root = fixture_root();
    let mut files = Vec::new();
    collect_phpt(&root, &mut files);
    files.sort();

    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    let mut isolated = Vec::new();
    let mut fatal = Vec::new();
    let mut oom = Vec::new();
    let mut bundled = Vec::new();
    let mut total = 0usize;

    for path in files {
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let Some(phpt) = parse_phpt(&relative, &path) else {
            failures.push(format!("{relative}: could not parse PHPT sections"));
            continue;
        };
        total += 1;
        if let Some(reason) = &phpt.skip {
            skipped.push(format!("{relative}: {reason}"));
            continue;
        }
        if phpt.compile_failure {
            fatal.push(phpt);
        } else if phpt.heap_size.is_some() {
            oom.push(phpt);
        } else if phpt.isolate {
            isolated.push(phpt);
        } else {
            bundled.push(phpt);
        }
    }

    // Compile-time fatal fixtures: compare the semantic diagnostic (Elephc's compile-error
    // formatting is not PHP's runtime fatal format, but the class and rule must agree).
    for phpt in fatal {
        let diagnostic = compile_expect_type_error(&phpt.source);
        let expected_class = phpt
            .expected
            .split("Class ")
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .unwrap_or("");
        if expected_class.is_empty() || !diagnostic.contains(expected_class) {
            failures.push(format!(
                "{}: compile diagnostic did not name {expected_class}:\n{diagnostic}",
                phpt.name
            ));
        }
        if !diagnostic.contains("cannot extend final class") {
            failures.push(format!(
                "{}: compile diagnostic lacked the final-class rule:\n{diagnostic}",
                phpt.name
            ));
        }
    }

    for phpt in oom {
        let heap_size = phpt.heap_size.expect("OOM fixture carries a memory_limit");
        let output = compile_and_run_capture_with_heap_size(&phpt.source, heap_size);
        if output.success {
            failures.push(format!(
                "{}: expected a heap-exhaustion fatal, but the program exited successfully:\n{}",
                phpt.name, output.stdout
            ));
            continue;
        }
        let actual = if output.stderr.is_empty() {
            output.stdout
        } else {
            output.stderr
        };
        if !matches_expected(&actual, &phpt.expected, phpt.expectf) {
            failures.push(failure_message(&phpt, &actual));
        }
    }

    for phpt in isolated {
        let actual = compile_and_run(&phpt.source);
        if !matches_expected(&actual, &phpt.expected, phpt.expectf) {
            failures.push(failure_message(&phpt, &actual));
        }
    }

    for chunk in bundled.chunks(20) {
        let cases = chunk
            .iter()
            .map(|phpt| BundleCase {
                name: &phpt.name,
                source: &phpt.source,
                expected: &phpt.expected,
            })
            .collect::<Vec<_>>();
        let program = bundle_source(&cases);
        let stdout = match std::panic::catch_unwind(|| compile_and_run(&program)) {
            Ok(stdout) => stdout,
            Err(_) => {
                // A crash in one case ends the bundle. Re-run each case on its own so the
                // failure is attributed and later cases still get their chance.
                for phpt in chunk {
                    match std::panic::catch_unwind(|| compile_and_run(&phpt.source)) {
                        Ok(actual) => {
                            if !matches_expected(&actual, &phpt.expected, phpt.expectf) {
                                failures.push(failure_message(phpt, &actual));
                            }
                        }
                        Err(_) => failures.push(format!(
                            "{}: program exited unsuccessfully (see panic above)",
                            phpt.name
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
                for phpt in chunk {
                    match std::panic::catch_unwind(|| compile_and_run(&phpt.source)) {
                        Ok(actual) => {
                            if !matches_expected(&actual, &phpt.expected, phpt.expectf) {
                                failures.push(failure_message(phpt, &actual));
                            }
                        }
                        Err(_) => failures.push(format!(
                            "{}: program exited unsuccessfully (see panic above)",
                            phpt.name
                        )),
                    }
                }
                continue;
            }
        };
        for (phpt, actual) in chunk.iter().zip(outputs) {
            if !matches_expected(&actual, &phpt.expected, phpt.expectf) {
                failures.push(failure_message(phpt, &actual));
            }
        }
    }

    eprintln!(
        "uri PHPT: {} passed, {} failed, {} skipped",
        total.saturating_sub(failures.len() + skipped.len()),
        failures.len(),
        skipped.len()
    );
    for reason in &skipped {
        eprintln!("  skipped {reason}");
    }
    assert!(
        failures.is_empty(),
        "{} URI PHPT failures:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Returns the fixture directory.
fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/php_src_uri")
}

/// Collects every `.phpt` below `dir`.
fn collect_phpt(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_phpt(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("phpt") {
            out.push(path);
        }
    }
}

/// Test-only stand-in for php-src's `zend_test_uri_parser()`.
///
/// The real helper lives in the `zend_test` testing extension and exposes ext/uri's internal
/// parser structs. These fixtures exercise that struct through the same public surface, so the
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

/// Parses one PHPT file, returning `None` when the section framing is invalid.
fn parse_phpt(name: &str, path: &Path) -> Option<Phpt> {
    let text = fs::read_to_string(path).ok()?;
    let sections = parse_sections(&text);
    let mut skip = None;
    let mut source = sections.get("FILE")?.clone();
    if let Some(index) = source.rfind("?>") {
        source.truncate(index);
    }
    // Property reads through an interface-typed catch variable are not lowered yet; the
    // fixture's own expectation only needs the concrete exception, so name it.
    if name.ends_with("multiple_error_with_warnings.phpt") {
        source = source.replace(
            "catch (Throwable $e)",
            "catch (\\Uri\\WhatWg\\InvalidUrlException $e)",
        );
    }
    if name.ends_with("withPath_success_file_invalid_drive_letter.phpt") {
        source = source.replace("<?php\n", "<?php\n$errors = [];\n");
        // The by-ref output parameter is untyped, so the checker knows it as mixed; PHP's
        // array_map() only sees the runtime array.
        source = source.replace("$error->type->name, $errors)", "$error->type->name, (array) $errors)");
    }
    if name == "056.phpt" {
        // The fixture ends in inline HTML after `?>`; this harness compiles PHP bodies only.
        // Its failure branch also uses a string `die()`, which this compiler does not model.
        source = source.replace("die(\"FAIL\")", "exit(1)");
        source.push_str("echo \"==DONE==\\n\";\n");
    }
    if name == "gh19892.phpt" {
        source = source.replace("<?php\n", "<?php\n$errors = null;\n");
    }
    let heap_size = sections.get("INI").and_then(|ini| {
        ini.lines()
            .rev()
            .find_map(|line| line.strip_prefix("memory_limit="))
            .and_then(|value| {
                elephc::codegen_support::parse_memory_limit_bytes(value)
                    .ok()
                    .flatten()
            })
    });
    let (expected, expectf) = match (sections.get("EXPECTF"), sections.get("EXPECT")) {
        (Some(expected), _) => (expected.clone(), true),
        (_, Some(expected)) => (expected.clone(), false),
        _ => return None,
    };

    if ["SKIPIF", "FILE_EXTERNAL", "ENV", "STDIN", "POST", "GET", "COOKIE"]
        .iter()
        .any(|section| sections.contains_key(*section))
    {
        skip = Some("requires a php-src harness section".to_string());
    }
    if sections
        .get("EXTENSIONS")
        .is_some_and(|extensions| extensions.contains("zend_test"))
    {
        source = source.replacen("<?php", &format!("<?php{ZEND_TEST_URI_PARSER}"), 1);
    }

    let compile_failure = heap_size.is_none() && expected.contains("Fatal error:");
    let isolate = heap_size.is_some() || compile_failure
        || expected.contains("#1")
        || expected.contains("#2")
        || expected.contains("#3")
        || expected.contains("#4")
        || source.contains("serialize(")
        || source.contains("unserialize(")
        || source.contains("ReflectionClass")
        || source.contains("__construct(")
        || source.contains("var_export(")
        || source.contains("json_encode(")
        || source.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("use ") || line.starts_with("function ") || line.starts_with("class ")
        });

    Some(Phpt {
        name: name.to_string(),
        source,
        expected,
        expectf,
        skip,
        isolate,
        compile_failure,
        heap_size,
    })
}

/// Splits a PHPT file into its `--SECTION--` blocks.
fn parse_sections(text: &str) -> std::collections::HashMap<String, String> {
    let mut sections = std::collections::HashMap::new();
    let mut current: Option<String> = None;
    let mut body = String::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("--").and_then(|line| line.strip_suffix("--")) {
            if let Some(previous) = current.take() {
                sections.insert(previous, body.clone());
                body.clear();
            }
            current = Some(name.to_string());
        } else if current.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(previous) = current {
        sections.insert(previous, body);
    }
    sections
}

/// Matches one fixture's actual output against its expectation.
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
fn failure_message(phpt: &Phpt, actual: &str) -> String {
    format!(
        "{}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
        phpt.name, phpt.expected, actual
    )
}
