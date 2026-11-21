//! Purpose:
//! Runs the php-src ext/tokenizer corpus, transcribed into Rust, against the compiled binary.
//!
//! Called from:
//! - `cargo test --test tokenizer_php_src -- --ignored` in the tokenizer CI job.
//!
//! Key details:
//! - Cases and expectations live in `tests/tokenizer_php_src/cases.rs`; no `.phpt` file is
//!   read at run time.
//! - Derived from php-src `ext/tokenizer/tests` (PHP License 3.01; see the sibling LICENSE).
//! - Cases run with two workers; a failure names the original php-src test.

#[path = "tokenizer_php_src/cases.rs"]
mod cases;

use cases::{Case, CASES};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

/// Compiles and runs every transcribed php-src tokenizer case.
///
/// The suite is ignored in the ordinary test run because it spawns one compiler per case;
/// the dedicated tokenizer CI job runs it explicitly with `--ignored`.
#[test]
#[ignore = "php-src tokenizer corpus parity; run in the dedicated CI job"]
fn php_src_tokenizer_corpus() {
    // The corpus is transcribed data, so a truncated or empty `cases.rs` must not pass silently.
    assert_eq!(CASES.len(), 54, "php-src tokenizer corpus lost cases");
    let elephc = std::env::var_os("ELEPHC_TEST_COMPILER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_elephc")));

    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(2, |count| count.get().min(2));
    let next = &next;
    let failures = &failures;
    let elephc = &elephc;
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(case) = CASES.get(index) else { break; };
                match run_case(case, elephc, index) {
                    Ok(()) => println!("ok {}", case.name),
                    Err(detail) => {
                        println!("FAIL {}: {detail}", case.name);
                        failures.lock().unwrap().push((case.name, detail));
                    }
                }
            });
        }
    });

    let failures = std::mem::take(&mut *failures.lock().unwrap());
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .map(|(name, detail)| format!("{name}: {detail}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

/// Compiles one transcribed case, runs the resulting binary, and checks its expected output.
fn run_case(case: &Case, elephc: &Path, index: usize) -> Result<(), String> {
    let directory = TempDir::new(index);
    let source_path = directory.path().join("test.php");
    let source = if case.name == "token_get_all_TOKEN_PARSE_oom_feedback" {
        // The PHPT uses Zend's tracked allocator to set a limit below one heap chunk.
        // A valid native limit and a larger token stream exercise the tokenizer's memory guard.
        r#"<?php
$code = '<?php X::continue; ' . str_repeat('$a = 1; ', 8000);
ini_set('memory_limit', '4M');
token_get_all($code, TOKEN_PARSE);
"#
    } else {
        case.source
    };
    fs::write(&source_path, source.as_bytes()).map_err(|error| format!("write: {error}"))?;

    let compiled = Command::new(elephc)
        .arg("build")
        .arg(&source_path)
        .output()
        .map_err(|error| format!("spawn elephc: {error}"))?;
    if !compiled.status.success() {
        if expected_is_fatal(case.expected) {
            let expected = case.expected;
            let core = fatal_core(expected);
            let stderr = String::from_utf8_lossy(&compiled.stderr);
            if !core.is_empty() && stderr.contains(&core) {
                if let (Some(actual), Some(expected_line)) = (
                    compile_error_line(&stderr),
                    expected_fatal_line(expected, source),
                ) {
                    if actual != expected_line {
                        return Err(format!(
                            "fatal line differs: compiler reported {actual}, expected {expected_line}"
                        ));
                    }
                }
                return Ok(());
            }
        }
        return Err(format!("compile: {}", last_line(&compiled.stderr)));
    }

    let binary = directory.path().join("test");
    let ran = Command::new(&binary)
        .output()
        .map_err(|error| format!("run: {error}"))?;
    let got = String::from_utf8_lossy(&ran.stdout);
    let got = got.trim_end_matches('\n');

    if expected_is_fatal(case.expected) {
        let stderr = String::from_utf8_lossy(&ran.stderr);
        let first_stdout = first_fatal_line(got);
        let first_stderr = first_fatal_line(&stderr);
        let pattern = expectf(case.expected.lines().next().unwrap_or(""));
        let matches = full_match(&pattern, first_stdout)? || full_match(&pattern, first_stderr)?;
        return if matches && !ran.status.success() {
            Ok(())
        } else {
            Err(format!("fatal line differs: stdout {first_stdout:?}, stderr {first_stderr:?}"))
        };
    }

    if !ran.status.success() {
        return Err(format!(
            "exit {}; got {got:?}",
            ran.status.code().unwrap_or(-1)
        ));
    }

    if case.expectf {
        let pattern = expectf(case.expected);
        return if full_match(&pattern, got)? {
            Ok(())
        } else {
            Err(format!("EXPECTF mismatch; got {got:?}"))
        };
    }

    if got == case.expected {
        Ok(())
    } else {
        Err(format!(
            "EXPECT mismatch; expected {:?}; got {got:?}",
            case.expected
        ))
    }
}

/// Returns whether this case expects a PHP fatal diagnostic.
fn expected_is_fatal(expected: &str) -> bool {
    expected.trim_start().starts_with("Fatal error:")
}

/// Extracts the stable part of a PHP fatal message before its `in file on line` suffix.
fn fatal_core(expected: &str) -> String {
    let first = expected.lines().next().unwrap_or("").trim_start();
    let core = first.strip_prefix("Fatal error:").unwrap_or(first).trim();
    core.split(" in ")
        .next()
        .unwrap_or(core)
        .trim_end_matches("()")
        .to_string()
}

/// Extracts the line from elephc's `path:line:column` diagnostic prefix.
fn compile_error_line(stderr: &str) -> Option<usize> {
    let pattern = Regex::new(r":(\d+):\d+\]").ok()?;
    pattern
        .captures(stderr)
        .and_then(|captures| captures.get(1))
        .and_then(|line| line.as_str().parse().ok())
}

/// Extracts a concrete expected fatal line from an `EXPECTF` pattern and source.
///
/// `%d` does not pin a line, so the line is recovered from a concrete
/// `on line N` or from the source line declaring the method named after `::`.
fn expected_fatal_line(expected: &str, source: &str) -> Option<usize> {
    let first = expected.lines().next().unwrap_or("");
    if let Some(line) = Regex::new(r"on line (\d+)")
        .ok()
        .and_then(|pattern| pattern.captures(first))
        .and_then(|captures| captures.get(1))
        .and_then(|line| line.as_str().parse().ok())
    {
        return Some(line);
    }
    let method = Regex::new(r"::([A-Za-z_][A-Za-z0-9_]*)")
        .ok()
        .and_then(|pattern| pattern.captures(first))
        .and_then(|captures| captures.get(1))
        .map(|method| method.as_str().to_string())?;
    source
        .lines()
        .position(|line| line.contains(&format!("function {method}")))
        .map(|index| index + 1)
}

/// Returns the first nonempty output line.
fn first_fatal_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
}

/// Returns the last line of a diagnostic byte stream.
fn last_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .last()
        .unwrap_or("")
        .to_string()
}

/// Matches a translated `EXPECTF` pattern against the complete output.
fn full_match(pattern: &str, text: &str) -> Result<bool, String> {
    let regex = Regex::new(&format!("(?s)^(?:{pattern})$"))
        .map_err(|error| format!("invalid EXPECTF regex: {error}"))?;
    Ok(regex.is_match(text))
}

/// Translates PHP's `EXPECTF` placeholders into a regular expression.
fn expectf(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '%' && index + 1 < chars.len() {
            let code = chars[index + 1];
            if code == 'r' {
                let start = index + 2;
                let mut end = start;
                let mut closed = false;
                while end + 1 < chars.len() {
                    if chars[end] == '%' && chars[end + 1] == 'r' {
                        closed = true;
                        break;
                    }
                    end += 1;
                }
                if !closed {
                    let rest: String = chars[index..].iter().collect();
                    out.push_str(&regex::escape(&rest));
                    break;
                }
                out.push_str("(?:");
                out.extend(&chars[start..end]);
                out.push(')');
                index = end + 2;
                continue;
            }
            let replacement = match code {
                'd' => Some(r"\d+".to_string()),
                'i' => Some(r"[+-]?\d+".to_string()),
                's' => Some(r"[^\r\n]+".to_string()),
                'S' => Some(r"[^\r\n]*".to_string()),
                'a' => Some(r".+?".to_string()),
                'A' => Some(r".*?".to_string()),
                'w' => Some(r"\s*".to_string()),
                'f' => Some(r"[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[Ee][+-]?\d+)?".to_string()),
                'x' => Some(r"[0-9a-fA-F]+".to_string()),
                'c' => Some(r".".to_string()),
                'e' => Some(regex::escape(&std::path::MAIN_SEPARATOR.to_string())),
                '%' => {
                    out.push('%');
                    index += 2;
                    continue;
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                out.push_str("(?:");
                out.push_str(&replacement);
                out.push(')');
                index += 2;
                continue;
            }
        }
        out.push_str(&regex::escape(&chars[index].to_string()));
        index += 1;
    }
    out
}

/// Owns one isolated temporary directory and removes it on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a unique directory for one compiled fixture.
    fn new(index: usize) -> Self {
        let path = std::env::temp_dir().join(format!(
            "elephc_tokenizer_php_src_{}_{}",
            std::process::id(),
            index
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create tokenizer corpus temp directory");
        Self { path }
    }

    /// Returns the directory path.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    /// Removes the fixture binary and source after one case finishes.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
