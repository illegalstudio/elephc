//! Purpose:
//! Opt-in timing guards for the String hot paths.
//!
//! Called from:
//! - `cargo test --test codegen_tests perf_regressions -- --ignored`.
//!
//! Key details:
//! - These tests compile a small native fixture, run it once, and assert a generous
//!   wall-clock ceiling. They are ignored by default because CI runners vary widely and
//!   the deterministic IR/asm guards are the merge gate.
//! - The ceilings are intentionally loose: they catch order-of-magnitude regressions
//!   (for example, losing a length-only sink and allocating per iteration), not small
//!   timing noise.
//! - Run with `--release` when comparing against a release compiler locally.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::support::*;

/// Compiles one fixture and returns the wall-clock time of one native run.
fn compile_and_time(source: &str) -> Duration {
    let dir = make_cli_test_dir("elephc_perf_regression");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write perf fixture");
    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("run elephc");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let binary = php_path.with_extension("");
    // The first execution after linking pays cold code-signing/page-cache costs.
    let warm = Command::new(&binary).output().expect("warm perf fixture");
    assert!(
        warm.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&warm.stderr)
    );
    let start = Instant::now();
    let run = Command::new(&binary).output().expect("run perf fixture");
    let elapsed = start.elapsed();
    let _ = fs::remove_dir_all(dir);
    assert!(
        run.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    elapsed
}

/// Asserts one timing ceiling with the fixture name in the failure.
fn assert_under(name: &str, source: &str, limit: Duration) {
    let elapsed = compile_and_time(source);
    assert!(
        elapsed < limit,
        "{name} took {elapsed:?}, ceiling {limit:?}"
    );
}

/// The raw `strstr()` length helper must not allocate a boxed suffix per iteration.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_strstr_len_loop() {
    assert_under(
        "strstr_len",
        r#"<?php
$s = str_repeat('a', 100) . 'needle';
$n = 0;
for ($i = 0; $i < 200000; $i++) { $n ^= (strlen(strstr($s, 'needle')) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(100),
    );
}

/// All four position searches keep the raw integer path under a cast.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_search_raw_loop() {
    assert_under(
        "search_raw",
        r#"<?php
$s = str_repeat('a', 100) . 'Needle';
$n = 0;
for ($i = 0; $i < 200000; $i++) {
    $n ^= ((int)strpos($s, 'Needle') + (int)stripos($s, 'needle'));
    $n ^= ((int)strrpos($s, 'Needle') + (int)strripos($s, 'needle') + $i);
}
echo $n, "\n";
"#,
        Duration::from_millis(200),
    );
}

/// The three-argument `strtr()` length fold must not rebuild the translation table.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_strtr_len_loop() {
    assert_under(
        "strtr_len",
        r#"<?php
$s = str_repeat('abc', 40);
$n = 0;
for ($i = 0; $i < 100000; $i++) { $n ^= (strlen(strtr($s, 'abc', 'xyz')) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(100),
    );
}

/// The `chunk_split()` length fold must not allocate the split string.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_chunk_split_len_loop() {
    assert_under(
        "chunk_split_len",
        r#"<?php
$s = str_repeat('abcdef', 20);
$n = 0;
for ($i = 0; $i < 100000; $i++) { $n ^= (strlen(chunk_split($s, 76, "\r\n")) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(80),
    );
}

/// `ucwords()` keeps the default-separator fast path.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_ucwords_loop() {
    assert_under(
        "ucwords",
        r#"<?php
$s = 'the quick brown fox jumps over the lazy dog';
$n = 0;
for ($i = 0; $i < 100000; $i++) { $n ^= (strlen(ucwords($s)) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(120),
    );
}

/// `strlen(strip_tags())` keeps the count-only scanner instead of publishing and
/// persisting a temporary string per iteration.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_strip_tags_len_loop() {
    assert_under(
        "strip_tags_len",
        r#"<?php
$s = '<p>Hello <b>world</b> <a href="x">link</a></p>';
$n = 0;
for ($i = 0; $i < 50000; $i++) { $n ^= (strlen(strip_tags($s)) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(100),
    );
}

/// `vsprintf()` keeps the direct scalar record fast path and the unadorned-decimal
/// formatter instead of reintroducing a libc `snprintf` per `%d`.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_vsprintf_plain_decimal_loop() {
    assert_under(
        "vsprintf_decimal",
        r#"<?php
$v = [42, 'x'];
$n = 0;
for ($i = 0; $i < 50000; $i++) { $n ^= (strlen(vsprintf('%d-%s', $v)) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(100),
    );
}

/// `strlen(inet_pton())` keeps the allocation-free packed-width mode.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_inet_pton_len_loop() {
    assert_under(
        "inet_pton_len",
        r#"<?php
$s = '127.0.0.1';
$n = 0;
for ($i = 0; $i < 100000; $i++) { $n ^= (strlen(inet_pton($s)) + $i); }
echo $n, "\n";
"#,
        Duration::from_millis(100),
    );
}

/// `(int)ip2long()` keeps the raw integer parser and can be hoisted when loop-invariant.
#[test]
#[ignore = "wall-clock performance guard; run explicitly"]
fn perf_ip2long_int_loop() {
    assert_under(
        "ip2long_int",
        r#"<?php
$s = '192.168.1.42';
$n = 0;
for ($i = 0; $i < 100000; $i++) { $n ^= ((int)ip2long($s)) + $i; }
echo $n, "\n";
"#,
        Duration::from_millis(100),
    );
}
