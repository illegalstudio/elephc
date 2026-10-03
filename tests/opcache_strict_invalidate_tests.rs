//! Purpose:
//! End-to-end tests for the opt-in `--strict-opcache` flag, which makes D5 — the ONE documented
//! divergence of elephc's OPcache model — throw instead of passing silently.
//!
//! WHAT D5 IS. Reference PHP's `opcache_invalidate($file, true)` discards the cached script so
//! the next include re-reads and re-compiles it from disk. Code elephc compiled into the binary
//! is frozen at link time and can never be re-read. Reporting `true` is right for a program that
//! merely inspects the cache, and silently wrong for one that invalidates in order to pick up
//! CHANGED CODE — a dev-mode cache-buster, a plugin reloader. Such a program keeps running the
//! old code with no signal at all. `--strict-opcache` turns that one case into a throw.
//!
//! WHY THE THROW IS NARROW, and why that is the whole design:
//!
//! | `$force` | path in manifest | reference PHP | elephc default | `--strict-opcache` |
//! |----------|------------------|---------------|----------------|--------------------|
//! | `false`  | yes              | `true`        | `true`         | `true`             |
//! | `true`   | NO               | `true`        | `true`         | `true`             |
//! | `true`   | yes              | `true`        | `true`         | THROWS             |
//!
//! Only the last row is impossible to honor. Without `$force` reference PHP discards nothing
//! either, so there is nothing elephc fails to do; a non-manifest path is a file this binary
//! never compiled, so invalidating it is a no-op in reference PHP too. Narrowing the throw to the
//! single un-honorable request is what keeps the flag from being a blunt "reject opcache" switch.
//!
//! Called from:
//! - `cargo test --test opcache_strict_invalidate_tests` through Rust's test harness.
//!
//! Key details:
//! - THE DEFAULT MUST NOT MOVE. Every assertion here is paired: the same probe is compiled with
//!   and without the flag, and the no-flag run must match reference PHP 8.5.6 byte for byte. A
//!   flag that changed behaviour when absent would be a regression dressed as a feature.
//! - Expected values captured from `php -d xdebug.mode=off -d opcache.enable=1
//!   -d opcache.enable_cli=1`, which reports `A:true B:true C:true` for the three shapes.
//! - The thrown message is asserted through `getMessage()` in a `catch`, not from the fatal-error
//!   line. The uncaught handler does print the message now, but its output also carries the
//!   script's absolute path in the ` in <file>:<line>` suffix, and these tests compile into a temp
//!   directory whose name changes on every run — so a stderr assertion would be pinning
//!   `uncaught_exception_report_tests`' feature through a moving string instead of this one.
//! - `opcache.enable=1` AND `opcache.enable_cli=1` are both required: on CLI the cache is enabled
//!   only when both are set, and a disabled cache makes `opcache_invalidate()` return `false`
//!   before reaching any manifest logic — which would make these tests pass vacuously.
//! - Host-target only; this is a prelude-template selection with no emitted assembly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The three-shape probe: no-force manifest member, forced NON-member, forced manifest member.
const PROBE: &str = r#"<?php
echo "A:", var_export(opcache_invalidate(__FILE__, false), true), "\n";
echo "B:", var_export(opcache_invalidate('/etc/hosts', true), true), "\n";
echo "C:", var_export(opcache_invalidate(__FILE__, true), true), "\n";
echo "after\n";
"#;

/// Reference PHP 8.5.6 stdout for [`PROBE`] with the cache enabled, and elephc's no-flag output.
const REFERENCE_STDOUT: &str = "A:true\nB:true\nC:true\nafter\n";

/// The INI pair that enables the cache on CLI; both keys are required.
const ENABLED_INI: &[&str] = &["opcache.enable=1", "opcache.enable_cli=1"];

/// Creates an isolated temp dir unique across parallel test threads/processes, CANONICALIZED so
/// the compiled manifest path matches what `realpath()` returns at runtime.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
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

/// Compiles `source` in `dir` with the enabled-cache INI pair and any `extra` flags.
fn compile(dir: &Path, stem: &str, source: &str, extra: &[&str]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(extra);
    for assignment in ENABLED_INI {
        cmd.arg("--ini").arg(assignment);
    }
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed with {extra:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns `(stdout, success)`.
fn run_binary(bin: &Path) -> (String, bool) {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.success(),
    )
}

/// Without the flag, all three shapes return `true` exactly as reference PHP 8.5.6 does.
///
/// This is the load-bearing test of the pair: `--strict-opcache` is opt-in, so its ABSENCE must
/// leave the model byte-identical to reference PHP. A regression here would mean the flag changed
/// the default, which is the one thing it must never do.
#[test]
fn default_matches_reference_php_for_all_three_shapes() {
    let dir = make_test_dir("opcache_strict_default");
    let bin = compile(&dir, "app", PROBE, &[]);
    let (stdout, ok) = run_binary(&bin);

    assert!(ok, "default build must exit cleanly, got:\n{stdout}");
    assert_eq!(
        stdout, REFERENCE_STDOUT,
        "the no-flag default must match reference PHP exactly"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// With the flag, a FORCED invalidate of a manifest member throws and the program stops.
///
/// The two lines before the throw are asserted as well: they prove the throw happens at shape C
/// specifically, not at the first `opcache_invalidate()` call — a flag that threw on any
/// invalidate would produce empty stdout and pass a weaker assertion.
#[test]
fn strict_flag_throws_only_on_the_forced_manifest_member() {
    let dir = make_test_dir("opcache_strict_throws");
    let bin = compile(&dir, "app", PROBE, &["--strict-opcache"]);
    let (stdout, ok) = run_binary(&bin);

    assert!(!ok, "the strict build must fail on the uncaught throw");
    // The trailing `C:` is not slop: `echo "C:", var_export(...)` emits its arguments LEFT TO
    // RIGHT, so the label reaches stdout before the throw inside the second argument. Its
    // presence is what proves the throw happened at shape C and not earlier.
    //
    // A PREFIX, not an equality: the uncaught report follows PHP onto stdout, so it lands right
    // after `C:` on the same stream. What this test pins is where execution stopped, and the
    // report's own wording is asserted by the test below.
    assert!(
        stdout.starts_with("A:true\nB:true\nC:"),
        "shapes A and B must still return true; only the forced manifest member throws; got: {stdout:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The thrown `RuntimeException` carries a message naming the file and why it cannot be honored.
///
/// Asserted through `getMessage()` in a `catch`, because elephc's uncaught handler prints no
/// message for ANY exception (pre-existing, unrelated to this flag).
#[test]
fn strict_throw_is_a_catchable_runtime_exception_with_an_explanatory_message() {
    let source = r#"<?php
try {
    opcache_invalidate(__FILE__, true);
    echo "no-throw\n";
} catch (RuntimeException $e) {
    echo "caught\n";
    echo (strpos($e->getMessage(), '--strict-opcache') !== false ? "names-flag\n" : "BAD\n");
    echo (strpos($e->getMessage(), 'compiled into this binary') !== false ? "explains\n" : "BAD\n");
    echo (strpos($e->getMessage(), 'app.php') !== false ? "names-file\n" : "BAD\n");
}
"#;
    let dir = make_test_dir("opcache_strict_message");
    let bin = compile(&dir, "app", source, &["--strict-opcache"]);
    let (stdout, ok) = run_binary(&bin);

    assert!(ok, "a caught throw must let the program finish, got:\n{stdout}");
    assert_eq!(stdout, "caught\nnames-flag\nexplains\nnames-file\n");
    let _ = fs::remove_dir_all(&dir);
}

/// A DISABLED cache still short-circuits to `false` under the flag, never throwing.
///
/// php-src runs the enabled guard before anything else, so a disabled cache returns `false` with
/// no invalidation attempted — there is nothing elephc fails to honor, hence nothing to throw
/// about. Without this test the flag could start throwing in a configuration where reference PHP
/// quietly returns `false`, which would be an over-rejection.
#[test]
fn strict_flag_does_not_throw_when_the_cache_is_disabled() {
    let source = r#"<?php
echo var_export(opcache_invalidate(__FILE__, true), true), "\n";
"#;
    let dir = make_test_dir("opcache_strict_disabled");
    let php = dir.join("app.php");
    fs::write(&php, source).unwrap();
    let output = Command::new(elephc_bin()).arg("build")
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--strict-opcache")
        .arg("--ini")
        .arg("opcache.enable=0")
        .arg(&php)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (stdout, ok) = run_binary(&dir.join("app"));

    assert!(ok, "a disabled cache must not throw, got:\n{stdout}");
    assert_eq!(stdout, "false\n", "disabled cache returns false, as reference PHP does");
    let _ = fs::remove_dir_all(&dir);
}
