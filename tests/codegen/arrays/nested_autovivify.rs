//! Purpose:
//! Regression coverage for nested writes through a MISSING intermediate array
//! element (`$a[7][1] = ...` when `$a[7]` does not exist), which PHP resolves
//! by autovivifying the parent element as an array (issue #555).
//!
//! Called from:
//! - `cargo test --test codegen_tests arrays::nested_autovivify`.
//!
//! Key details:
//! - Before the fix, the parent chain of a nested assignment was lowered as a
//!   plain READ: a missing element produced a detached `Mixed(null)` box (plus
//!   an "Undefined array key" warning), `__rt_mixed_array_set` then hit its
//!   incompatible-target drop path, and the write was silently lost.
//! - The fix routes the parent chain through fetch-for-write reads
//!   (`__rt_mixed_array_get_for_write` for boxed Mixed parents,
//!   `__rt_array_ensure_elem_for_write` for concrete container locals) that
//!   install an empty array into the missing slot and return the STORED cell,
//!   so the subsequent three-operand set writes through the parent storage.
//! - PHP emits no undefined-key warning for a legal autovivifying write, so
//!   these tests also assert the absence of the warning on stderr.

use std::process::Command;

use crate::support::{
    compile_and_run_with_heap_debug, elephc_cli_command, make_cli_test_dir,
};

/// Compiles and runs the issue #555 repro through one explicit EIR optimizer mode.
fn run_autovivify_optimizer_fixture(ir_opt: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_nested_autovivify_optimizer");
    let php_path = dir.join("main.php");
    std::fs::write(
        &php_path,
        r#"<?php
$a = [['x', 'y'], 7];
$a[7][1] = 'patched';
echo $a[7][1] . "\n";
"#,
    )
    .expect("failed to write nested-autovivify optimizer fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile nested-autovivify optimizer fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run nested-autovivify optimizer fixture");
    assert!(
        output.status.success(),
        "fixture execution failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    let _ = std::fs::remove_dir_all(&dir);
    (stdout, stderr)
}

/// Issue #555 exact repro: index 7 is past the end of the outer array, the
/// write must create `$a[7]` as an array, store the value, print `patched`,
/// warn nothing, and stay heap-clean.
#[test]
fn test_autovivify_indexed_missing_parent() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];

$a[7][1] = 'patched';

echo $a[7][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Missing parent at the exact append position (index == length).
#[test]
fn test_autovivify_missing_parent_at_append_position() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$a[2][0] = 'append';
echo $a[2][0] . "\n";
echo $a[0][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "append\ny\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A first out-of-range write leaves null gap slots behind; a second nested
/// write must autovivify THROUGH such a null gap slot.
#[test]
fn test_autovivify_null_gap_slot_parent() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$a[7][1] = 'far';
$a[4][0] = 'gap';
echo $a[4][0] . "\n";
echo $a[7][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "gap\nfar\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A slot that explicitly holds null (not a gap) also autovivifies in PHP.
#[test]
fn test_autovivify_boxed_null_slot_parent() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$a[1] = null;
$a[1][0] = 'thruNull';
echo $a[1][0] . "\n";
"#,
    );
    assert_eq!(out.stdout, "thruNull\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Missing string key on a concrete associative-array local.
#[test]
fn test_autovivify_assoc_missing_key_parent() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ['k' => ['a', 1], 'n' => 5];
$a['k2']['x'] = 'assocMiss';
echo $a['k2']['x'] . "\n";
echo $a['k'][0] . "\n";
"#,
    );
    assert_eq!(out.stdout, "assocMiss\na\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Multiple missing levels with string keys, starting from an empty array
/// (`$a['x']['y']['z'] = 1` from the issue's coverage list). The base local
/// promotes from indexed to associative storage on the first string key.
#[test]
fn test_autovivify_multiple_missing_levels_string_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
$a['x']['y']['z'] = 1;
echo $a['x']['y']['z'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "1\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Multiple missing levels with integer keys: every level of the chain is
/// created on the fly.
#[test]
fn test_autovivify_multiple_missing_levels_int_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$a[3][2][1] = 'deep';
echo $a[3][2][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "deep\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Missing string key on a boxed Mixed hash receiver: the fetch-for-write
/// goes through `__rt_mixed_array_get_for_write` on the receiver cell.
#[test]
fn test_autovivify_mixed_assoc_receiver_missing_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function patch(mixed $m): void {
    $m['b']['c'] = 5;
    echo $m['b']['c'] . "\n";
    echo $m['a'] . "\n";
}
patch(['a' => 1, 'z' => 'w']);
"#,
    );
    assert_eq!(out.stdout, "5\n1\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Missing index on a boxed Mixed indexed receiver: the fetch-for-write
/// grows the boxed array and installs the autovivified element.
#[test]
fn test_autovivify_mixed_indexed_receiver_missing_index() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function patch(mixed $m): void {
    $m[5][0] = 'deep';
    echo $m[5][0] . "\n";
    echo $m[1][1] . "\n";
}
patch([[1, 2], [3, 4]]);
"#,
    );
    assert_eq!(out.stdout, "deep\n4\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Issue-style end-to-end check on a json_decode receiver. The stdout and
/// warning behavior are asserted; heap cleanliness is NOT, because Mixed
/// results of builtin calls stored into locals leak one owner reference on
/// current main independently of nested writes (reproducible with a bare
/// `$m = json_decode('{"a":1}', true); echo $m['a'];`).
#[test]
fn test_autovivify_json_decode_receiver_missing_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = json_decode('{"a":1}', true);
$m['b']['c'] = 5;
echo $m['b']['c'] . "\n";
echo $m['a'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "5\n1\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
}

/// A string leaf key on an EXISTING indexed child promotes the child to hash
/// storage instead of dropping the write (PHP array semantics).
#[test]
fn test_autovivify_string_key_promotes_existing_indexed_child() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$a[0]['new'] = 'added';
echo $a[0]['new'] . "\n";
echo $a[0][0] . "\n";
"#,
    );
    assert_eq!(out.stdout, "added\nx\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Existing-parent control from the #529 / #553 suite: autovivification must
/// not disturb in-place writes through parents that already exist.
#[test]
fn test_autovivify_existing_parent_control() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y0'], ['x', 'y1'], 7];
$a[1][1] = 'patched';
$a[5][0] = 'grown';
echo $a[1][1] . "\n";
echo $a[0][1] . "\n";
echo $a[5][0] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\ny0\ngrown\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// COW control: a shared outer array must be split before autovivification,
/// leaving the alias untouched.
#[test]
fn test_autovivify_shared_outer_array_cow() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$b = $a;
$a[7][1] = 'patched';
echo $a[7][1] . "\n";
echo $b[0][0] . "\n";
echo $b[1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\nx\n7\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The autovivified write must survive a function-return boundary.
#[test]
fn test_autovivify_write_visible_after_function_return() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function patch(): array {
    $a = [['x', 'y'], 7];
    $a[7][1] = 'patched';
    return $a;
}
$r = patch();
echo $r[7][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\n");
    assert!(
        !out.stderr.contains("Undefined array key"),
        "autovivifying write must not warn, got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the issue repro stays silent and heap-clean with EIR optimization on and off.
#[test]
fn test_autovivify_with_optimizer_on_and_off() {
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_autovivify_optimizer_fixture(ir_opt);
        assert_eq!(stdout, "patched\n", "unexpected stdout with ir_opt={ir_opt}");
        assert!(
            !stderr.contains("Undefined array key"),
            "autovivifying write must not warn with ir_opt={ir_opt}, got: {stderr}"
        );
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}
