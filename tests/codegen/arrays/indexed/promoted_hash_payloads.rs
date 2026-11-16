//! Purpose:
//! Regression tests for the runtime array setters and appenders when the destination is, or
//! becomes, a promoted hash: refcounted appends after a sparse write, shared-array promotion,
//! and the runtime value tag of float, bool and refcounted payloads inserted into a hash.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output below is the `php -n` output of the same program.
//! - Each program writes through a non-literal key (`$p`), the shape that reaches the runtime
//!   helpers; a literal key is lowered to a hash operation at IR-build time.
//! - Heap-debug runs also assert a clean leak summary where ownership is involved.

use super::*;

/// Asserts a heap-debug run succeeded, printed `expected` and released everything.
fn assert_ran_clean(out: &ProgramOutput, expected: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A refcounted append to an array that an earlier sparse write promoted to a hash. The
/// AArch64 helper saved its value tag over the saved return address, so the routine returned
/// to the tag (a small integer) and crashed.
#[test]
fn test_refcounted_append_after_sparse_write_returns_normally() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class F {}
function show(array $a) {
    echo count($a), "\n";
    foreach ($a as $k => $v) { echo $k, ":", get_debug_type($v), "\n"; }
}
$a = [new F];
$p = 5;
$a[$p] = new F;
$a[] = new F;
show($a);
$b = [[1]];
$b[$p] = [2];
$b[] = [3];
show($b);
"#,
    );
    assert_ran_clean(&out, "3\n0:F\n5:F\n6:F\n3\n0:array\n5:array\n6:array\n");
}

/// Writes of refcounted payloads into a hash that is already promoted keep the payload's own
/// runtime tag (a hash keeps its value_type in its header, not in the kind word), so a mixed
/// reader sees objects and arrays and the hash's deep free releases them.
#[test]
fn test_refcounted_write_into_promoted_hash_keeps_payload_tag() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class F { function __destruct() { echo "d\n"; } }
function show(array $a) {
    echo count($a), "\n";
    foreach ($a as $k => $v) { echo $k, ":", get_debug_type($v), "\n"; }
}
$a = [new F];
$p = 5;
$a[$p] = new F;
$q = 9;
$a[$q] = new F;
$a[] = [1];
show($a);
$a = null;
echo "end\n";
"#,
    );
    assert_ran_clean(&out, "4\n0:F\n5:F\n9:F\n10:array\nd\nd\nd\nend\n");
}

/// Float and bool payloads written through a variable key into a packed array are promoted to
/// a hash under their own runtime tag, not as integers.
#[test]
fn test_float_and_bool_payloads_keep_their_tag_through_promotion() {
    let out = compile_and_run(
        r#"<?php
function show(array $a) { var_dump($a); }
$a = [1.5];
$p = 10;
$a[$p] = 2.5;
$a[] = 3.5;
show($a);
$b = [true];
$b[$p] = false;
$b[] = true;
show($b);
$c = [];
$c[$p] = 1.5;
$c[] = 2.5;
show($c);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  float(1.5)\n  [10]=>\n  float(2.5)\n  [11]=>\n  float(3.5)\n}\n\
         array(3) {\n  [0]=>\n  bool(true)\n  [10]=>\n  bool(false)\n  [11]=>\n  bool(true)\n}\n\
         array(2) {\n  [10]=>\n  float(1.5)\n  [11]=>\n  float(2.5)\n}\n"
    );
}

/// A sparse write into an array whose storage is shared splits it first, then promotes the
/// private copy. The x86_64 promotion arms reached `__rt_array_to_hash` with a stale `rdi`
/// once the split had run, so the copy was converted from garbage.
#[test]
fn test_sparse_write_into_shared_array_promotes_the_private_copy() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show(array $a) {
    echo count($a), ":";
    foreach ($a as $k => $v) { echo " ", $k, "=", is_object($v) ? "obj" : $v; }
    echo "\n";
}
class F {}
$p = 10;
$a = ["x", "y"]; $b = $a; $b[$p] = "z"; show($b); show($a);
$c = [1, 2]; $d = $c; $d[$p] = 7; show($d); show($c);
$e = [new F]; $f = $e; $f[$p] = new F; show($f); show($e);
"#,
    );
    assert_ran_clean(
        &out,
        "3: 0=x 1=y 10=z\n2: 0=x 1=y\n3: 0=1 1=2 10=7\n2: 0=1 1=2\n2: 0=obj 10=obj\n1: 0=obj\n",
    );
}
