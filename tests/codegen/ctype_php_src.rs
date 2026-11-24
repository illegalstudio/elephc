//! Purpose:
//! Compiles and runs the PHP 8.5.10 `ext/ctype` compatibility cases as inline Rust fixtures.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - The cases live in the hand-maintained sibling module `ctype_php_src_cases.rs`; each
//!   test carries its PHP source and assertion inline, matching the ordinary Elephc
//!   `compile_and_run` fixture style.
//! - Every fixture gets an isolated temp directory with a `test.php` entry file.
//! - stdout and stderr share one OS file so deprecation and output order is preserved.
//! - `--INI-- error_reporting=...` is emulated with a runtime `error_reporting()` call
//!   inserted on the opening PHP line, because Elephc bakes the default mask into the
//!   runtime object rather than reading the directive at startup.
//! - The fixtures are derived from php-src 8.5.10 `ext/ctype/tests` (PHP License 3.01);
//!   each one is fully inline and the test process never reads an external fixture.

use crate::support::*;
use regex::RegexBuilder;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Disambiguates temporary directories across parallel tests.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

#[path = "ctype_php_src_cases.rs"]
mod cases;

/// Restores the compilation context after one fixture's `--INI--` overrides.
struct IniRestore;

impl Drop for IniRestore {
    /// Clears the thread-local overrides so the next fixture starts from defaults.
    fn drop(&mut self) {
        elephc::codegen::set_ini_overrides(Vec::new());
    }
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

/// Compiles one inline fixture in `work` and returns its merged stdout/stderr.
fn compile_and_capture(source: &str, work: &Path, env: &[(&str, &str)]) -> (bool, String) {
    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, work, 8_388_608, false, false);
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
    let mut command = Command::new(&binary);
    command.current_dir(work);
    for (key, value) in env {
        command.env(key, value);
    }
    let status = command
        .stdout(Stdio::from(log.try_clone().expect("log handle")))
        .stderr(Stdio::from(log))
        .status()
        .expect("run fixture");
    let output = fs::read_to_string(&log_path).unwrap_or_default();
    (status.success(), output.replace("\r\n", "\n").trim().to_string())
}

/// Applies the runtime-visible part of a fixture's `--INI--` section.
///
/// `error_reporting` is the only ctype directive that changes program output. Elephc bakes
/// the default error mask into its cached runtime object, so the fixture's requested mask is
/// installed by an inline call on the opening PHP line instead. Keeping the call on that line
/// preserves every later source line number that an `EXPECTF` `%d` would otherwise observe.
fn apply_runtime_ini(source: &str, ini: &[(&str, &str)]) -> String {
    let Some((_, value)) = ini.iter().find(|(key, _)| key.eq_ignore_ascii_case("error_reporting")) else {
        return source.to_string();
    };
    let mask = match value.replace(' ', "").as_str() {
        "E_ALL&~E_DEPRECATED" => "E_ALL & ~E_DEPRECATED",
        "E_ALL" => "E_ALL",
        other => panic!("unsupported ctype fixture error_reporting expression: {other}"),
    };
    source.replacen("<?php", &format!("<?php error_reporting({mask});"), 1)
}

/// Reports whether the host libc exposes one of the requested locale names.
///
/// The locale-inheritance case has a php-src `--SKIPIF--` requirement for a real
/// `de_DE` locale. Rust tests cannot use corpus skip blocks, so the fixture calls this
/// helper and returns early when the host lacks the locale. The probe reads `locale -a`
/// in a child process and never mutates the test process.
///
/// CI provides German locales on every executing target and sets
/// `ELEPHC_REQUIRE_TEST_LOCALES`; there a missing locale panics instead of letting the
/// caller skip, so locale-dependent behavior cannot go unverified behind a green run.
pub(crate) fn locale_available(names: &[&str]) -> bool {
    let available = Command::new("locale")
        .arg("-a")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            let listed = String::from_utf8_lossy(&output.stdout);
            names.iter().any(|name| listed.lines().any(|line| line == *name))
        });
    assert!(
        available || !test_locales_required(),
        "ELEPHC_REQUIRE_TEST_LOCALES is set but none of {names:?} is listed by `locale -a`"
    );
    available
}

/// Reports whether this run must execute locale-dependent tests instead of skipping them.
pub(crate) fn test_locales_required() -> bool {
    std::env::var_os("ELEPHC_REQUIRE_TEST_LOCALES").is_some_and(|value| value == "1")
}

/// Runs one inline php-src ctype fixture and returns its merged output.
///
/// `ini` mirrors the fixture's `--INI--` compile-time overrides; `env` mirrors `--ENV--`
/// at program start.
pub(super) fn run_case(source: &str, ini: &[(&str, &str)], env: &[(&str, &str)]) -> String {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    // O_EXCL + 0700 avoids the predictable-name symlink race on shared hosts.
    let work = loop {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0);
        let candidate = std::env::temp_dir().join(format!(
            "elephc_ctype_fixture_{}_{:?}_{id}_{nanos}", std::process::id(), std::thread::current().id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temp fixture directory: {error}"),
        }
    };
    let source = apply_runtime_ini(source, ini);
    fs::write(work.join("test.php"), &source).expect("fixture source");
    let overrides = ini.iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    elephc::codegen::set_ini_overrides(overrides);
    let _restore = IniRestore;

    let (success, actual) = compile_and_capture(&source, &work, env);
    let _ = fs::remove_dir_all(&work);
    assert!(success, "compiled fixture exited with failure\n{actual}");
    actual
}

/// Asserts one php-src `EXPECTF` pattern against a fixture's merged output.
pub(super) fn assert_format(actual: &str, expected: &str) {
    let pattern = format!("^{}$", expectf_to_regex(expected.trim()));
    let regex = RegexBuilder::new(&pattern).dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTF regex {pattern}: {error}"));
    assert!(regex.is_match(actual), "--- pattern ---\n{pattern}\n--- expected ---\n{}\n--- actual ---\n{actual}",
        expected.trim());
}
