//! Purpose:
//! Regression tests for storing a runtime-helper string into a STATIC property. Such a
//! string may be a pointer into the shared concat scratch buffer rather than owned heap
//! storage, and the store used to move that pointer in instead of persisting it.
//!
//! Called from:
//! - `cargo test --test static_property_string_ownership_tests` through Rust's test harness.
//!
//! Key details:
//! - Every probe writes the property, then runs an UNRELATED `str_repeat()` whose only job
//!   is to overwrite the scratch buffer, then reads the property back. Without the fix the
//!   read returned the repeat's bytes at the right LENGTH — silently, with no warning and
//!   no crash — so a test that only checked `strlen()` would have passed against the defect.
//! - The whole `RuntimeCallTarget::UnaryString` family is covered rather than one member:
//!   all twelve measured behaved identically, so pinning one would leave eleven unguarded.
//! - Local, instance-property and array-element stores are pinned too. They were already
//!   correct (they acquire unconditionally), and they are what makes the static store's
//!   asymmetry visible if it ever comes back.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir. Host-target only.

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

/// Compiles and runs one PHP source, returning its stdout.
fn compile_and_run(dir: &Path, source: &str) -> String {
    let probe = dir.join("main.php");
    fs::write(&probe, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&probe);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let run = Command::new(dir.join("main"))
        .output()
        .expect("failed to run binary");
    assert!(
        run.status.success(),
        "binary failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Builds a probe that stores `call` in a static property, clobbers the scratch buffer, reads back.
fn static_property_probe(call: &str) -> String {
    format!(
        r#"<?php
class B {{ public static string $s = ""; }}
B::$s = {call};
$noise = str_repeat("Z", 24);
echo B::$s;
"#
    )
}

/// Verifies every unary-string builtin survives a static-property store.
///
/// The scratch buffer is the whole point of the `str_repeat()` line: without it the stale
/// pointer still happens to hold the right bytes and every one of these passes.
#[test]
fn unary_string_results_survive_a_static_property_store() {
    let dir = make_test_dir("static_prop_unary");
    // (call, expected) for the whole `RuntimeCallTarget::UnaryString` family that has a
    // PHP-visible name and a deterministic result.
    let cases = [
        (r#"strtoupper("second")"#, "SECOND"),
        (r#"strtolower("SECOND")"#, "second"),
        (r#"strrev("abcdef")"#, "fedcba"),
        (r#"addslashes("a'b")"#, r#"a\'b"#),
        (r#"stripslashes("a\\b")"#, "ab"),
        (r#"base64_encode("second")"#, "c2Vjb25k"),
        (r#"bin2hex("sec")"#, "736563"),
        (r#"hex2bin("736563")"#, "sec"),
        (r#"urlencode("a b")"#, "a+b"),
        (r#"rawurlencode("a b")"#, "a%20b"),
        (r#"urldecode("a%20b")"#, "a b"),
        (r#"rawurldecode("a%20b")"#, "a b"),
        (r#"quotemeta("a.b")"#, r"a\.b"),
    ];
    for (call, expected) in cases {
        let actual = compile_and_run(&dir, &static_property_probe(call));
        assert_eq!(
            actual, expected,
            "`B::$s = {call}` did not survive the scratch buffer being overwritten"
        );
    }
}

/// Verifies the same value survives a LOCAL, an instance property and an array element.
///
/// These were already correct. They are pinned because they are the comparison that made the
/// static store's missing persist visible in the first place.
#[test]
fn the_other_storage_locations_survive_it_too() {
    let dir = make_test_dir("static_prop_others");

    let local = compile_and_run(
        &dir,
        r#"<?php
$x = strtoupper("second");
$noise = str_repeat("Z", 24);
echo $x;
"#,
    );
    assert_eq!(local, "SECOND", "a local lost its string");

    let instance = compile_and_run(
        &dir,
        r#"<?php
class B { public string $s = ""; }
$b = new B();
$b->s = strtoupper("second");
$noise = str_repeat("Z", 24);
echo $b->s;
"#,
    );
    assert_eq!(instance, "SECOND", "an instance property lost its string");

    let element = compile_and_run(
        &dir,
        r#"<?php
$a = [];
$a["k"] = strtoupper("second");
$noise = str_repeat("Z", 24);
echo $a["k"];
"#,
    );
    assert_eq!(element, "SECOND", "an array element lost its string");
}

/// Verifies a static property survives repeated overwriting, not just one clobber.
///
/// A persist that copied into the SAME scratch block would pass the single-clobber probes.
#[test]
fn a_static_property_survives_repeated_scratch_reuse() {
    let dir = make_test_dir("static_prop_repeat");

    let output = compile_and_run(
        &dir,
        r#"<?php
class B { public static string $a = ""; public static string $b = ""; }
B::$a = strtoupper("first");
B::$b = strtolower("SECOND");
$n1 = str_repeat("Z", 24);
$n2 = strrev("abcdefghijklmnop");
$n3 = base64_encode("more and more noise");
echo B::$a, "|", B::$b, "|", strlen(B::$a), "|", strlen(B::$b);
"#,
    );

    assert_eq!(output, "FIRST|second|5|6");
}
