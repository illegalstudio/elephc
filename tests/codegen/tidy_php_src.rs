//! Purpose:
//! Hand-written harness for the generated PHP 8.5.10 `ext/tidy` PHPT cases.
//!
//! Called from:
//! - `cargo test --test codegen_tests php_src_tidy`, through the test functions in
//!   `tidy_php_src_cases.rs`.
//!
//! Key details:
//! - Each fixture is compiled against the managed `libtidy` package and run in its own
//!   isolated directory with the corpus assets copied beside `test.php`.
//! - Output is captured as bytes because `020.phpt` emits Latin-1. `EXPECT` compares
//!   bytes directly; `EXPECTF`/`EXPECTREGEX` use a lossy UTF-8 view because regex
//!   placeholders cannot match invalid UTF-8 in the `regex` crate.
//! - PHP prints a blank line before displayed diagnostics; elephc's runtime does not.
//!   `normalize_php_diagnostic_spacing` restores that display-layer separator without
//!   changing any diagnostic text, matching the existing GMP PHPT harness.
//! - INI overrides are installed through `elephc::codegen::set_ini_overrides`; the
//!   compile helper feeds `tidy.default_config`, `open_basedir`, and `tidy.clean_output`
//!   into the injected tidy prelude.

use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::RegexBuilder;

use crate::support::*;

#[path = "tidy_php_src_cases.rs"]
mod cases;

/// Unique temp-directory counter.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

/// Clears the compile-time INI overrides when a case finishes.
struct IniRestore;

impl Drop for IniRestore {
    /// Restores the no-override default even when a case panics.
    fn drop(&mut self) {
        elephc::codegen::set_ini_overrides(Vec::new());
    }
}

/// Returns the committed corpus root.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/php_src/tidy")
}

/// Creates a fresh private work directory unique to this test process.
fn make_work_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let index = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let work = std::env::temp_dir().join(format!(
        "elephc_phpt_tidy_{}_{}_{index}_{stamp}",
        std::process::id(),
        index
    ));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(&work).expect("create tidy fixture directory");
    work
}

/// Copies every corpus asset the fixture may reference beside `test.php`.
fn copy_assets(work: &Path) {
    let root = corpus_root();
    for entry in fs::read_dir(&root).expect("read tidy corpus") {
        let entry = entry.expect("corpus entry");
        let path = entry.path();
        if path.is_dir() {
            copy_directory(&path, &work.join(entry.file_name()));
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".phpt")
            || matches!(
                name.as_ref(),
                "README.md" | "LICENSE" | "VERSION" | "phpt_status.json"
            )
        {
            continue;
        }
        fs::copy(&path, work.join(&*name)).expect("copy tidy corpus asset");
    }
}

/// Recursively copies one corpus subdirectory.
fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create corpus subdirectory");
    for entry in fs::read_dir(source).expect("read corpus subdirectory") {
        let entry = entry.expect("corpus subdirectory entry");
        let path = entry.path();
        if path.is_dir() {
            copy_directory(&path, &destination.join(entry.file_name()));
        } else {
            fs::copy(&path, destination.join(entry.file_name())).expect("copy corpus file");
        }
    }
}

/// Compiles and runs one fixture, returning its merged stdout/stderr bytes.
pub(super) fn run_case(
    source: &str,
    relative_dir: &str,
    ini: &[(&str, &str)],
    env: &[(&str, &str)],
    expect_fatal: bool,
) -> Vec<u8> {
    let work = make_work_dir();
    copy_assets(&work);
    if !relative_dir.is_empty() {
        fs::create_dir_all(work.join(relative_dir)).expect("create relative fixture directory");
    }
    let test_path = work.join("test.php");
    fs::write(&test_path, source).expect("write test.php");

    let work_text = work.to_string_lossy().into_owned();
    let overrides: Vec<(String, String)> = ini
        .iter()
        .map(|(key, value)| {
            (
                (*key).to_string(),
                value.replace("{PWD}", &work_text),
            )
        })
        .collect();
    elephc::codegen::set_ini_overrides(overrides.clone());
    let _restore = IniRestore;

    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, &work, 8_388_608, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let object = work.join("fixture.o");
    assemble_from_stdin(&user_asm, &object);
    let binary = work.join("test");
    link_binary(
        &object,
        &runtime_obj,
        &binary,
        &requirements,
        &default_link_paths(),
        &[],
    );

    let log_path = work.join("output.log");
    let log = fs::File::create(&log_path).expect("create output log");
    let mut command = Command::new(&binary);
    command.current_dir(&work);
    for (key, value) in env {
        command.env(key, value.replace("{PWD}", &work_text));
    }
    // A php-src fixture may deliberately end in an uncaught fatal (035.phpt); the
    // generator marks those and the status assertion flips. Every other fixture must
    // exit successfully, so a crash cannot masquerade as empty expected output.
    let status = command
        .stdout(Stdio::from(log.try_clone().expect("clone output log")))
        .stderr(Stdio::from(log))
        .status()
        .expect("run tidy fixture");
    if expect_fatal {
        assert!(!status.success(), "fixture expected a fatal error but exited {status}");
    } else {
        assert!(status.success(), "compiled tidy fixture exited with {status}");
    }
    let output = fs::read(&log_path).unwrap_or_default();
    let mut normalized = Vec::with_capacity(output.len());
    let mut index = 0;
    while index < output.len() {
        if output[index] == b'\r' && output.get(index + 1) == Some(&b'\n') {
            index += 2;
            continue;
        }
        normalized.push(output[index]);
        index += 1;
    }
    let _ = fs::remove_dir_all(&work);
    trim_ascii(&normalized)
}

/// Trims ASCII whitespace from both ends of a byte string.
fn trim_ascii(bytes: &[u8]) -> Vec<u8> {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    bytes[start..end].to_vec()
}

/// Restores PHP's display-layer blank line before a diagnostic.
fn normalize_php_diagnostic_spacing(text: &str) -> String {
    const LEVELS: &[&str] = &[
        "Deprecated: ",
        "Warning: ",
        "Notice: ",
        "Fatal error: ",
        "Parse error: ",
        "Recoverable fatal error: ",
    ];
    let mut result = String::with_capacity(text.len());
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let diagnostic = LEVELS.iter().any(|level| line.starts_with(level));
        if diagnostic && index > 0 && !result.ends_with("\n\n") {
            result.push('\n');
        }
        result.push_str(line);
    }
    result
}

/// Returns a lossy UTF-8 view with PHP diagnostic spacing restored.
fn normalized_text(bytes: &[u8]) -> String {
    normalize_php_diagnostic_spacing(&String::from_utf8_lossy(bytes))
}

/// Restores PHP's display-layer blank line before a diagnostic without decoding bytes.
fn normalize_php_diagnostic_spacing_bytes(bytes: &[u8]) -> Vec<u8> {
    const LEVELS: &[&[u8]] = &[
        b"Deprecated: ",
        b"Warning: ",
        b"Notice: ",
        b"Fatal error: ",
        b"Parse error: ",
        b"Recoverable fatal error: ",
    ];
    let mut result = Vec::with_capacity(bytes.len());
    let mut line_start = true;
    let mut index = 0;
    while index < bytes.len() {
        if line_start
            && !result.is_empty()
            && !result.ends_with(b"\n\n")
            && LEVELS.iter().any(|level| bytes[index..].starts_with(level))
        {
            result.push(b'\n');
        }
        let end = bytes[index..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| index + offset + 1);
        result.extend_from_slice(&bytes[index..end]);
        line_start = bytes[end - 1] == b'\n';
        index = end;
    }
    result
}

/// Asserts exact byte equality after diagnostic spacing normalization.
///
/// `020.phpt` emits Latin-1; decoding through `String::from_utf8_lossy` would collapse every
/// invalid byte to U+FFFD and let a wrong byte pass. Only the ASCII blank-line insertion is
/// normalized here, on the raw byte stream.
pub(super) fn assert_bytes(actual: &[u8], expected: &[u8]) {
    let actual = trim_ascii(&normalize_php_diagnostic_spacing_bytes(actual));
    let expected = trim_ascii(&normalize_php_diagnostic_spacing_bytes(expected));
    assert_eq!(
        actual,
        expected,
        "--- expected bytes ---\n{:?}\n--- actual bytes ---\n{:?}",
        expected,
        actual
    );
}

/// Compiles a php-src `--EXPECTF--` pattern into an anchored regular expression.
fn format_regex(pattern: &str) -> Result<regex::Regex, String> {
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
            'f' => regex.push_str(
                "[+-]?(?:[0-9]+(?:\\.[0-9]*)?|\\.[0-9]+)(?:[eE][+-]?[0-9]+)?",
            ),
            'x' => regex.push_str("[0-9a-fA-F]+"),
            'c' => regex.push('.'),
            'e' => regex.push_str("[/\\\\]"),
            other => return Err(format!("unsupported %{other} placeholder")),
        }
    }
    regex.push('$');
    regex::Regex::new(&regex).map_err(|error| error.to_string())
}

/// Asserts an `--EXPECTF--` match after diagnostic spacing normalization.
pub(super) fn assert_format(actual: &[u8], expected: &[u8]) {
    let pattern = normalize_php_diagnostic_spacing(&String::from_utf8_lossy(expected));
    let actual = normalized_text(actual);
    let regex = format_regex(pattern.trim_end())
        .unwrap_or_else(|error| panic!("invalid EXPECTF pattern {pattern:?}: {error}"));
    assert!(
        regex.is_match(actual.trim_end()),
        "--- pattern ---\n{}\n--- actual ---\n{}",
        pattern,
        actual
    );
}

/// Asserts an `--EXPECTREGEX--` match after diagnostic spacing normalization.
#[allow(dead_code)]
pub(super) fn assert_regex(actual: &[u8], expected: &[u8]) {
    let pattern = normalize_php_diagnostic_spacing(&String::from_utf8_lossy(expected));
    let actual = normalized_text(actual);
    let regex = RegexBuilder::new(pattern.trim_end())
        .dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTREGEX pattern {pattern:?}: {error}"));
    assert!(
        regex.is_match(actual.trim_end()),
        "--- pattern ---\n{}\n--- actual ---\n{}",
        pattern,
        actual
    );
}
