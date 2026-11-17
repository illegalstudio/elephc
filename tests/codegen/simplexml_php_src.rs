//! Purpose:
//! Compiles and runs the committed PHP 8.5.8 `ext/simplexml/tests` cases.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - The cases live in the generated sibling module `simplexml_php_src_cases.rs`; each
//!   test carries its PHP source and assertion inline, matching the ordinary Elephc
//!   `compile_and_run` fixture style.
//! - Every fixture gets an isolated temp directory with the committed XML/DTD/entity
//!   assets symlinked beside a `test.php` entry file, so `__DIR__` resolves as under
//!   php-src.
//! - stdout and stderr share one OS file so diagnostic order is preserved.
//! - The `.phpt` files under `tests/php_src/simplexml/` remain the provenance copy and
//!   the input for `scripts/gen_php_src_simplexml_cases.py`.
//! - Fixtures needing the managed libxml2 artifact skip through
//!   `skip_without_xml_native`; CI sets `ELEPHC_TEST_REQUIRE_XML_NATIVE=1` to turn that
//!   skip into a provisioning failure.

use crate::support::*;
use regex::RegexBuilder;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Root of the committed php-src fixture assets.
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/php_src/simplexml");

/// Disambiguates temporary directories across parallel tests.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

#[path = "simplexml_php_src_cases.rs"]
mod cases;

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

/// Symlinks every fixture asset beside the generated `test.php` entry file.
fn link_assets(work: &Path) {
    for entry in fs::read_dir(CORPUS).expect("fixture corpus") {
        let entry = entry.expect("fixture entry");
        let name = entry.file_name();
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "phpt")
        {
            continue;
        }
        link_asset(&entry.path(), &work.join(name));
    }
}

/// Links one asset, materializing directories so fixtures cannot write through a dir symlink.
fn link_asset(source: &Path, target: &Path) {
    if source.is_dir() {
        fs::create_dir(target).expect("fixture directory");
        for entry in fs::read_dir(source).expect("fixture directory") {
            let entry = entry.expect("fixture entry");
            link_asset(&entry.path(), &target.join(entry.file_name()));
        }
    } else {
        std::os::unix::fs::symlink(source, target).expect("asset symlink");
    }
}

/// Compiles one inline fixture in `work` and returns its merged stdout/stderr.
fn compile_and_capture(source: &str, work: &Path) -> (bool, String) {
    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, work, 128 * 1024 * 1024, false, false);
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
    let mut command = Command::new(&binary);
    command.current_dir(work);
    let output = run_command_with_merged_output(command, &work.join("output.log"));
    (output.status.success(), String::from_utf8(output.stdout)
        .expect("fixture UTF-8 output").replace("\r\n", "\n").trim().to_string())
}

/// Requires normal termination for every ordinary EXPECT/EXPECTF fixture.
pub(super) fn run_case(source: &str) -> String {
    run_case_with_exit(source, false)
}

/// Requires unsuccessful termination for a fixture whose expectation declares a fatal.
pub(super) fn run_fatal_case(source: &str) -> String {
    run_case_with_exit(source, true)
}

/// Releases all fixture artifacts even when compilation or execution panics.
struct FixtureDirectory(std::path::PathBuf);

impl Drop for FixtureDirectory {
    /// Removes only the unique directory created for this fixture.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs an isolated fixture with an explicit termination contract.
fn run_case_with_exit(source: &str, fatal: bool) -> String {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let work = loop {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos()).unwrap_or(0);
        let candidate = std::env::temp_dir().join(format!(
            "elephc_simplexml_phpt_{}_{:?}_{id}_{nanos}",
            std::process::id(), std::thread::current().id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => break FixtureDirectory(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temp fixture directory: {error}"),
        }
    };
    link_assets(&work.0);
    fs::write(work.0.join("test.php"), source).expect("fixture source");
    let (success, actual) = compile_and_capture(source, &work.0);
    assert_case_exit(success, fatal, &actual);
    actual
}

/// Checks process status independently of text that a program happened to print.
fn assert_case_exit(success: bool, fatal: bool, actual: &str) {
    assert_eq!(success, !fatal, "unexpected fixture termination (fatal={fatal}):\n{actual}");
}

/// Rewrites `var_dump()` object handles to the opaque `#N` placeholder.
///
/// Handles are allocation order, not observable program state, so comparisons replace them.
/// The match is scoped to `object(...)#N` because a bare `#<digits>` also appears in PHP
/// argument numbers (`Argument #1`) and XML character references (`&#38;`), which must stay
/// literal for the assertion to mean anything.
fn normalize_handles(text: &str) -> String {
    regex::Regex::new(r"object\(([^)\r\n]*)\)#\d+")
        .expect("object handle regex")
        .replace_all(text, "object($1)#N")
        .into_owned()
}

/// Asserts an exact php-src `EXPECT` while treating object handles as opaque identities.
///
/// Both sides normalize object handles to the same placeholder, so the comparison checks the
/// dumped shape and values without requiring php-src's allocator history (which can also
/// reuse a handle for different wrappers).
pub(super) fn assert_exact(actual: &str, expected: &str) {
    assert_eq!(normalize_handles(actual), normalize_handles(expected));
}

/// Asserts a fatal fixture's message line.
///
/// PHP's uncaught trace lists interpreter frames; Elephc's native frames are a different
/// implementation detail. The regression this corpus owns is the fatal message and its
/// source location, so compare that line against the expected `EXPECTF` prefix.
pub(super) fn assert_fatal_format(actual: &str, expected: &str) {
    let expected_line = expected.trim().lines().next().unwrap_or_default();
    let actual_line = actual.lines().next().unwrap_or_default();
    let pattern = format!("^{}$", expectf_to_regex(expected_line));
    let regex = RegexBuilder::new(&pattern)
        .build()
        .unwrap_or_else(|error| panic!("invalid fatal EXPECTF regex {pattern}: {error}"));
    assert!(
        regex.is_match(actual_line),
        "--- pattern ---\n{pattern}\n--- expected ---\n{expected_line}\n--- actual first line ---\n{actual_line}"
    );
}

/// Asserts one php-src `EXPECTF` pattern against a fixture's merged output.
pub(super) fn assert_format(actual: &str, expected: &str) {
    // php-src's EXPECTF usually writes `object(Foo)#%d`, but a few fixtures embed one concrete
    // handle. Rewrite only `object(...)#N` so `Argument #1` and `&#38;` keep their literal value.
    let expected = regex::Regex::new(r"object\(([^)\r\n]*)\)#\d+")
        .expect("object handle regex")
        .replace_all(expected.trim(), "object($1)#%d")
        .into_owned();
    let pattern = format!("^{}$", expectf_to_regex(&expected));
    let regex = RegexBuilder::new(&pattern)
        .dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTF regex {pattern}: {error}"));
    assert!(
        regex.is_match(actual),
        "--- pattern ---\n{pattern}\n--- expected ---\n{}\n--- actual ---\n{actual}",
        expected.trim()
    );
}

/// Verifies handle normalization cannot swallow non-handle `#<digits>` text.
#[test]
fn test_handle_normalization_keeps_argument_numbers_and_entities_literal() {
    assert_eq!(
        normalize_handles("object(SimpleXMLElement)#12 (0) {\n}"),
        "object(SimpleXMLElement)#N (0) {\n}"
    );
    assert_eq!(
        normalize_handles("Argument #1 ($qualifiedName) and a &#38; b"),
        "Argument #1 ($qualifiedName) and a &#38; b"
    );
}

/// Output equality must not hide an unsuccessful ordinary fixture (including a crash).
#[test]
#[should_panic(expected = "unexpected fixture termination")]
fn test_simplexml_followup_corpus_rejects_failed_normal_exit() {
    run_case("<?php echo 'expected output'; exit(7);");
}

/// Fatal-looking user output does not change the declared successful-exit contract.
#[test]
fn test_simplexml_followup_corpus_exit_is_independent_of_output() {
    assert_eq!(run_case("<?php echo 'Fatal error: ordinary text';"),
        "Fatal error: ordinary text");
}

/// Fatal fixtures cannot pass after printing an expected message and exiting successfully.
#[test]
#[should_panic(expected = "unexpected fixture termination")]
fn test_simplexml_followup_corpus_rejects_successful_fatal_exit() {
    run_fatal_case("<?php echo 'Fatal error: expected exception';");
}

/// Merged capture retains write order and the unsuccessful child status without compiling PHP.
#[test]
fn test_simplexml_followup_corpus_merged_capture_keeps_status_and_order() {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let path = std::env::temp_dir().join(format!("elephc_simplexml_capture_{}_{id}_{nanos}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).expect("capture directory");
    let work = FixtureDirectory(path);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf first; printf second >&2; exit 7"]);
    let output = run_command_with_merged_output(command, &work.0.join("output.log"));
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"firstsecond");
}
