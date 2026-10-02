//! Purpose:
//! End-to-end tests for hosted PHP extensions: `elephc extension add` builds the
//! `elephc_demo` fixture extension from source, and compiled programs call it.
//!
//! Called from:
//! - `cargo test --test php_ext_tests -- --ignored`.
//!
//! Key details:
//! - Ignored by default: the first run downloads the pinned php-src tarball
//!   (24 MB) and compiles the Zend engine archive into the native cache
//!   (`ELEPHC_NATIVE_CACHE`, or `~/.cache/elephc/native`). Later runs reuse it.
//! - `demo_program.expected` is the output of real PHP running the same
//!   program with the fixture loaded (`php -d extension=elephc_demo.so`), so a
//!   hosted call that differs from PHP fails here, not just one that crashes.
//! - Each test works in its own temporary project, so the manifest and lock it
//!   writes never touch the repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};



fn elephc() -> &'static str {
    env!("CARGO_BIN_EXE_elephc")
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/php_ext")
}

/// A fresh project directory with the fixture extension added as a path source.
fn project(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("elephc-extension-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let output = Command::new(elephc())
        .current_dir(&dir)
        .args(["extension", "add", "elephc_demo", "--path"])
        .arg(fixture_dir().join("demo"))
        .output()
        .expect("run elephc extension add");
    assert_success(&output, "extension add");
    dir
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed ({:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Compiles `source` as `main.php` in `dir` and runs the binary.
fn compile_and_run(dir: &Path, source: &str) -> Output {
    let main = dir.join("main.php");
    fs::write(&main, source).unwrap();
    let compile = Command::new(elephc()).current_dir(dir).arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile");
    Command::new(dir.join("main")).current_dir(dir).output().expect("run the compiled program")
}

/// Every value path of the fixture matches real PHP, line for line.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn hosted_calls_match_real_php() {

    let dir = project("oracle");
    let program = fs::read_to_string(fixture_dir().join("demo_program.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("demo_program.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    fs::remove_dir_all(dir).unwrap();
}

/// `add` reports the surface and names the function it cannot call yet;
/// `list` shows the extension as built.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn add_and_list_report_the_surface() {
    let dir = project("list");
    let list = Command::new(elephc()).current_dir(&dir).args(["extension", "list"]).output().unwrap();
    assert_success(&list, "extension list");
    let text = String::from_utf8_lossy(&list.stdout);
    assert!(text.contains("elephc_demo 1.2.3 — 19 functions, 2 classes, 3 constants"), "{text}");
    assert!(text.contains("not callable yet: demo_apply()"), "{text}");
    let manifest = fs::read_to_string(dir.join("elephc.toml")).unwrap();
    assert!(manifest.contains("php-src = \"8.5.6\""), "{manifest}");
    assert!(manifest.contains("elephc_demo = { path ="), "{manifest}");
    fs::remove_dir_all(dir).unwrap();
}

/// `[extension.ini]` is applied before the extension starts, as php.ini is.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn ini_directives_reach_the_extension() {
    let dir = project("ini");
    let mut manifest = fs::read_to_string(dir.join("elephc.toml")).unwrap();
    manifest.push_str("\n[extension.ini]\n\"elephc_demo.greeting\" = \"Bonjour\"\n");
    fs::write(dir.join("elephc.toml"), manifest).unwrap();
    let run = compile_and_run(&dir, "<?php echo demo_greet('Ada'), \"\\n\";");
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Bonjour, Ada.\n");
    fs::remove_dir_all(dir).unwrap();
}

/// A warning is reported and the call returns; an E_ERROR ends the program
/// with PHP's exit status; a function taking a callable explains itself.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn diagnostics_follow_php() {
    let dir = project("diagnostics");
    let run = compile_and_run(
        &dir,
        r#"<?php
var_dump(demo_warn("fire"));
try {
    demo_apply(fn($x) => $x, [1]);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
echo "before\n";
demo_fatal();
echo "after\n";
"#,
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(255), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.starts_with("bool(true)\n"), "{stdout}");
    assert!(stdout.contains("demo_apply() cannot be called from Elephc yet"), "{stdout}");
    assert!(stdout.contains("before\n") && !stdout.contains("after"), "{stdout}");
    assert!(stderr.contains("Warning: demo_warn(): careful with fire"), "{stderr}");
    assert!(stderr.contains("Fatal error: demo_fatal(): the fixture gave up"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// Many calls returning strings, arrays, objects and arrays holding objects
/// leave the heap flat: the engine copies are freed with each call and Elephc
/// owns one copy of each result.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn repeated_calls_do_not_grow_the_heap() {
    let dir = project("heap");
    let main = dir.join("main.php");
    fs::write(
        &main,
        r#"<?php
$total = 0;
for ($i = 0; $i < 300; $i++) {
    $total += strlen(demo_greet("Ada"));
    $total += count(demo_split("a,b,c"));
    $total += demo_object("xy")->length;
    $total += count(demo_echo(["k" => [1, "two", 3.5]]));
    $total += count(demo_nested());
}
echo $total, "\n";
"#,
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).args(["--heap-debug"]).arg(&main).output().unwrap();
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main")).output().unwrap();
    assert_success(&run, "compiled program");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// A result Elephc cannot represent (an object of a class other than
/// stdClass, a value containing itself) is refused with a catchable `Error`
/// before any of it is rebuilt.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn unrepresentable_results_are_refused() {
    let dir = project("refused");
    let run = compile_and_run(
        &dir,
        r#"<?php
try {
    demo_error_object();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    demo_cycle();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
echo "done\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a hosted PHP extension returned a value of type DemoParseException, which Elephc cannot represent yet\n\
         a hosted PHP extension returned a recursive object, which Elephc cannot represent yet\n\
         done\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Every error path frees the call it prepared: an extension exception after
/// a by-reference write, a refused result, a refused argument, a named
/// argument refused before the call. The host counts calls not yet freed.
///
/// Elephc's own heap is not asserted here: a function left by `throw` does
/// not release its heap locals on the current backend (a pure-PHP
/// `function f(string $s) { throw new Error("x"); }` leaks `$s`), so every
/// exception costs a few blocks whatever the bridge does.
#[test]
#[ignore = "downloads php-src and builds a C extension"]
fn error_paths_free_the_call() {
    let dir = project("error-paths");
    let run = compile_and_run(
        &dir,
        r#"<?php
extern function elephc_php_ext_live_calls(): int;
$caught = 0;
for ($i = 0; $i < 50; $i++) {
    $left = 1;
    try {
        demo_consume(5, $left);
    } catch (UnderflowException $e) {
        $caught += 1 + $left;
    }
    try {
        demo_error_object();
    } catch (Error $e) {
        $caught++;
    }
    try {
        demo_cycle();
    } catch (Error $e) {
        $caught++;
    }
    try {
        demo_parse("x");
    } catch (DemoException $e) {
        $caught++;
    }
    try {
        demo_echo(new stdClass());
    } catch (TypeError $e) {
        $caught++;
    }
}
echo $caught, " ", elephc_php_ext_live_calls(), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "250 0\n");
    fs::remove_dir_all(dir).unwrap();
}
