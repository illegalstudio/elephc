//! Purpose:
//! Checks bounded integer counters in numeric closure loops against real PHP.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - IR-on/off execution, heap validation and complete target emission guard representation changes.

use super::*;

/// Runs the selected IR mode with strict heap checks and optionally the installed PHP oracle.
fn run(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_bounded_counter");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut cmd = elephc_cli_command(&dir);
    cmd.args(["--heap-debug", "--gc-stats"]);
    if !optimized {
        cmd.arg("--no-ir-opt");
    }
    let compiled = cmd.arg(&path).output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = run_binary(&path.with_extension(""), &dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert!(
            php.status.success(),
            "{}",
            String::from_utf8_lossy(&php.stderr)
        );
        assert_eq!(output.stdout, php.stdout, "actual PHP oracle");
    }
    let text = String::from_utf8(output.stdout).unwrap();
    fs::remove_dir_all(dir).unwrap();
    text
}

/// Canonical closure counters retain integer arguments and exact numeric results in both modes.
#[test]
fn test_bounded_mixed_counter_closure_total_matches_php() {
    for optimized in [false, true] {
        assert_eq!(
            run(
                include_str!("../../../examples/bounded-closure-counter/main.php"),
                optimized
            ),
            "500500"
        );
    }
}

/// An unrelated loop bound never proves that a PHP_INT_MAX argument increment cannot overflow.
#[test]
fn test_bounded_mixed_counter_overflow_and_late_type_changes_keep_call_gates() {
    let source = r#"<?php
function boundaries(int $n): void {
    $f = function($x) { return $x; };
    $value = PHP_INT_MAX;
    for ($i=0; $i<$n; $i++) { $result=$f($value); $value++; }
    echo gettype($result), '|';
}
function late_null(int $n): void {
    $f=function($x) { return $x; }; $value=10;
    for ($i=0; $i<$n; $i++) { $result=$f($value); $value=null; $sink=$f($i); }
    echo gettype($result), ':', $result, '|';
}
function late_bool(int $n): void {
    $f=function($x) { return $x; }; $value=10;
    for ($i=0; $i<$n; $i++) { $result=$f($value); $value=true; $sink=$f($i); }
    echo gettype($result), ':', $result, '|';
}
boundaries($argc+1);late_null($argc+1);late_bool($argc+1);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), "double|NULL:|boolean:1|");
    }
}

/// Reference captures and delayed argument snapshots preserve the original PHP value sequence.
#[test]
fn test_bounded_mixed_counter_aliases_and_values_are_independent() {
    let source = r#"<?php
function reference_counter(int $n): void {
    $i=0; $r=&$i; $f=function($x) { return $x+1; }; $sum=0;
    while ($i<$n) { $sum+=$f($i); $i++; }
    echo $r, ':', $sum, '|';
}
function saved_counter(int $n): void {
    $i=0; $saved=[]; $f=function($x) { return $x+1; };
    while ($i<$n) { $saved[]=$i; $sink=$f($i); $i++; }
    echo $saved[0], ':', $saved[2], ':', $i, '|';
}
reference_counter($argc+2);saved_counter($argc+2);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), "3:6|0:2:3|");
    }
}

/// Every target materializes unchanged Mixed call parameters from a proven integer counter.
#[test]
fn test_bounded_mixed_counter_emits_on_all_supported_targets() {
    let source = include_str!("../../../examples/bounded-closure-counter/main.php");
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_bounded_counter_target");
            let path = dir.join("main.php");
            fs::write(&path, source).unwrap();
            let mut cmd = elephc_cli_command(&dir);
            cmd.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") {
                cmd.args(["--emit", "staticlib"]);
            }
            if !optimized {
                cmd.arg("--no-ir-opt");
            }
            let output = cmd.arg(&path).output().unwrap();
            assert!(
                output.status.success(),
                "{target}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!fs::read_to_string(path.with_extension("s"))
                .unwrap()
                .is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

/// The real PHP lowering must expose an integer counter, rather than merely preserve output.
#[test]
fn test_bounded_mixed_counter_real_lowering_uses_integer_recurrence() {
    let dir = make_cli_test_dir("elephc_bounded_counter_ir");
    let path = dir.join("main.php");
    fs::write(
        &path,
        include_str!("../../../examples/bounded-closure-counter/main.php"),
    )
    .unwrap();
    for optimized in [false, true] {
        let mut cmd = elephc_cli_command(&dir);
        cmd.arg("--emit-ir");
        if !optimized {
            cmd.arg("--no-ir-opt");
        }
        let output = cmd.arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = String::from_utf8(output.stdout).unwrap();
        let body = ir
            .split("function closure_total(")
            .nth(1)
            .unwrap()
            .split("\n  function ")
            .next()
            .unwrap();
        if optimized {
            assert!(!body.contains("str_inc_dec"), "{body}");
            assert!(!body.contains("php_rel_cmp"), "{body}");
            assert!(body.contains("icmp"), "{body}");
            assert!(body.contains("iadd"), "{body}");
        } else {
            assert!(body.contains("str_inc_dec"), "{body}");
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
