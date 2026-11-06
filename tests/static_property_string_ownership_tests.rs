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

/// Returns the EIR instruction that produced the value `source`'s first static store writes.
///
/// Follows the stored value through its `acquire` to the instruction that made it. A probe
/// whose producer turns out to be `const_str` stores a rodata literal, never touches the
/// scratch buffer, and proves nothing — so the producer table asserts on this.
fn static_store_producer(dir: &Path, source: &str) -> String {
    let probe = dir.join("ir.php");
    fs::write(&probe, source).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .arg("--emit-ir")
        .arg(&probe)
        .output()
        .expect("failed to spawn elephc");
    assert!(output.status.success(), "--emit-ir failed: {}", String::from_utf8_lossy(&output.stderr));
    let ir = String::from_utf8_lossy(&output.stdout).into_owned();
    let lines: Vec<&str> = ir.lines().map(str::trim).collect();
    let store_at = lines
        .iter()
        .position(|line| line.starts_with("store_static_property "))
        .unwrap_or_else(|| panic!("no static store in:\n{ir}"));
    // Value numbers restart in every function, so a definition is searched BACKWARDS from
    // the store: the nearest one is the store's own function's, never another's `vN`.
    let definition = |value: &str| -> String {
        let prefix = format!("{value}: ");
        lines[..store_at]
            .iter()
            .rev()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no definition of {value} in:\n{ir}"))
            .to_string()
    };
    let stored = lines[store_at].split_whitespace().nth(1).unwrap();
    let mut producer = definition(stored);
    if let Some(rest) = producer.split(" = acquire ").nth(1) {
        let source_value = rest.split_whitespace().next().unwrap().to_string();
        producer = definition(&source_value);
    }
    producer
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

/// Compiles `source` with `--heap-debug` and returns the program's stderr heap report.
fn run_with_heap_debug(dir: &Path, name: &str, source: &str) -> String {
    let probe = dir.join(format!("{name}.php"));
    fs::write(&probe, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--heap-debug");
    cmd.arg(&probe);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\nstderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let run = Command::new(dir.join(name))
        .output()
        .expect("failed to run binary");
    assert!(
        run.status.success(),
        "binary failed:\nstderr: {}",
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8_lossy(&run.stderr).into_owned()
}

/// Reads `live_blocks=N` out of a `--heap-debug` report.
fn live_blocks(stderr: &str) -> u64 {
    stderr
        .split_whitespace()
        .find_map(|token| token.strip_prefix("live_blocks="))
        .unwrap_or_else(|| panic!("no live_blocks= in heap report:\n{stderr}"))
        .parse()
        .expect("live_blocks was not a number")
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

/// Verifies the store survives for EVERY string producer, not just the unary-string family.
///
/// This is the test that decides WHERE the fix belongs. Reclassifying runtime-call `Str`
/// results as non-owning at the producer fixes only `RuntimeCallTarget::UnaryString`, because
/// that is the only family such a predicate can name. Concatenation, interpolation, an `(int)`
/// cast and `strval()` reach the scratch buffer by different opcodes and were still clobbered.
/// Persisting at the STORE covers all of them at once, and this table is what says so: if the
/// fix ever migrates back to the producer, the `UnaryString` row keeps passing and these fail.
///
/// EVERY INPUT DEPENDS ON `$argc`, so no row can be constant-folded into a rodata literal,
/// which would sidestep the scratch buffer and make the row vacuous. Reading a constant out
/// of a variable is NOT enough: the optimizer propagates it, and four rows of an earlier
/// version (`$x . "c"`, `"v=$x!"`, `(string)$i`, `(string)$b`) stored a `const_str` and
/// exercised nothing. The ternary keeps each value's own type — `$argc + 41` would make the
/// integer rows `mixed` and test a different cast. Each row also asserts, on the EIR, that
/// its producer survived, so a smarter optimizer fails this test instead of hollowing it.
#[test]
fn every_string_producer_survives_a_static_property_store() {
    let dir = make_test_dir("static_prop_producers");
    // (prelude, expression, expected)
    let cases = [
        (r#"$x = $argc > 0 ? "ab" : "";"#, r#"$x . "c""#, "abc"),
        (r#"$x = $argc > 0 ? "ab" : "";"#, r#""v=$x!""#, "v=ab!"),
        (r#"$i = $argc > 0 ? 42 : 0;"#, r#"(string)$i"#, "42"),
        (r#"$i = $argc > 0 ? 42 : 0;"#, r#"strval($i)"#, "42"),
        (r#"$x = $argc > 0 ? "ab" : "";"#, r#"strtoupper($x) . "!""#, "AB!"),
        (r#"$x = $argc > 0 ? "ab" : "";"#, r#"str_repeat($x, 2)"#, "abab"),
        (r#"$f = $argc > 0 ? 1.5 : 0.0;"#, r#"(string)$f"#, "1.5"),
        (r#"$b = $argc > 0;"#, r#"(string)$b"#, "1"),
    ];
    for (prelude, call, expected) in cases {
        let source = format!(
            r#"<?php
class B {{ public static string $s = ""; }}
{prelude}
B::$s = {call};
$noise = str_repeat("Z", 24);
echo B::$s;
"#
        );
        let producer = static_store_producer(&dir, &source);
        assert!(
            !producer.contains(" = const_str "),
            "`B::$s = {call}` was folded to a literal, so it tests nothing: {producer}"
        );
        let actual = compile_and_run(&dir, &source);
        assert_eq!(
            actual, expected,
            "`B::$s = {call}` did not survive the scratch buffer being overwritten"
        );
    }
}

/// Verifies the persist does not LEAK when the producer's result is heap-backed.
///
/// Above the 64 KiB `_concat_buf` capacity `__rt_concat_reserve` stops handing out scratch and
/// returns an owned heap block instead, so the same call that must be persisted below the
/// threshold must also be RELEASED above it. An earlier attempt at this fix suppressed the
/// release for every runtime-call string and leaked one block per call — invisible in a CLI
/// one-shot, unbounded in a `--web` worker.
///
/// The assertion compares two iteration counts rather than checking `live_blocks=0`, because a
/// correct program legitimately ends with live blocks: `$big` is still in scope at exit. A
/// leak is what SCALES with the loop, so that is what is measured; an absolute count would
/// force the test to encode how many values happen to be live at exit, which is neither stable
/// nor the property under test.
#[test]
fn a_heap_backed_producer_does_not_leak_through_the_static_store() {
    let dir = make_test_dir("static_prop_bigleak");
    let program = |iterations: u32| {
        format!(
            r#"<?php
class B {{ public static string $s = ""; }}
$big = str_repeat("a'b", 30000);
for ($i = 0; $i < {iterations}; $i++) {{
    B::$s = addslashes($big);
}}
echo strlen(B::$s), "\n";
"#
        )
    };

    let few = run_with_heap_debug(&dir, "few", &program(5));
    let many = run_with_heap_debug(&dir, "many", &program(50));
    let (few_blocks, many_blocks) = (live_blocks(&few), live_blocks(&many));

    assert_eq!(
        few_blocks, many_blocks,
        "live blocks scale with the iteration count, so the store leaks one per call:\n\
         5 iterations -> {few_blocks}\n50 iterations -> {many_blocks}\n\n{many}"
    );
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
