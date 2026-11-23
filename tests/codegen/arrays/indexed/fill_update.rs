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
    // The loop's own `$i++` is control, not a proof-breaking write, so both the update loop's
    // two reads and the later sum loop's read stay fill-proven and raw.
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

/// The fill proof only covers the range the fill loop actually wrote: switching the consumer
/// from `<` to `<=` reaches one key past the fill and must stay a nullable read.
#[test]
fn test_fill_proof_requires_matching_range_operator() {
    let source = r#"<?php
$count = 3 + $argc;
$x = [];
for ($i = 0; $i < $count; $i++) {
    $x[$i] = 1.5;
}
$last = null;
for ($i = 0; $i <= $count; $i++) {
    $last = $x[$i];
}
var_export($last);
"#;
    assert_eq!(compile_and_run(source), "NULL");
}

/// A promotable float read keeps the element type: an unannotated function returning it must
/// infer `?float`, not the integer-or-null shape of an integer array.
#[test]
fn test_promotable_float_read_keeps_float_return_type() {
    let source = r#"<?php
function first(int $n) {
    $a = [];
    for ($i = 0; $i < $n; $i++) {
        $a[$i] = 2.5;
    }
    return $a[0];
}
echo first(3 + $argc), "\n";
"#;
    assert_eq!(compile_and_run(source), "2.5\n");
}

/// A removal inside the consumer loop invalidates the reads earlier in the same body too: the
/// next iteration re-executes them after the pop, so they must stay nullable.
#[test]
fn test_loop_carried_removal_unmarks_earlier_fill_reads() {
    let source = r#"<?php
$count = 3 + $argc;
$x = [];
for ($i = 0; $i < $count; $i++) {
    $x[$i] = 1.5;
}
$last = null;
for ($i = 0; $i < $count; $i++) {
    $last = $x[$i];
    array_pop($x);
}
var_export($last);
"#;
    assert_eq!(compile_and_run(source), "NULL");
}

/// A call after a completed fill-and-update loop must not unmark the reads that loop already
/// executed: only consumer loops still being checked can see a later removal.
#[test]
fn test_later_call_keeps_completed_fill_reads_raw() {
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
$n = count($x);
$sum = 0;
for ($i = 0; $i < $count; $i++) {
    $sum += (int) $x[$i];
}
echo $sum, "|", $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "44|11\n");
    let main = main_ir(source);
    // The completed update loop keeps its two raw reads; the sum loop after the call boxes.
    assert_eq!(main.matches("F64 php=float = array_get").count(), 2, "{main}");
}
