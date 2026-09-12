//! Purpose:
//! Regression tests for reading an array whose STORAGE was promoted to a hash at run time
//! while its static type stayed `Array(Mixed)`.
//!
//! Called from:
//! - `cargo test --test runtime_promoted_array_reader_tests` through Rust's test harness.
//!
//! Key details:
//! - `__rt_array_set_mixed_key` promotes the destination to hash storage when a key does not
//!   fit the packed layout (a `foreach` key is always a boxed `Mixed` cell, so this is the
//!   ordinary path for rebuilding an array inside a loop). The static type does not move with
//!   it, so a consumer compiled for the indexed layout reads hash internals: `var_dump` of a
//!   one-entry `[5 => 3]` printed `[0]=> int(9)`.
//! - The WRITE was never wrong — `count()`, `isset()` and `foreach` over the DIRECTLY typed
//!   value all agreed with PHP. The readers were, and they were wrong in three different
//!   ways, one per group of tests below:
//!     1. `var_dump`, `print_r`, `serialize` and `json_encode` walk the payload in emitted
//!        assembly. Their indexed entries now probe the heap-kind byte and tail-jump to their
//!        own hash counterpart.
//!     2. `implode`'s renderers need a DENSE payload and cannot walk a hash at all, so its
//!        operand is materialized at lowering time instead: promoted storage is copied through
//!        the same extraction `array_values()` uses, packed storage is merely retained.
//!     3. `foreach` over a `Mixed` picked its path from the boxed TAG, which still said
//!        "indexed"; it read the hash header's first word as a length of zero and ran the body
//!        no times. `var_export()` is a PHP-level prelude built on exactly that loop, which is
//!        why it printed an empty `array (...)` for a one-entry array.
//! - Both control shapes are pinned alongside: a genuinely packed array and a genuinely
//!   associative one must keep rendering exactly as before, since the fix adds a runtime
//!   branch to the path they share.
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
        "binary failed (exit {:?}):\nstdout: {}\nstderr: {}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// The prelude that produces a runtime-promoted array: rebuilding through a `foreach` key.
const PROMOTE: &str = r#"$src = [5 => "x"];
$a = [];
foreach ($src as $k => $v) { $a[$k] = 3; }
"#;

/// Verifies the WRITE was always correct, which is what localises the bug to the readers.
///
/// If this ever fails, the promotion itself broke and the reader tests below are measuring
/// the wrong thing.
#[test]
fn the_promoted_write_itself_is_correct() {
    let dir = make_test_dir("promoted_write");

    let output = compile_and_run(
        &dir,
        &format!(
            r#"<?php
{PROMOTE}echo "count=", count($a), "\n";
echo "has5=", (isset($a[5]) ? "y" : "n"), "\n";
echo "has0=", (isset($a[0]) ? "y" : "n"), "\n";
foreach ($a as $kk => $vv) {{ echo "pair=", $kk, "=>", $vv, "\n"; }}
"#
        ),
    );

    assert_eq!(output, "count=1\nhas5=y\nhas0=n\npair=5=>3\n");
}

/// Verifies `var_dump` renders a runtime-promoted array as the sparse array it is.
#[test]
fn var_dump_renders_a_promoted_array() {
    let dir = make_test_dir("promoted_var_dump");

    let output = compile_and_run(&dir, &format!("<?php\n{PROMOTE}var_dump($a);\n"));

    assert_eq!(output, "array(1) {\n  [5]=>\n  int(3)\n}\n");
}

/// Verifies `print_r` renders a runtime-promoted array as the sparse array it is.
#[test]
fn print_r_renders_a_promoted_array() {
    let dir = make_test_dir("promoted_print_r");

    let output = compile_and_run(&dir, &format!("<?php\n{PROMOTE}print_r($a);\n"));

    assert_eq!(output, "Array\n(\n    [5] => 3\n)\n");
}

/// Verifies `serialize` encodes a runtime-promoted array with its real key.
#[test]
fn serialize_encodes_a_promoted_array() {
    let dir = make_test_dir("promoted_serialize");

    let output = compile_and_run(&dir, &format!("<?php\n{PROMOTE}echo serialize($a);\n"));

    assert_eq!(output, "a:1:{i:5;i:3;}");
}

/// Verifies a genuinely PACKED array still renders identically through all three readers.
///
/// The fix adds a runtime branch to the path packed arrays take, so they are pinned here
/// rather than assumed untouched.
#[test]
fn a_packed_array_still_renders_unchanged() {
    let dir = make_test_dir("promoted_control_packed");

    let output = compile_and_run(
        &dir,
        r#"<?php
$p = [1, 2, 3];
var_dump($p);
print_r($p);
echo serialize($p);
"#,
    );

    assert_eq!(
        output,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n\
         Array\n(\n    [0] => 1\n    [1] => 2\n    [2] => 3\n)\n\
         a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}"
    );
}

/// Verifies a genuinely ASSOCIATIVE array still renders identically through all three readers.
#[test]
fn an_associative_array_still_renders_unchanged() {
    let dir = make_test_dir("promoted_control_hash");

    let output = compile_and_run(
        &dir,
        r#"<?php
$h = ["x" => 1, "y" => 2];
var_dump($h);
print_r($h);
echo serialize($h);
"#,
    );

    assert_eq!(
        output,
        "array(2) {\n  [\"x\"]=>\n  int(1)\n  [\"y\"]=>\n  int(2)\n}\n\
         Array\n(\n    [x] => 1\n    [y] => 2\n)\n\
         a:2:{s:1:\"x\";i:1;s:1:\"y\";i:2;}"
    );
}

/// Verifies a nested container still renders, since the readers recurse through the same entry.
#[test]
fn a_nested_container_still_renders_unchanged() {
    let dir = make_test_dir("promoted_control_nested");

    let output = compile_and_run(
        &dir,
        r#"<?php
$n = [[1, 2], ["k" => 3]];
var_dump($n);
"#,
    );

    assert_eq!(
        output,
        "array(2) {\n  [0]=>\n  array(2) {\n    [0]=>\n    int(1)\n    [1]=>\n    int(2)\n  }\n  \
         [1]=>\n  array(1) {\n    [\"k\"]=>\n    int(3)\n  }\n}\n"
    );
}

/// A two-entry promotion, for the readers whose glue or separator only shows with several.
const PROMOTE_TWO: &str = r#"$src = [5 => "x", 9 => "y"];
$a = [];
foreach ($src as $k => $v) { $a[$k] = 3; }
"#;

/// Verifies `json_encode` renders a runtime-promoted array in PHP's OBJECT form.
///
/// The keys are what force it: PHP emits `{"5":3}` rather than `[3]` because they are not the
/// sequential `0..count-1` that justifies array form.
#[test]
fn json_encode_renders_a_promoted_array_as_an_object() {
    let dir = make_test_dir("promoted_json");

    let output = compile_and_run(&dir, &format!("<?php\n{PROMOTE}echo json_encode($a);\n"));

    assert_eq!(output, "{\"5\":3}");
}

/// Verifies `implode` joins the VALUES of a runtime-promoted array.
///
/// `implode` is not a walker like the others: its renderers require a dense payload, so this
/// pins a lowering-time materialization rather than a runtime tail jump. Two entries, so the
/// glue between them is exercised too.
#[test]
fn implode_joins_a_promoted_array() {
    let dir = make_test_dir("promoted_implode");

    let output = compile_and_run(
        &dir,
        &format!("<?php\n{PROMOTE_TWO}echo implode(\",\", $a), \"\\n\";\n"),
    );

    assert_eq!(output, "3,3\n");
}

/// Verifies `var_export` renders a runtime-promoted array.
///
/// `var_export` is a PHP-level prelude whose walker is `foreach` over a `mixed` parameter, so
/// this is the one case that reaches the iterator fix through a builtin.
#[test]
fn var_export_renders_a_promoted_array() {
    let dir = make_test_dir("promoted_var_export");

    let output = compile_and_run(&dir, &format!("<?php\n{PROMOTE}echo var_export($a, true);\n"));

    assert_eq!(output, "array (\n  5 => 3,\n)");
}

/// Verifies `foreach` over a `Mixed` holding a runtime-promoted array visits its entries.
///
/// This is the defect `var_export` inherited, stated directly: the boxed tag says "indexed"
/// while the storage is a hash, and the loop body used to run zero times.
#[test]
fn foreach_over_a_boxed_promoted_array_visits_its_entries() {
    let dir = make_test_dir("promoted_foreach_mixed");

    let output = compile_and_run(
        &dir,
        &format!(
            r#"<?php
{PROMOTE}function walk(mixed $m): void {{
    foreach ($m as $k => $v) {{ echo "pair=", $k, "=>", $v, "\n"; }}
}}
walk($a);
"#
        ),
    );

    assert_eq!(output, "pair=5=>3\n");
}

/// Verifies `implode` still joins every element layout it has a dedicated renderer for.
///
/// The operand now goes through a run-time branch on ALL of these, so each layout is pinned
/// rather than assumed untouched — including the empty literal, whose element type is
/// uninhabited and describes no layout at all, and the single-argument `join()` form.
/// Expectations taken from `php -n`.
#[test]
fn implode_still_joins_every_packed_layout() {
    let dir = make_test_dir("promoted_implode_control");

    let output = compile_and_run(
        &dir,
        r#"<?php
echo implode(",", [1, 2, 3]), "\n";
echo implode("|", [1.5, 2.25, 40.0]), "\n";
echo implode(",", [true, false, true]), "\n";
echo implode("-", ["a", "b", "c"]), "\n";
echo implode(",", ["x" => 1, "y" => 2]), "\n";
echo "[", implode(",", []), "]\n";
echo join(["a", "b"]), "\n";
"#,
    );

    assert_eq!(output, "1,2,3\n1.5|2.25|40\n1,,1\na-b-c\n1,2\n[]\nab\n");
}

/// Verifies an array joined TWICE is still intact afterwards.
///
/// The packed branch of the new materialization RETAINS the operand and the caller releases it
/// after the join. If that pairing were unbalanced, the second join — or the read after it —
/// would be reading freed storage.
#[test]
fn a_twice_joined_array_survives_both_joins() {
    let dir = make_test_dir("promoted_implode_refcount");

    let output = compile_and_run(
        &dir,
        r#"<?php
$reused = [7, 8, 9];
echo implode(",", $reused), "\n";
echo implode(":", $reused), "\n";
echo count($reused), " ", $reused[1], "\n";
"#,
    );

    assert_eq!(output, "7,8,9\n7:8:9\n3 8\n");
}

/// Verifies `foreach` over a `Mixed` still visits every iterable shape it accepted before.
///
/// The indexed tag now carries a run-time probe, so the shapes that reach it unchanged — and
/// the shapes that never reach it at all — are pinned together. Expectations from `php -n`.
#[test]
fn foreach_over_a_boxed_iterable_still_visits_every_shape() {
    let dir = make_test_dir("promoted_foreach_control");

    let output = compile_and_run(
        &dir,
        r#"<?php
function walk(mixed $m): void {
    foreach ($m as $k => $v) { echo "  ", $k, "=>", (is_array($v) ? "array" : $v), "\n"; }
    echo "  --\n";
}
walk([1, 2, 3]);
walk(["a" => 1, "b" => 2]);
walk([]);
walk([[1], [2]]);
walk([3 => "x", 7 => "y"]);
class Bag implements IteratorAggregate {
    public function getIterator(): Iterator { return new ArrayIterator([10, 20]); }
}
walk(new Bag());
"#,
    );

    assert_eq!(
        output,
        concat!(
            "  0=>1\n  1=>2\n  2=>3\n  --\n",
            "  a=>1\n  b=>2\n  --\n",
            "  --\n",
            "  0=>array\n  1=>array\n  --\n",
            "  3=>x\n  7=>y\n  --\n",
            "  0=>10\n  1=>20\n  --\n",
        )
    );
}

/// Verifies a promoted array whose static element type is a CONCRETE scalar reads correctly.
///
/// This is the case that decides where the probes have to live. The promotion does not depend
/// on the element type — `__rt_array_set_mixed_key` promotes on the KEY — so an `array<int>`
/// is promoted exactly like an `array<mixed>`, and every reader has to probe rather than trust
/// its static type. It is also the only case that reaches `__rt_json_encode_array_int`'s own
/// probe, since the generic encoder is not the one lowering picks for an int array.
/// Expectations taken from `php -n`.
#[test]
fn a_promoted_array_with_a_concrete_element_type_reads_correctly() {
    let dir = make_test_dir("promoted_typed_elem");

    let output = compile_and_run(
        &dir,
        r#"<?php
$src = [5 => "x", 9 => "y"];
$a = [1];
foreach ($src as $k => $v) { $a[$k] = 2; }
var_dump($a);
echo implode(",", $a), "\n";
echo json_encode($a), "\n";
echo var_export($a, true), "\n";
"#,
    );

    assert_eq!(
        output,
        concat!(
            "array(3) {\n  [0]=>\n  int(1)\n  [5]=>\n  int(2)\n  [9]=>\n  int(2)\n}\n",
            "1,2,2\n",
            "{\"0\":1,\"5\":2,\"9\":2}\n",
            "array (\n  0 => 1,\n  5 => 2,\n  9 => 2,\n)\n",
        )
    );
}
