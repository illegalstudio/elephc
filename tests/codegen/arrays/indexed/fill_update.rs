//! Purpose:
//! Verifies that a contiguous fill loop lets a following read-modify-write loop keep raw
//! packed float storage while still matching PHP for partial fills and aliases.
//!
//! Called from:
//! - `cargo test` through the indexed array suite.
//!
//! Key details:
//! - The fill loop must write every key through the same counter and bound as the consumer.
//! - A whole-array rebind, unset, reference alias or call clears the proof.
//! - The IR assertion checks the promoted packed setter, not just stdout.

use crate::support::*;

/// Emits the optimized `main` function body for one fixture.
fn main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_fill_update_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write fill-update fixture");
    let output = elephc_cli_command(&dir)
        .args(["--emit-ir", "--ir-opt=on"])
        .arg(&path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string();
    let _ = fs::remove_dir_all(dir);
    main
}

/// A fill loop followed by a read-modify-write loop keeps raw float slots.
#[test]
fn test_fill_then_update_keeps_raw_float_storage() {
    let source = r#"<?php
$count = 10 + $argc;
$x = [];
$v = [];
for ($i = 0; $i < $count; $i++) {
    $x[$i] = 0.0;
    $v[$i] = 1.5;
}
for ($step = 0; $step < 3; $step++) {
    for ($i = 0; $i < $count; $i++) {
        $x[$i] = $x[$i] + $v[$i];
    }
}
$sum = 0;
for ($i = 0; $i < $count; $i++) {
    $sum += (int) $x[$i];
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "44\n");
    let main = main_ir(source);
    assert!(main.contains("promotable_array"), "{main}");
    assert!(!main.contains("array_set_mixed_key"), "{main}");
    assert_eq!(main.matches("F64 php=float = array_get").count(), 3, "{main}");
}

/// A partial fill (a nested branch can skip a key) keeps the ordinary nullable path.
#[test]
fn test_partial_fill_does_not_enable_raw_float_reads() {
    let source = r#"<?php
$count = 4 + $argc;
$x = [];
$v = [];
for ($i = 0; $i < $count; $i++) {
    if ($i !== 1) {
        $x[$i] = 0.0;
        $v[$i] = 1.5;
    }
}
for ($i = 0; $i < $count; $i++) {
    $x[$i] = $x[$i] + $v[$i];
}
$sum = 0;
for ($i = 0; $i < $count; $i++) {
    $sum += (int) $x[$i];
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "4\n");
    let main = main_ir(source);
    assert!(main.contains("array_set_mixed_key"), "{main}");
}

/// Rebinding the array after the fill clears the proof.
#[test]
fn test_rebind_after_fill_clears_raw_float_reads() {
    let source = r#"<?php
$count = 3 + $argc;
$x = [];
for ($i = 0; $i < $count; $i++) {
    $x[$i] = 1.5;
}
$x = [];
for ($i = 0; $i < $count; $i++) {
    $x[$i] = $x[$i] + 1.0;
}
echo (int) $x[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "1\n");
    let main = main_ir(source);
    assert!(main.contains("array_set_mixed_key"), "{main}");
}

/// The same fill-then-update proof applies inside a function body.
#[test]
fn test_fill_then_update_inside_function_keeps_raw_float_storage() {
    let source = r#"<?php
function run(int $n): int {
    $x = [];
    $v = [];
    for ($i = 0; $i < $n; $i++) {
        $x[$i] = 0.0;
        $v[$i] = 1.5;
    }
    for ($step = 0; $step < 3; $step++) {
        for ($i = 0; $i < $n; $i++) {
            $x[$i] = $x[$i] + $v[$i];
        }
    }
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += (int) $x[$i];
    }
    return $sum;
}
echo run(4 + $argc), "\n";
"#;
    assert_eq!(compile_and_run(source), "20\n");
}

/// A static hash-forcing key wins over a keyed float candidate on the same local: the
/// counter-shaped write must not keep the fast storage contract after `$a[-3] = 1` or a
/// literal string key has already moved the array to hash storage.
#[test]
fn test_static_hash_key_wins_over_keyed_float_candidate() {
    let source = r#"<?php
$a = [];
$a[-3] = 1;
$a[] = 2;
for ($i = 0; $i < 3; $i++) {
    $a[$i] = 0.0;
}
echo implode(",", array_keys($a)), "\n";
echo implode(",", $a), "\n";
$b = [];
for ($i = 0; $i < 2; $i++) {
    $b["k"] = 1.0;
    $b[$i] = 2.0;
}
echo implode(",", array_keys($b)), "\n";
echo implode(",", $b), "\n";
"#;
    assert_eq!(compile_and_run(source), "-3,-2,0,1,2\n1,2,0,0,0\nk,0,1\n1,2,2\n");
}
