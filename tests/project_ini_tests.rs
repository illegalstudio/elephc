//! Purpose:
//! End-to-end tests for the `[ini]` table of a project's `elephc.toml`: the directives a
//! project sets once instead of passing `--ini` on every compile.
//!
//! Called from:
//! - `cargo test --test project_ini_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, compile a plain executable and run it.
//! - The directives read back are OPcache ones, whose `ini_get()` reports the raw INI string
//!   `--ini` supplied: exactly what an `[ini]` entry must become.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads and processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "{}_{}_{:?}_{}",
        prefix,
        std::process::id(),
        std::thread::current().id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Prints each listed directive's `ini_get()` value, one per line.
fn probe(keys: &[&str]) -> String {
    let lines: String = keys
        .iter()
        .map(|key| format!("echo '{key}=', var_export(ini_get('{key}'), true), \"\\n\";\n"))
        .collect();
    format!("<?php\n{lines}")
}

/// Compiles `source` (a path under `root`) with `extra` CLI arguments.
fn compile(root: &Path, source: &Path, extra: &[&str]) -> Output {
    Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", root.join("cache-root"))
        .current_dir(root)
        .args(extra)
        .arg(source)
        .output()
        .expect("failed to spawn elephc")
}

/// Compiles and runs `source`, returning its stdout.
fn compile_and_run(root: &Path, source: &Path, extra: &[&str]) -> String {
    let output = compile(root, source, extra);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let run = Command::new(source.with_extension(""))
        .output()
        .expect("failed to run binary");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// The manifest's `[ini]` entries reach `ini_get()` exactly as `--ini` would set them, from a
/// source in a SUB-directory of the project: a string verbatim, an integer in decimal, `true`
/// as `'1'`, `false` as `''`, and a bare dotted key flattened back to its directive name.
///
/// The `[ini.opcache]` sub-table spelling is pinned in `project_ini`'s unit tests: TOML refuses
/// it in the same file as a bare `opcache.…` key, which already defines that table.
#[test]
fn ini_entries_of_the_project_manifest_act_like_ini_flags() {
    let root = make_test_dir("project_ini_values");
    fs::write(
        root.join("elephc.toml"),
        "[ini]\n\
         \"opcache.memory_consumption\" = 256\n\
         opcache.enable_cli = true\n\
         \"opcache.validate_timestamps\" = false\n\
         \"opcache.lockfile_path\" = \"/var/tmp\"\n\
         opcache.max_accelerated_files = 4000\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    let source = root.join("app").join("main.php");
    fs::write(
        &source,
        probe(&[
            "opcache.memory_consumption",
            "opcache.enable_cli",
            "opcache.validate_timestamps",
            "opcache.lockfile_path",
            "opcache.max_accelerated_files",
        ]),
    )
    .unwrap();

    let from_manifest = compile_and_run(&root, &source, &[]);
    let from_flags = {
        fs::remove_file(root.join("elephc.toml")).unwrap();
        compile_and_run(
            &root,
            &source,
            &[
                "--ini",
                "opcache.memory_consumption=256",
                "--ini",
                "opcache.enable_cli=1",
                "--ini",
                "opcache.validate_timestamps=",
                "--ini",
                "opcache.lockfile_path=/var/tmp",
                "--ini",
                "opcache.max_accelerated_files=4000",
            ],
        )
    };

    assert_eq!(
        from_manifest,
        "opcache.memory_consumption='256'\n\
         opcache.enable_cli='1'\n\
         opcache.validate_timestamps=''\n\
         opcache.lockfile_path='/var/tmp'\n\
         opcache.max_accelerated_files='4000'\n"
    );
    assert_eq!(from_manifest, from_flags, "an [ini] entry is exactly one --ini flag");
}

/// An `--ini` on the command line wins over the manifest's value for the same directive.
#[test]
fn the_command_line_wins_over_the_manifest() {
    let root = make_test_dir("project_ini_precedence");
    fs::write(root.join("elephc.toml"), "[ini]\n\"opcache.memory_consumption\" = 256\n").unwrap();
    let source = root.join("main.php");
    fs::write(&source, probe(&["opcache.memory_consumption"])).unwrap();

    assert_eq!(
        compile_and_run(&root, &source, &["--ini", "opcache.memory_consumption=512"]),
        "opcache.memory_consumption='512'\n"
    );
}

/// A value with no INI spelling fails the compile with a message naming the manifest.
#[test]
fn a_value_with_no_ini_spelling_is_refused() {
    let root = make_test_dir("project_ini_refused");
    fs::write(root.join("elephc.toml"), "[ini]\n\"opcache.blacklist_filename\" = [\"a\"]\n").unwrap();
    let source = root.join("main.php");
    fs::write(&source, "<?php echo 1;\n").unwrap();

    let output = compile(&root, &source, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the compile must fail");
    assert!(stderr.contains("elephc.toml"), "{stderr}");
    assert!(stderr.contains("opcache.blacklist_filename"), "{stderr}");
}

/// The `[ini.opcache]` sub-table spelling reaches the compiled binary like the dotted one.
#[test]
fn a_sub_table_names_its_directives_with_the_table_prefix() {
    let root = make_test_dir("project_ini_sub_table");
    fs::write(root.join("elephc.toml"), "[ini.opcache]\nmax_accelerated_files = 4000\n").unwrap();
    let source = root.join("main.php");
    fs::write(&source, probe(&["opcache.max_accelerated_files"])).unwrap();

    assert_eq!(
        compile_and_run(&root, &source, &[]),
        "opcache.max_accelerated_files='4000'\n"
    );
}

/// A directive the file names twice, under two spellings TOML accepts together, fails the
/// compile: keeping both would let key order, not the file, pick the value.
#[test]
fn a_directive_named_twice_is_refused() {
    let root = make_test_dir("project_ini_twice");
    fs::write(
        root.join("elephc.toml"),
        "[ini]\n\"opcache.memory_consumption\" = 256\n\n[ini.opcache]\nmemory_consumption = 64\n",
    )
    .unwrap();
    let source = root.join("main.php");
    fs::write(&source, "<?php echo 1;\n").unwrap();

    let output = compile(&root, &source, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the compile must fail");
    assert!(stderr.contains("opcache.memory_consumption twice"), "{stderr}");
}

/// A manifest that is not valid TOML fails the compile with a message naming the file, even
/// for a program that needs no native package.
#[test]
fn a_malformed_manifest_fails_the_compile() {
    let root = make_test_dir("project_ini_malformed");
    fs::write(root.join("elephc.toml"), "[native]\nschema = 1\n[native.dependencies]\npcre2 = \"10.47\n").unwrap();
    let source = root.join("main.php");
    fs::write(&source, "<?php echo 1;\n").unwrap();

    let output = compile(&root, &source, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the compile must fail");
    assert!(stderr.contains("elephc.toml") && stderr.contains("invalid TOML"), "{stderr}");
}

/// A manifest holding only `[ini]` (no `[native]`) is a valid project file, and a manifest
/// holding only `[native]` sets no directive.
#[test]
fn ini_and_native_sections_are_each_optional() {
    let root = make_test_dir("project_ini_sections");
    let source = root.join("main.php");
    fs::write(&source, probe(&["opcache.memory_consumption"])).unwrap();

    fs::write(root.join("elephc.toml"), "[native]\nschema = 1\n\n[native.dependencies]\n").unwrap();
    assert_eq!(
        compile_and_run(&root, &source, &[]),
        "opcache.memory_consumption='128'\n",
        "the compiled-in default"
    );

    fs::write(root.join("elephc.toml"), "[ini]\n\"opcache.memory_consumption\" = 64\n").unwrap();
    assert_eq!(compile_and_run(&root, &source, &[]), "opcache.memory_consumption='64'\n");
}
