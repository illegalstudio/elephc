//! Purpose:
//! Runs the vendored php-src ext/tokenizer `.phpt` suite against the compiled elephc binary.
//!
//! Called from:
//! - `cargo test --test tokenizer_php_src -- --ignored` in the tokenizer CI job.
//!
//! Key details:
//! - Fixtures live under `tests/fixtures/php-src-tokenizer` (PHP License 3.01).
//! - No PHP installation or network access is needed: EXPECT/EXPECTF are vendored.
//! - Cases run with two workers; a failure names the `.phpt` file and the mismatch.

use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

/// Vendored php-src tokenizer fixture directory.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/php-src-tokenizer"
);

/// Compiles and runs every vendored php-src tokenizer case.
///
/// The suite is ignored in the ordinary test run because it spawns one compiler per
/// `.phpt`; the dedicated tokenizer CI job runs it explicitly with `--ignored`.
#[test]
#[ignore = "vendored php-src tokenizer phpt parity; run in the dedicated CI job"]
fn php_src_tokenizer_phpt_suite() {
    let elephc = PathBuf::from(env!("CARGO_BIN_EXE_elephc"));
    let mut cases: Vec<PathBuf> = fs::read_dir(FIXTURES)
        .expect("read vendored php-src tokenizer fixtures")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "phpt"))
        .collect();
    cases.sort();
    assert_eq!(cases.len(), 54, "the vendored php-src tokenizer corpus is incomplete");

    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(2, |count| count.get().min(2));
    let next = &next;
    let failures = &failures;
    let cases = &cases;
    let elephc = &elephc;
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(case) = cases.get(index) else { break; };
                let name = case.file_name().unwrap_or_default().to_string_lossy().into_owned();
                match run_case(case, elephc, index) {
                    Ok(()) => println!("ok {name}"),
                    Err(detail) => {
                        println!("FAIL {name}: {detail}");
                        failures.lock().unwrap().push((name, detail));
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

/// Compiles one `.phpt` file, runs the resulting binary, and checks its expected output.
fn run_case(path: &Path, elephc: &Path, index: usize) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("read: {error}"))?;
    let sections = parse_sections(&text);
    let file = sections
        .get("FILE")
        .ok_or_else(|| "missing --FILE--".to_string())?;
    let source = phpt_source(file);
    let directory = TempDir::new(index);
    let source_path = directory.path().join("test.php");
    fs::write(&source_path, source.as_bytes()).map_err(|error| format!("write: {error}"))?;

    let compiled = Command::new(elephc)
        .arg(&source_path)
        .output()
        .map_err(|error| format!("spawn elephc: {error}"))?;
    if !compiled.status.success() {
        if expected_is_fatal(&sections) {
            let expected = expected_text(&sections).unwrap_or("");
            let core = fatal_core(expected);
            let stderr = String::from_utf8_lossy(&compiled.stderr);
            if !core.is_empty() && stderr.contains(&core) {
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

    if expected_is_fatal(&sections) {
        let expected = expected_text(&sections)
            .ok_or_else(|| "missing fatal expectation".to_string())?;
        let first = first_fatal_line(got);
        let pattern = expectf(expected.lines().next().unwrap_or(""));
        let matches = full_match(&pattern, first)?;
        return if matches && !ran.status.success() {
            Ok(())
        } else {
            Err(format!("fatal line differs: {first:?}"))
        };
    }

    if !ran.status.success() {
        return Err(format!(
            "exit {}; got {got:?}",
            ran.status.code().unwrap_or(-1)
        ));
    }

    if let Some(expected) = sections.get("EXPECT") {
        let expected = expected.trim_end_matches('\n');
        return if got == expected {
            Ok(())
        } else {
            Err(format!("EXPECT mismatch; expected {expected:?}; got {got:?}"))
        };
    }
    if let Some(pattern) = sections.get("EXPECTF") {
        let pattern = expectf(pattern.trim_end_matches('\n'));
        return if full_match(&pattern, got)? {
            Ok(())
        } else {
            Err(format!("EXPECTF mismatch; got {got:?}"))
        };
    }
    if let Some(pattern) = sections.get("EXPECTREGEX") {
        return if full_match(pattern.trim_end_matches('\n'), got)? {
            Ok(())
        } else {
            Err(format!("EXPECTREGEX mismatch; got {got:?}"))
        };
    }
    Err("missing --EXPECT--/--EXPECTF--/--EXPECTREGEX--".to_string())
}

/// Splits a `.phpt` file into its `--SECTION--` bodies.
fn parse_sections(text: &str) -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut body = String::new();
    for line in text.split_inclusive('\n') {
        let marker = line.trim_end_matches(['\r', '\n']);
        if let Some(name) = marker
            .strip_prefix("--")
            .and_then(|marker| marker.strip_suffix("--"))
        {
            if !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_uppercase() || character == '_')
            {
                if let Some(previous) = current.take() {
                    sections.insert(previous, std::mem::take(&mut body));
                }
                current = Some(name.to_string());
                continue;
            }
        }
        if current.is_some() {
            body.push_str(line);
        }
    }
    if let Some(previous) = current {
        sections.insert(previous, body);
    }
    sections
}

/// Replaces a trailing close tag with a semicolon so elephc can compile the fixture body.
fn phpt_source(file: &str) -> String {
    let trimmed = file.trim_end();
    match trimmed.strip_suffix("?>") {
        Some(prefix) => format!("{};\n", prefix.trim_end()),
        None => file.to_string(),
    }
}

/// Returns the expected body when the case declares one.
fn expected_text(sections: &BTreeMap<String, String>) -> Option<&str> {
    sections
        .get("EXPECT")
        .or_else(|| sections.get("EXPECTF"))
        .map(String::as_str)
}

/// Returns whether this case expects a PHP fatal diagnostic.
fn expected_is_fatal(sections: &BTreeMap<String, String>) -> bool {
    expected_text(sections)
        .is_some_and(|expected| expected.trim_start().starts_with("Fatal error:"))
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
            "elephc_tokenizer_phpt_{}_{}",
            std::process::id(),
            index
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create phpt temp directory");
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
