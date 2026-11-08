//! Purpose:
//! Shared runner and assertions for php-src tests rewritten as Elephc codegen tests
//! (`ext/dom`, `ext/libxml`): fixture staging, execution, and run-tests.php's
//! `EXPECT`/`EXPECTF` comparison rules.
//!
//! Called from:
//! - `dom_php_src/mod.rs` and `libxml_php_src/mod.rs`.
//!
//! Key details:
//! - `run_case_in` mirrors a whole fixture tree with symlinks (directories are materialized
//!   so a fixture cannot write through a directory link), writes the program as `test.php`
//!   in the fixture's own folder and runs it from there, so relative paths such as
//!   `__DIR__ . "/../../book.xml"` resolve as under php-src.
//! - Output stays raw bytes: PHP output is a byte stream and the encoding fixtures print
//!   Shift_JIS, GB18030 or UTF-16 bytes. `EXPECTF` patterns are therefore compiled as byte
//!   regexes whose `%` tokens match any byte, and every non-UTF-8 byte of an expectation
//!   becomes a `(?-u:\xHH)` literal.
//! - stdout and stderr share one OS file so diagnostic order is preserved.

use crate::support::*;
use regex::bytes::{Regex, RegexBuilder};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Disambiguates temporary directories across parallel tests.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

/// Heap budget of every corpus fixture: PHP's default 128 MiB memory limit.
const CORPUS_HEAP: usize = 128 * 1024 * 1024;

/// First private-use code point standing in for a raw non-UTF-8 byte while a pattern is built.
const RAW_BYTE_BASE: u32 = 0xF780;

/// Decodes `bytes` as UTF-8, replacing each invalid byte with its private-use stand-in.
fn mark_raw_bytes(mut bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    loop {
        match std::str::from_utf8(bytes) {
            Ok(valid) => {
                text.push_str(valid);
                return text;
            }
            Err(error) => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                text.push_str(std::str::from_utf8(valid).expect("valid prefix"));
                let invalid = error.error_len().unwrap_or(rest.len());
                for byte in &rest[..invalid] {
                    text.push(char::from_u32(RAW_BYTE_BASE + u32::from(*byte)).expect("stand-in"));
                }
                bytes = &rest[invalid..];
            }
        }
    }
}

/// Converts a php-src `EXPECTF` section into a byte-regex body (unanchored).
///
/// Mirrors run-tests.php: literal text is escaped, `%r...%r` regions are raw PCRE, and the
/// `%` placeholders get their run-tests meaning, widened to match arbitrary bytes.
pub(super) fn expectf_to_regex(wanted: &[u8]) -> String {
    let wanted = mark_raw_bytes(wanted).replace("\r\n", "\n");
    let mut regex = String::new();
    let mut start_offset = 0;
    let length = wanted.len();
    while start_offset < length {
        let start = wanted[start_offset..].find("%r").map(|index| start_offset + index);
        let (start, end) = match start {
            Some(start) => match wanted[start + 2..].find("%r").map(|index| start + 2 + index) {
                Some(end) => (start, end),
                None => (length, length),
            },
            None => (length, length),
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
        ("%s", r"(?-u:[^\r\n])+".to_string()),
        ("%S", r"(?-u:[^\r\n])*".to_string()),
        ("%a", r"(?s-u:.)+?".to_string()),
        ("%A", r"(?s-u:.)*?".to_string()),
        ("%w", r"\s*".to_string()),
        ("%i", r"[+-]?\d+".to_string()),
        ("%d", r"\d+".to_string()),
        ("%x", r"[0-9a-fA-F]+".to_string()),
        ("%f", r"[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[Ee][+-]?\d+)?".to_string()),
        ("%c", r"(?s-u:.)".to_string()),
        ("%0", r"\x00".to_string()),
    ] {
        regex = regex.replace(&regex::escape(token), &replacement);
    }
    regex
        .chars()
        .map(|char| match u32::from(char).checked_sub(RAW_BYTE_BASE) {
            Some(byte) if byte <= 0xFF => format!(r"(?-u:\x{byte:02X})"),
            _ => char.to_string(),
        })
        .collect()
}

/// Links one asset, materializing directories so fixtures cannot write through a dir symlink.
///
/// `.phpt` files are provenance only and are never linked.
fn link_asset(source: &Path, target: &Path) {
    if source.is_dir() {
        fs::create_dir(target).expect("fixture directory");
        for entry in fs::read_dir(source).expect("fixture directory") {
            let entry = entry.expect("fixture entry");
            if entry.path().extension().is_some_and(|extension| extension == "phpt") {
                continue;
            }
            link_asset(&entry.path(), &target.join(entry.file_name()));
        }
    } else {
        std::os::unix::fs::symlink(source, target).expect("asset symlink");
    }
}

/// Creates a fresh private work directory named after `label`.
fn fresh_work_dir(label: &str) -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    // O_EXCL + 0700 avoids the predictable-name symlink race on shared hosts.
    loop {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0);
        let candidate = std::env::temp_dir().join(format!(
            "elephc_{label}_phpt_{}_{:?}_{id}_{nanos}",
            std::process::id(),
            std::thread::current().id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => return candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temp fixture directory: {error}"),
        }
    }
}

/// Runs one corpus fixture and returns its merged, trimmed stdout/stderr bytes.
///
/// `corpus` is the committed fixture tree, `folder` the fixture's directory inside it
/// (empty for a flat corpus) and `env` its `--ENV--` section.
pub(super) fn run_case_in(
    label: &str,
    corpus: &str,
    folder: &str,
    env: &[(&str, &str)],
    source: &str,
) -> Vec<u8> {
    let work = fresh_work_dir(label);
    let tree = work.join("tests");
    link_asset(Path::new(corpus), &tree);
    let cwd = tree.join(folder);
    fs::write(cwd.join("test.php"), source).expect("fixture source");

    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, &cwd, CORPUS_HEAP, false, false);
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
    let log = fs::File::create(&log_path).expect("output log");
    let status = Command::new(&binary)
        .current_dir(&cwd)
        .envs(env.iter().copied())
        .stdout(Stdio::from(log.try_clone().expect("log handle")))
        .stderr(Stdio::from(log))
        .status()
        .expect("run fixture");
    let output = fs::read(&log_path).unwrap_or_default();
    let _ = fs::remove_dir_all(&work);
    let output = trim_bytes(&crlf_to_lf(&output)).to_vec();
    // PHP aborts on an uncaught fatal; a fixture that prints the fatal line and keeps running
    // would otherwise satisfy the message-only comparison.
    if output.windows(12).any(|window| window == b"Fatal error:") {
        assert!(
            !status.success(),
            "fatal fixture exited successfully:\n{}",
            String::from_utf8_lossy(&output)
        );
    }
    output
}

/// Replaces every CRLF pair with LF.
fn crlf_to_lf(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            continue;
        }
        output.push(*byte);
    }
    output
}

/// Strips ASCII whitespace at both ends, as run-tests.php's `trim()` does.
fn trim_bytes(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

/// Matches a `var_dump()` object handle: `object(Class)#N`.
fn handle_regex() -> Regex {
    Regex::new(r"object\(([^)\r\n]*)\)#\d+").expect("object handle regex")
}

/// Rewrites `var_dump()` object handles to the opaque `#N` placeholder.
///
/// Handles are allocation order, not observable program state, and php-src reuses them.
/// The match is scoped to `object(...)#N` because a bare `#<digits>` also appears in
/// argument numbers (`Argument #1`) and character references (`&#38;`), which must stay
/// literal for the assertion to mean anything.
fn normalize_handles(text: &[u8]) -> Vec<u8> {
    handle_regex().replace_all(text, &b"object($1)#N"[..]).into_owned()
}

/// Asserts an exact php-src `EXPECT`, treating object handles as opaque identities.
pub(super) fn assert_exact(actual: impl AsRef<[u8]>, expected: impl AsRef<[u8]>) {
    let actual = normalize_handles(actual.as_ref());
    let expected = normalize_handles(trim_bytes(&crlf_to_lf(expected.as_ref())));
    assert!(
        actual == expected,
        "--- expected ---\n{}\n--- actual ---\n{}",
        String::from_utf8_lossy(&expected),
        String::from_utf8_lossy(&actual)
    );
}

/// Asserts one php-src `EXPECTF` pattern against a fixture's merged output.
pub(super) fn assert_format(actual: impl AsRef<[u8]>, expected: impl AsRef<[u8]>) {
    let actual = actual.as_ref();
    // php-src's EXPECTF usually writes `object(Foo)#%d`, but a few fixtures embed one concrete
    // handle. Rewrite only `object(...)#N` so `Argument #1` and `&#38;` keep their literal value.
    let expected = handle_regex()
        .replace_all(trim_bytes(expected.as_ref()), &b"object($1)#%d"[..])
        .into_owned();
    let pattern = format!("^{}$", expectf_to_regex(&expected));
    let regex = RegexBuilder::new(&pattern)
        .dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTF regex {pattern}: {error}"));
    assert!(
        regex.is_match(actual),
        "--- pattern ---\n{pattern}\n--- expected ---\n{}\n--- actual ---\n{}",
        String::from_utf8_lossy(&expected),
        String::from_utf8_lossy(actual)
    );
}

/// Asserts a fatal fixture's message line.
///
/// PHP's uncaught trace lists interpreter frames; Elephc's native frames are a different
/// implementation detail. The regression a corpus owns is the fatal message and its source
/// location, so only that line is compared against the expected `EXPECTF` prefix.
pub(super) fn assert_fatal_format(actual: impl AsRef<[u8]>, expected: impl AsRef<[u8]>) {
    let expected = trim_bytes(expected.as_ref());
    let expected_line = expected.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let actual_line = actual.as_ref().split(|byte| *byte == b'\n').next().unwrap_or_default();
    let pattern = format!("^{}$", expectf_to_regex(expected_line));
    let regex = RegexBuilder::new(&pattern)
        .build()
        .unwrap_or_else(|error| panic!("invalid fatal EXPECTF regex {pattern}: {error}"));
    assert!(
        regex.is_match(actual_line),
        "--- pattern ---\n{pattern}\n--- expected ---\n{}\n--- actual first line ---\n{}",
        String::from_utf8_lossy(expected_line),
        String::from_utf8_lossy(actual_line)
    );
}

/// Verifies handle normalization cannot swallow non-handle `#<digits>` text.
#[test]
fn test_handle_normalization_keeps_argument_numbers_and_entities_literal() {
    assert_eq!(
        normalize_handles(b"object(SimpleXMLElement)#12 (0) {\n}"),
        b"object(SimpleXMLElement)#N (0) {\n}".to_vec()
    );
    assert_eq!(
        normalize_handles(b"Argument #1 ($qualifiedName) and a &#38; b"),
        b"Argument #1 ($qualifiedName) and a &#38; b".to_vec()
    );
}

/// Verifies expectations and placeholders match raw non-UTF-8 output bytes exactly.
#[test]
fn test_expectations_match_raw_non_utf8_bytes() {
    assert_exact(b"\x82\xa0 ok", b"\x82\xa0 ok\n");
    assert_format(b"\x82\xa0 line 3 \xff\xfe", b"\x82\xa0 line %d %s");
    assert_format("caf\u{e9} 1", "caf\u{e9} %d");
    assert!(std::panic::catch_unwind(|| assert_format(b"\x82\xa1", b"\x82\xa0")).is_err());
    assert!(std::panic::catch_unwind(|| assert_exact(b"\x82\xa1", b"\x82\xa0")).is_err());
}
