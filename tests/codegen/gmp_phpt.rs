//! Purpose:
//! Runs the vendored php-src 8.5 `ext/gmp/tests` PHPT corpus through the elephc backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_gmp_php_src_phpt_corpus -- --ignored` and
//!   the dedicated `gmp-phpt-corpus` CI job.
//!
//! Key details:
//! - The PHPT files are the source of truth; do not edit them to make a run pass.
//! - `--FILE--`, `--EXPECT--`, `--EXPECTF--` and the platform-relevant `--SKIPIF--`
//!   predicates are interpreted. `--INI--`, `--ENV--` and `--EXPECTREGEX--` do not
//!   appear in this corpus and are rejected if a refresh introduces them.
//! - Object handle numbers (`object(GMP)#3`) are an engine allocation detail, not part
//!   of the GMP contract. They are normalised unless `GMP_PHPT_STRICT_HANDLES=1`.
//! - elephc keeps no call stack, so an expectation that ends in a PHP stack trace is
//!   compared on its uncaught class and message only (`bug70284`), and an `%A` tail
//!   that only stands for that trace is accepted as empty (`gh16411`). The GMP
//!   semantic assertion — the exception class and text — stays byte-exact.
//! - `GMP_PHPT_FILTER` selects file names by substring, `GMP_PHPT_LIST=1` prints the
//!   corpus, and `GMP_PHPT_INCLUDE_EXCLUDED=1` runs files listed in `EXCLUDED`.
//! - Each case is compiled and run in its own isolated directory by the shared codegen
//!   runner; a compile panic is recorded as that case's failure and does not abort the
//!   remaining corpus.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::support::compile_and_run_capture_merged_with_gmp;

/// Directory holding the vendored php-src GMP PHPT files.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/php-src-8.5.6/ext/gmp/tests")
}

/// One parsed `.phpt` fixture.
struct Phpt {
    /// File name, used in filters and failure reports.
    name: String,
    /// `--TEST--` title.
    title: String,
    /// `--SKIPIF--` PHP source, when present.
    skipif: Option<String>,
    /// `--FILE--` PHP source.
    file: String,
    /// Expected output and its comparison mode.
    expected: Expected,
}

/// Expected-output comparison mode.
enum Expected {
    /// Byte-for-byte output after handle normalisation.
    Exact(String),
    /// php-src `--EXPECTF--` format string.
    Format(String),
}

/// One failed corpus case.
struct Failure {
    /// File name.
    name: String,
    /// Human-readable reason, including a bounded output diff.
    reason: String,
}

/// Files deliberately excluded from the default run, with a concrete reason.
const EXCLUDED: &[(&str, &str)] = &[
    (
        "gmp_setbit_long.phpt",
        "the fixture allocates a multi-gigabyte number and its SKIPIF requires /proc/meminfo",
    ),
];

/// Runs the whole vendored GMP PHPT corpus and reports every failing file.
///
/// This is a multi-minute corpus, so the sharded codegen suite skips it; the dedicated
/// `gmp-phpt-corpus` CI job runs it with `--ignored` and a 45-minute budget.
#[test]
#[ignore = "slow: runs in the dedicated GMP PHPT corpus job"]
fn test_gmp_php_src_phpt_corpus() {
    if crate::support::skip_without_gmp_native("gmp_php_src_phpt_corpus") {
        return;
    }
    let filter = std::env::var("GMP_PHPT_FILTER").ok();
    let list_only = std::env::var_os("GMP_PHPT_LIST").is_some();
    let include_excluded = std::env::var_os("GMP_PHPT_INCLUDE_EXCLUDED").is_some();
    let mut paths = phpt_paths();
    paths.sort();

    let mut failures = Vec::new();
    let mut ran = 0usize;
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if let Some(filter) = &filter {
            if !name.contains(filter.as_str()) {
                continue;
            }
        }
        let phpt = match parse_phpt(&path) {
            Ok(phpt) => phpt,
            Err(reason) => {
                failures.push(Failure { name, reason });
                continue;
            }
        };
        if list_only {
            println!("{}: {}", phpt.name, phpt.title);
            continue;
        }
        if !include_excluded {
            if let Some((_, reason)) = EXCLUDED.iter().find(|(excluded, _)| *excluded == phpt.name) {
                println!("SKIP {}: {reason}", phpt.name);
                continue;
            }
        }
        if skipif_says_skip(&phpt) {
            println!("SKIP {}: SKIPIF", phpt.name);
            continue;
        }
        ran += 1;
        println!("compile {}", phpt.name);
        match catch_unwind(AssertUnwindSafe(|| run_case(&phpt))) {
            Ok(Ok(())) => println!("ok {}", phpt.name),
            Ok(Err(reason)) => failures.push(Failure { name: phpt.name, reason }),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|value| value.to_string()))
                    .unwrap_or_else(|| "non-string panic payload".to_string());
                if compile_fatal_matches(&phpt.expected, &message) {
                    println!("ok {} (AOT compile-time fatal)", phpt.name);
                } else {
                    failures.push(Failure {
                        name: phpt.name,
                        reason: format!("compilation panicked: {message}"),
                    });
                }
            }
        }
    }

    if list_only {
        return;
    }
    assert!(
        !failures.is_empty() || ran > 0,
        "GMP PHPT corpus produced no runnable cases (filter={filter:?})"
    );
    if failures.is_empty() {
        println!("GMP PHPT corpus: {ran} cases passed");
        return;
    }
    let mut report = format!(
        "GMP PHPT corpus: {ran} ran, {} failed\n",
        failures.len()
    );
    for failure in &failures {
        report.push_str(&format!("\n=== {} ===\n{}\n", failure.name, failure.reason));
    }
    panic!("{report}");
}

/// Returns every `.phpt` path in the vendored corpus directory.
fn phpt_paths() -> Vec<PathBuf> {
    let dir = corpus_dir();
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read GMP PHPT corpus {}: {error}", dir.display()))
    {
        let path = entry.expect("read GMP PHPT directory entry").path();
        if path.extension().is_some_and(|extension| extension == "phpt") {
            paths.push(path);
        }
    }
    paths
}

/// Parses one PHPT file into the sections the runner understands.
fn parse_phpt(path: &Path) -> Result<Phpt, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in source.split_inclusive('\n') {
        let header = line.trim_end_matches('\n');
        if let Some(section) = header.strip_prefix("--").and_then(|line| line.strip_suffix("--")) {
            if section.chars().all(|ch| ch.is_ascii_uppercase()) && !section.is_empty() {
                if let Some((name, content)) = current.take() {
                    sections.push((name, content));
                }
                current = Some((section.to_string(), String::new()));
                continue;
            }
        }
        if let Some((_, content)) = current.as_mut() {
            content.push_str(line);
        }
    }
    if let Some((name, content)) = current.take() {
        sections.push((name, content));
    }

    let section = |name: &str| {
        sections
            .iter()
            .find(|(section, _)| section == name)
            .map(|(_, content)| content.clone())
    };
    for rejected in ["INI", "ENV", "EXPECTREGEX"] {
        if sections.iter().any(|(section, _)| section == rejected) {
            return Err(format!(
                "{name} uses --{rejected}--, which the runner does not implement"
            ));
        }
    }
    let title = section("TEST").ok_or_else(|| format!("{name} has no --TEST-- section"))?;
    let file = section("FILE").ok_or_else(|| format!("{name} has no --FILE-- section"))?;
    let expected = if let Some(expected) = section("EXPECT") {
        Expected::Exact(expected)
    } else if let Some(expected) = section("EXPECTF") {
        Expected::Format(expected)
    } else {
        return Err(format!("{name} has neither --EXPECT-- nor --EXPECTF--"));
    };
    Ok(Phpt {
        name,
        title,
        skipif: section("SKIPIF"),
        file,
        expected,
    })
}

/// Applies the corpus' platform predicates without executing arbitrary SKIPIF PHP.
fn skipif_says_skip(phpt: &Phpt) -> bool {
    let Some(skipif) = &phpt.skipif else {
        return false;
    };
    // Every supported elephc target is 64-bit, so PHP_INT_SIZE is always 8. The other
    // predicates are GMP version checks (libgmp 6.3.0 satisfies 4.2.1) or memory probes
    // handled by EXCLUDED.
    let requests_skip = skipif.contains("die(\"skip") || skipif.contains("print \"skip\"");
    requests_skip && !skipif.contains("PHP_INT_SIZE") && !skipif.contains("GMP_VERSION")
}

/// Compiles and runs one case, returning a bounded diff on mismatch.
fn run_case(phpt: &Phpt) -> Result<(), String> {
    let source = adapt_phpt_source(&phpt.file);
    let output = compile_and_run_capture_merged_with_gmp(&source);
    let actual = normalize_php_diagnostic_spacing(&output.stdout);
    let mut expected = match &phpt.expected {
        Expected::Exact(expected) if expected.contains("Stack trace:") => {
            return compare_fatal_without_trace(expected, &actual);
        }
        Expected::Exact(expected) => expected.clone(),
        Expected::Format(format) if format.contains("Stack trace:") => {
            return compare_fatal_without_trace(format, &actual);
        }
        Expected::Format(format) => return compare_format(&phpt.name, format, &actual),
    };
    let mut actual = actual;
    if std::env::var_os("GMP_PHPT_STRICT_HANDLES").is_none() {
        expected = normalize_object_handles(&expected);
        actual = normalize_object_handles(&actual);
    }
    expected = expected.trim_matches('\n').to_string();
    actual = actual.trim_matches('\n').to_string();
    if actual == expected {
        return Ok(());
    }
    Err(format!(
        "stdout mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
        bounded(&expected),
        bounded(&actual)
    ))
}

/// Compares the uncaught class and message when php-src also expects a stack trace.
///
/// elephc deliberately keeps no call stack, so `Stack trace:` frames and the trailing
/// `thrown in` line have no honest rendering (see
/// `codegen_support::runtime::exceptions::uncaught_report`). The GMP corpus case that
/// asserts a stack trace is a use-after-free regression: the semantic requirement is that
/// `unserialize()` throws `Exception: Could not unserialize number` instead of returning
/// attacker-shaped data. This keeps that class and message byte-exact and ignores only the
/// engine-level trace that elephc does not implement.
fn compare_fatal_without_trace(expected: &str, actual: &str) -> Result<(), String> {
    let expected_head = expected
        .trim_matches('\n')
        .lines()
        .next()
        .unwrap_or_default()
        .split(" in ")
        .next()
        .unwrap_or_default();
    let actual_head = actual
        .trim_matches('\n')
        .lines()
        .next()
        .unwrap_or_default()
        .split(" in ")
        .next()
        .unwrap_or_default();
    if expected_head.starts_with("Fatal error: Uncaught ") && expected_head == actual_head {
        return Ok(());
    }
    Err(format!(
        "uncaught fatal mismatch\n--- expected head ---\n{expected_head}\n--- actual head ---\n{actual_head}"
    ))
}

/// Matches one `--EXPECTF--` pattern, mirroring php-src's format placeholders.
fn compare_format(name: &str, pattern: &str, actual: &str) -> Result<(), String> {
    let pattern = pattern.trim_matches('\n');
    let actual = actual.trim_matches('\n');
    let regex = format_regex(pattern)
        .map_err(|reason| format!("invalid --EXPECTF-- pattern in {name}: {reason}"))?;
    if regex.is_match(actual) {
        return Ok(());
    }
    // A fixture whose only tail is `%A` accepts any engine trace text. elephc renders the
    // fatal class, message and location but keeps no call stack, so the tail is legitimately
    // empty. The first line is still matched byte-for-byte through its own anchored pattern.
    if expects_engine_trace_tail(pattern) {
        let head_pattern = pattern.lines().next().unwrap_or_default();
        let head_actual = actual.lines().next().unwrap_or_default();
        if format_regex(head_pattern)
            .map(|head| head.is_match(head_actual))
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
    Err(format!(
        "stdout does not match --EXPECTF--\n--- pattern ---\n{}\n--- actual ---\n{}",
        bounded(pattern),
        bounded(actual)
    ))
}

/// Reports whether an `--EXPECTF--` pattern leaves only an engine trace as its tail.
///
/// The pattern must have at least one `%A`/`%a` on a line after the first and nothing else
/// but whitespace there. The uncaught-fatal reporters are the only place elephc deliberately
/// omits PHP's trace, and their first line is still compared normally.
fn expects_engine_trace_tail(pattern: &str) -> bool {
    let mut lines = pattern.lines();
    let _head = lines.next();
    let tail = lines.collect::<Vec<_>>().join("\n");
    let trace = tail.contains("%A") || tail.contains("%a");
    trace && tail.replace("%A", "").replace("%a", "").trim().is_empty()
}

/// Compiles a php-src `--EXPECTF--` string into an anchored regular expression.
fn format_regex(pattern: &str) -> Result<Regex, String> {
    let mut regex = String::from("(?s)^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            regex.push_str(&regex::escape(&ch.to_string()));
            continue;
        }
        let Some(specifier) = chars.next() else {
            return Err("trailing '%'".to_string());
        };
        match specifier {
            '%' => regex.push('%'),
            's' => regex.push_str("[^\\n]+"),
            'S' => regex.push_str("[^\\n]*"),
            'a' => regex.push_str("[\\s\\S]+"),
            'A' => regex.push_str("[\\s\\S]*"),
            'w' => regex.push_str("\\s*"),
            'd' | 'i' => regex.push_str("[+-]?[0-9]+"),
            'f' => regex.push_str("[+-]?(?:[0-9]+(?:\\.[0-9]*)?|\\.[0-9]+)(?:[eE][+-]?[0-9]+)?"),
            'x' => regex.push_str("[0-9a-fA-F]+"),
            'c' => regex.push('.'),
            'e' => regex.push_str("[/\\\\]"),
            other => return Err(format!("unsupported %{other} placeholder")),
        }
    }
    regex.push('$');
    Regex::new(&regex).map_err(|error| error.to_string())
}

/// Inserts PHP CLI's blank line before a displayed diagnostic.
///
/// PHP's `display_errors` prefix is `\n<Level>: ...` whenever output has already been
/// written, which is why PHPT expectations contain a blank line before every warning or
/// deprecation that follows a value dump. elephc writes the same diagnostic text to
/// stderr without that separator; after merging the streams the runner restores the
/// display-layer blank line without changing the message or its order.
fn normalize_php_diagnostic_spacing(output: &str) -> String {
    const LEVELS: &[&str] = &[
        "Deprecated: ",
        "Warning: ",
        "Notice: ",
        "Fatal error: ",
        "Parse error: ",
        "Recoverable fatal error: ",
    ];
    let mut result = String::with_capacity(output.len());
    for (index, line) in output.split_inclusive('\n').enumerate() {
        let diagnostic = LEVELS.iter().any(|level| line.starts_with(level));
        if diagnostic && index > 0 && !result.ends_with("\n\n") {
            result.push('\n');
        }
        result.push_str(line);
    }
    result
}

/// Rewrites PHPT's optional closing tag and trailing inline HTML into plain PHP.
///
/// elephc has no inline-HTML mode, while a few php-src tests close the PHP block and emit
/// a literal marker. Keep the marker by turning it into an `echo` after the PHP code.
fn adapt_phpt_source(source: &str) -> String {
    let Some((php, inline)) = source.split_once("?>") else {
        return source.trim_end().to_string();
    };
    let inline = inline.strip_prefix('\n').unwrap_or(inline);
    if inline.trim().is_empty() {
        return php.trim_end().to_string();
    }
    let escaped = inline.replace('\\', "\\\\").replace('\'', "\\'");
    format!("{}\necho '{}';", php.trim_end(), escaped)
}

/// Maps a compile-time AOT rejection onto php-src's runtime `Fatal error:` expectation.
///
/// AOT has no process to abort, so a construct PHP rejects at run time (extending a final
/// class) surfaces as a compile diagnostic. The semantic rejection is still exact; only
/// the display prefix differs. This accepts the case only when the expected fatal's
/// message text is present in the compiler diagnostic, never on a bare class name.
fn compile_fatal_matches(expected: &Expected, message: &str) -> bool {
    let text = match expected {
        Expected::Exact(text) | Expected::Format(text) => text,
    };
    let Some(rest) = text.trim_start().strip_prefix("Fatal error: ") else {
        return false;
    };
    let core = rest.split(" in ").next().unwrap_or(rest).trim();
    !core.is_empty() && message.contains(core)
}

/// Replaces engine object-handle numbers with a stable placeholder.
fn normalize_object_handles(output: &str) -> String {
    static HANDLE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let handle = HANDLE.get_or_init(|| Regex::new(r"object\(GMP\)#[0-9]+").unwrap());
    handle.replace_all(output, "object(GMP)#N").into_owned()
}

/// Truncates a failure payload so one noisy case cannot bury the rest of the report.
fn bounded(text: &str) -> String {
    const LIMIT: usize = 4_000;
    if text.len() <= LIMIT {
        return text.to_string();
    }
    let mut end = LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... [truncated]", &text[..end])
}
