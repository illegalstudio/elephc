//! Purpose:
//! End-to-end tests for `register_shutdown_function()` in a PLAIN (non-`--web`) binary: that
//! the queue exists at all, that it is drained on the normal end of the script AND on `exit()`,
//! that the process exit status survives, that callbacks run in registration order, and that a
//! program which never names the function emits a BYTE-IDENTICAL exit sequence.
//!
//! Called from:
//! - `cargo test --test shutdown_function_tests` through Rust's test harness.
//!
//! Key details:
//! - `exit()` IS the interesting path, and it is the reason these are not folded into
//!   `error_handling_surface_tests`. php runs shutdown functions on `exit()` and does NOT run
//!   `finally` blocks there, so the `--web` mechanism — a `try`/`finally` around the request
//!   body — covers the normal end and silently misses the path a console application takes
//!   (`Symfony\Component\Console\Application::run()` ends in `exit($exitCode)`). The drain is
//!   therefore emitted by CODEGEN at `lower_exit` and `emit_main_epilogue`, and
//!   `shutdown_functions_run_on_exit_and_the_status_survives` is the test that fails if a
//!   future change moves it back into a `finally`.
//! - Every expected string and status below was captured from `php 8.5.10` on the SAME source,
//!   one `php <fixture>` per row; the fixtures live in this file so the two can be re-run
//!   against each other. Where elephc diverges it is called out at the test, not papered over.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable and run it, asserting stdout AND the exit status. Host-target
//!   only, same harness style as `error_handling_surface_tests`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut exe = std::env::current_exe().expect("failed to resolve current test binary");
        exe.pop();
        if exe.ends_with("deps") {
            exe.pop();
        }
        exe.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` to a plain (non-`--web`) executable and returns its path.
fn compile_cli(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .arg(&php)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles and runs `source`, returning `(stdout, exit status)`.
///
/// The status is returned rather than asserted, because half of these fixtures exit non-zero on
/// purpose: `exit(3)` losing its status would otherwise pass a test about `exit(3)`.
fn compile_and_run(prefix: &str, source: &str) -> (String, i32) {
    let dir = make_test_dir(prefix);
    let bin = compile_cli(&dir, source, "app");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let code = output.status.code().unwrap_or(-1);
    (stdout, code)
}

/// The function must EXIST off `--web`. It did not: a plain build answered `Call to undefined
/// function register_shutdown_function()` at run time, having compiled cleanly.
///
/// `php -r 'var_dump(function_exists("register_shutdown_function"));'` prints `bool(true)`.
#[test]
fn register_shutdown_function_exists_in_a_plain_cli_build() {
    let (stdout, code) = compile_and_run(
        "shutdown_exists",
        "<?php echo function_exists('register_shutdown_function') ? \"yes\\n\" : \"no\\n\";\n",
    );
    assert_eq!(stdout, "yes\n");
    assert_eq!(code, 0);
}

/// THE NORMAL END. php drains the queue after the last top-level statement.
///
/// php 8.5.10 on this fixture prints `body` then `shutdown`, status 0.
#[test]
fn shutdown_functions_run_at_the_normal_end_of_the_script() {
    let (stdout, code) = compile_and_run(
        "shutdown_normal",
        r#"<?php
register_shutdown_function(function () { echo "shutdown\n"; });
echo "body\n";
"#,
    );
    assert_eq!(stdout, "body\nshutdown\n", "got {stdout:?}");
    assert_eq!(code, 0);
}

/// THE TEST THIS SUITE EXISTS FOR. `exit()` runs shutdown functions and does NOT run `finally`
/// blocks, so a drain implemented as a `try`/`finally` wrapper passes every other test here and
/// fails this one. Verified on php 8.5.10:
///
/// ```text
/// php -r 'register_shutdown_function(function(){ echo "SHUTDOWN\n"; });
///         try { echo "body\n"; exit(3); } finally { echo "FINALLY\n"; }'
/// body
/// SHUTDOWN
/// rc=3
/// ```
///
/// `FINALLY` never prints. The status must survive too: `exit(3)` is how a console application
/// reports a failing command, and a drain that terminated with 0 would turn every failure green.
#[test]
fn shutdown_functions_run_on_exit_and_the_status_survives() {
    let (stdout, code) = compile_and_run(
        "shutdown_exit3",
        r#"<?php
register_shutdown_function(function () { echo "shutdown\n"; });
echo "body\n";
exit(3);
"#,
    );
    assert_eq!(stdout, "body\nshutdown\n", "got {stdout:?}");
    assert_eq!(code, 3, "exit(3) must still report 3");
}

/// `exit(0)` is a separate lowering arm from `exit($n)` in elephc — a literal zero takes the
/// no-operand path — so it gets its own row rather than being assumed.
#[test]
fn shutdown_functions_run_on_exit_zero() {
    let (stdout, code) = compile_and_run(
        "shutdown_exit0",
        r#"<?php
register_shutdown_function(function () { echo "shutdown\n"; });
echo "body\n";
exit(0);
"#,
    );
    assert_eq!(stdout, "body\nshutdown\n", "got {stdout:?}");
    assert_eq!(code, 0);
}

/// The shape a console application actually has: the `exit()` is inside a FUNCTION, several
/// frames below the top level, not in the entry script's own statement list. A drain bolted
/// onto the top-level body would miss it; one emitted at the exit site does not.
#[test]
fn an_exit_inside_a_function_still_drains() {
    let (stdout, code) = compile_and_run(
        "shutdown_exit_in_fn",
        r#"<?php
function run(): void { echo "run\n"; exit(4); }
register_shutdown_function(function () { echo "shutdown\n"; });
run();
"#,
    );
    assert_eq!(stdout, "run\nshutdown\n", "got {stdout:?}");
    assert_eq!(code, 4);
}

/// REGISTRATION ORDER, which php guarantees and a stack would reverse.
#[test]
fn shutdown_functions_run_in_registration_order() {
    let (stdout, code) = compile_and_run(
        "shutdown_order",
        r#"<?php
register_shutdown_function(function () { echo "first\n"; });
register_shutdown_function(function () { echo "second\n"; });
echo "body\n";
"#,
    );
    assert_eq!(stdout, "body\nfirst\nsecond\n", "got {stdout:?}");
    assert_eq!(code, 0);
}

/// A callback registered from INSIDE another still runs — php appends it to the same queue and
/// the drain reaches it. Measured: `outer` then `inner`.
#[test]
fn a_shutdown_function_registered_from_inside_another_still_runs() {
    let (stdout, code) = compile_and_run(
        "shutdown_nested",
        r#"<?php
register_shutdown_function(function () {
    echo "outer\n";
    register_shutdown_function(function () { echo "inner\n"; });
});
echo "body\n";
"#,
    );
    assert_eq!(stdout, "body\nouter\ninner\n", "got {stdout:?}");
    assert_eq!(code, 0);
}

/// An ARRAY CALLABLE with a bound argument, which is the form frameworks use —
/// `Symfony\Component\ErrorHandler\ErrorHandler::register()` passes
/// `self::handleFatalError(...)`, a first-class callable over a static method.
#[test]
fn an_array_callable_is_a_shutdown_function() {
    let (stdout, code) = compile_and_run(
        "shutdown_array_callable",
        r#"<?php
class Recorder
{
    public function note(string $tag): void { echo "method:{$tag}\n"; }
}
$recorder = new Recorder();
register_shutdown_function([$recorder, 'note'], 'tag');
echo "body\n";
"#,
    );
    assert_eq!(stdout, "body\nmethod:tag\n", "got {stdout:?}");
    assert_eq!(code, 0);
}

/// A shutdown function may itself call `exit()`, and php then terminates AT ONCE: the callbacks
/// still queued behind it do not run, and the callback's status wins over the one the body
/// asked for. Measured on php 8.5.10 — this fixture prints `body`, `one`, status 7; `two` never
/// appears and the body's `exit(3)` is overridden.
///
/// This is what `__elephc_shutdown_function_state`'s `$draining` latch is for: that inner
/// `exit()` re-enters the drain through `lower_exit`, and without the latch the loop would
/// resume and print `two`.
#[test]
fn an_exit_inside_a_shutdown_function_skips_the_rest() {
    let (stdout, code) = compile_and_run(
        "shutdown_exit_in_callback",
        r#"<?php
register_shutdown_function(function () { echo "one\n"; exit(7); });
register_shutdown_function(function () { echo "two\n"; });
echo "body\n";
exit(3);
"#,
    );
    assert_eq!(stdout, "body\none\n", "got {stdout:?}");
    assert_eq!(code, 7, "the callback's exit status wins");
}

/// PAY-FOR-USE, and this one is about the EXIT SEQUENCE rather than about declarations.
///
/// The drain is emitted by codegen at every `exit()` and at the top level's epilogue, so the
/// failure mode of getting the gate wrong is that EVERY program grows a call it can never need.
/// Compares the emitted assembly for a program that exits against the same program with one
/// `register_shutdown_function()` added: the first must be byte-identical to a build made before
/// the surface existed, which is asserted here as "carries none of the surface's symbols and is
/// strictly smaller".
#[test]
fn a_program_that_names_nothing_emits_no_shutdown_code() {
    let dir = make_test_dir("shutdown_payforuse");
    let emit_asm = |stem: &str, source: &str| -> String {
        let php = dir.join(format!("{}.php", stem));
        fs::write(&php, source).unwrap();
        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg("--emit-asm")
            .arg(&php)
            .output()
            .expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "elephc --emit-asm failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(dir.join(format!("{}.s", stem))).expect("emitted assembly")
    };

    // Both programs EXIT, so the comparison is about the exit sequence and not about whether an
    // exit is emitted at all.
    let bare = emit_asm("bare", "<?php echo 1; exit(0);\n");
    let using = emit_asm(
        "using",
        "<?php register_shutdown_function(function () { echo 2; }); echo 1; exit(0);\n",
    );

    for symbol in [
        "register_shutdown_function",
        "__elephc_shutdown_run",
        "__elephc_shutdown_function_state",
    ] {
        assert!(
            !bare.contains(symbol),
            "a program that names no shutdown function must not carry {symbol}"
        );
    }
    assert!(
        using.contains("__elephc_shutdown_run"),
        "the drain entry is forced for a program that does name one"
    );
    assert!(
        bare.len() < using.len(),
        "the bare program must be smaller: {} vs {}",
        bare.len(),
        using.len()
    );
}

/// A program that names a DIFFERENT function from the same prelude must not pay for the drain
/// either. `error_reporting()` pulls in the shared error-handling surface, and the shutdown
/// registry rides in that same `declarations()` list — so the only thing keeping it out is the
/// forced-group rule being scoped to `register_shutdown_function`, which this pins.
#[test]
fn naming_a_sibling_surface_does_not_pay_for_the_drain() {
    let dir = make_test_dir("shutdown_sibling");
    let php = dir.join("sibling.php");
    fs::write(&php, "<?php error_reporting(E_ALL); echo 1; exit(0);\n").unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--emit-asm")
        .arg(&php)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("sibling.s")).expect("emitted assembly");
    assert!(
        asm.contains("__elephc_diag_render"),
        "the error surface it DID name must be present"
    );
    assert!(
        !asm.contains("__elephc_shutdown_run"),
        "but the shutdown drain it did not name must be absent"
    );
}
