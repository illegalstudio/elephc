//! Purpose:
//! Compiles and runs the Rust port of the PHP 8.5.13 `ext/zip/tests` corpus.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - The cases live in the generated sibling module `zip_php_src_cases.rs`; each
//!   test carries its PHP source and assertion inline, matching the ordinary Elephc
//!   `compile_and_run` fixture style.
//! - No `.phpt` file is committed or parsed at run time. The generator downloads the
//!   tagged upstream corpus; `tests/php_src/zip/` holds only the binary/text assets
//!   the cases reference plus the `prelude.php` transcription oracle.
//! - Every fixture gets an isolated temp directory with those assets symlinked inside
//!   a `tests/` child, so `__DIR__` resolves as under php-src and fixtures that assert
//!   on `basename(__DIR__)` see `tests`.
//! - stdout and stderr share one OS file so diagnostic order is preserved.

use crate::support::*;
use regex::RegexBuilder;
use std::fs;
use std::path::Path;
use std::os::unix::fs::DirBuilderExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Root of the committed php-src fixture assets used by the Rust port.
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/php_src/zip");

/// Disambiguates temporary directories across parallel tests.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

#[allow(non_snake_case)]
#[path = "zip_php_src_cases.rs"]
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

/// Symlinks every fixture asset beside the generated `test.php` entry file.
fn link_assets(work: &Path) {
    for entry in fs::read_dir(CORPUS).expect("fixture corpus") {
        let entry = entry.expect("fixture entry");
        let name = entry.file_name();
        if name == "README.md" || name == "LICENSE" || name == "VERSION" || name == "prelude.php" {
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

/// Runs one inline php-src zip fixture and returns its merged output.
///
/// `ini` mirrors the fixture's `--INI--` compile-time overrides; `env` mirrors `--ENV--`
/// at program start. Assets under `tests/php_src/zip/` are available through
/// `__DIR__`.
pub(super) fn run_case(source: &str, ini: &[(&str, &str)], env: &[(&str, &str)]) -> String {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    // O_EXCL + 0700 avoids the predictable-name symlink race on shared hosts.
    let root = loop {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0);
        let candidate = std::env::temp_dir().join(format!(
            "elephc_phpt_{}_{:?}_{id}_{nanos}", std::process::id(), std::thread::current().id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temp fixture directory: {error}"),
        }
    };
    // Run from a directory literally named `tests`: php-src fixtures such as
    // `bug78641.phpt` assert on `basename(__DIR__)`.
    let work = root.join("tests");
    fs::create_dir(&work).expect("fixture tests directory");
    link_assets(&work);
    fs::write(work.join("test.php"), source).expect("fixture source");
    let overrides = ini.iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    elephc::codegen::set_ini_overrides(overrides);
    let _restore = IniRestore;

    let (success, actual) = compile_and_capture(source, &work, env);
    let _ = fs::remove_dir_all(&root);
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
