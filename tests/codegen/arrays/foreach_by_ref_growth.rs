//! Purpose:
//! Regression tests for by-reference `foreach` over associative storage while the source
//! array grows, is copied, is overwritten through the referenced key, or is iterated twice.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - A hash entry that joins a PHP reference set carries runtime value tag 11 and a managed
//!   reference cell in `value_lo`, so the alias is a real heap allocation rather than an
//!   interior pointer into the table. Growth relocates the table but never the cell.
//! - `IterNext` reloads the live container from the iterator's origin local and validates the
//!   cursor from owned successor keys. This covers both table relocation and physical tombstone
//!   reuse, which can preserve the table pointer while changing a slot's logical identity.
//! - Expected values follow reference PHP semantics for by-reference `foreach`: appended
//!   elements ARE visited, `$a[$k] = x` on a referenced entry writes through, and copying an
//!   array that holds a reference element keeps that element shared.

use crate::support::*;

/// Replacing ordinary heap-backed values must return to the normal entry write path after release.
#[test]
fn overwriting_ordinary_heap_backed_hash_values_does_not_enter_reference_write_through() {
    let out = compile_and_run(
        r#"<?php
$a = ["text" => "old", "nested" => [1, 2]];
$a["text"] = "new";
$a["nested"] = [3, 4];
echo $a["text"], "|", implode(",", $a["nested"]);
"#,
    );
    assert_eq!(out, "new|3,4");
}

/// Appending inside a by-reference `foreach` forces several hash grows while the loop is live.
///
/// Each grow allocates a new table and frees the old one, so a stale iterator table pointer or a
/// stale slot cursor would read freed storage. The mutation through `$v` happens AFTER the append
/// in the same iteration, which is exactly the window where the alias must still be valid.
#[test]
fn test_by_ref_foreach_survives_repeated_hash_growth_while_appending() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$appended = 0;
foreach ($a as &$v) {
    if ($appended < 40) {
        $appended = $appended + 1;
        $a[] = 100;
    }
    $v = $v + 1;
}
unset($v);
echo count($a), "|", $a[0], "|", $a[1], "|", $a[2], "|", $a[42];
"#,
    );
    assert_eq!(out, "43|2|3|4|101");
}

/// A string-keyed append inside the loop grows the table through the persisted-key path.
#[test]
fn test_by_ref_foreach_survives_growth_from_string_keyed_writes() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2];
$added = 0;
foreach ($a as &$v) {
    if ($added < 24) {
        $a["k" . $added] = 0;
        $added = $added + 1;
    }
    $v = $v + 5;
}
unset($v);
echo count($a), "|", $a["a"], "|", $a["b"], "|", $a["k0"];
"#,
    );
    assert_eq!(out, "26|6|7|5");
}

/// A closure capturing `&$v` keeps a real managed cell, so it survives growth and `unset($a)`.
///
/// Before the reference-cell representation the capture retained nothing: the local held an
/// interior address inside the table allocation, `__rt_reference_cell_owner` answered zero, and
/// the closure silently kept a pointer into storage the next grow returned to the heap.
#[test]
fn test_closure_capture_of_by_ref_value_outlives_growth_and_source_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$readers = [];
foreach ($a as &$v) {
    $readers[] = function () use (&$v) { return $v; };
    break;
}
unset($v);
for ($i = 0; $i < 40; $i = $i + 1) {
    $a[] = $i;
}
$reader = $readers[0];
echo $reader();
unset($a);
echo "|", $reader();
"#,
    );
    assert_eq!(out, "1|1");
}

/// Overwriting the currently referenced key writes THROUGH the reference instead of detaching it.
#[test]
fn test_overwriting_referenced_key_writes_through_the_reference() {
    let out = compile_and_run(
        r#"<?php
$a = ["k" => 1, "j" => 2];
$seen = "";
foreach ($a as $key => &$v) {
    if ($key === "k") {
        $a["k"] = 5;
        $seen = $seen . $v;
    }
}
unset($v);
echo $seen, "|", $a["k"], "|", $a["j"];
"#,
    );
    assert_eq!(out, "5|5|2");
}

/// Iterating the same array by reference twice must not restamp or corrupt the reference entries.
///
/// The widening helper skips an entry that already carries the reference tag, so the second loop
/// reuses the existing cells instead of wrapping them a second time.
#[test]
fn test_repeated_by_ref_foreach_does_not_restamp_reference_entries() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $v = $v * 2;
}
unset($v);
foreach ($a as &$w) {
    $w = $w + 1;
}
unset($w);
echo implode(",", $a);
"#,
    );
    assert_eq!(out, "3,5,7");
}

/// Copying an array that still holds a reference element keeps that element shared, like PHP.
#[test]
fn test_array_copy_preserves_shared_reference_identity() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
}
$c = $a;
$v = 99;
echo $a[2], "|", $c[2], "|", $a[0], "|", $c[0];
"#,
    );
    assert_eq!(out, "99|99|1|1");
}

/// Unsetting a different key while a reference alias is live keeps the alias readable.
#[test]
fn test_unsetting_another_key_keeps_the_live_reference_readable() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
foreach ($a as $key => &$v) {
    if ($key === "a") {
        unset($a["c"]);
    }
}
unset($v);
echo count($a), "|", $a["a"], "|", $a["b"];
"#,
    );
    assert_eq!(out, "2|1|2");
}

/// A stable-table resume follows the tombstone's preserved next link instead of yielding it.
#[test]
fn deleting_the_immediate_successor_without_growth_skips_the_tombstone() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$seen = "";
foreach ($a as $k => &$v) {
    $seen = $seen . $k;
    if ($k === "a") {
        unset($a["b"]);
    }
}
unset($v);
echo $seen;
"#,
    );
    assert_eq!(out, "ac");
}

/// Reusing the deleted successor's bucket must not make the replacement appear out of order.
#[test]
fn reusing_the_immediate_successor_tombstone_validates_key_identity() {
    let out = compile_and_run(
        r#"<?php
$a = [0 => 1, 1 => 2, 2 => 3];
$seen = [];
foreach ($a as $k => &$v) {
    $seen[] = $k;
    if ($k === 0) {
        unset($a[1]);
        $a[9] = 10;
    }
}
unset($v);
echo implode(",", $seen);
"#,
    );
    assert_eq!(out, "0,2,9");
}

/// Deleting the CURRENT key and then relocating the table still resumes on the successor.
///
/// This is the case the yielded-key anchor could not recover: the key the resync would have
/// probed for is the one the loop body just deleted. The iterator anchors on the SUCCESSOR key
/// instead, which the deletion leaves untouched, so iteration continues exactly where PHP does.
#[test]
fn deleting_the_current_key_then_relocating_resumes_on_the_successor() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$seen = "";
foreach ($a as $k => &$v) {
    $seen = $seen . $k;
    if ($k === "a") {
        unset($a["a"]);
        for ($i = 0; $i < 40; $i = $i + 1) {
            $a["g" . $i] = 0;
        }
    }
}
unset($v);
echo substr($seen, 0, 3), "|", count($a), "|", strlen($seen);
"#,
    );
    assert_eq!(out, "abc|42|113");
}

/// Deleting a key that is NOT current leaves the anchor and the walk order alone.
#[test]
fn deleting_a_later_key_then_relocating_skips_only_that_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3, "d" => 4];
$seen = "";
foreach ($a as $k => &$v) {
    $seen = $seen . $k;
    if ($k === "a") {
        unset($a["c"]);
        for ($i = 0; $i < 40; $i = $i + 1) {
            $a["g" . $i] = 0;
        }
    }
}
unset($v);
echo substr($seen, 0, 3), "|", count($a);
"#,
    );
    assert_eq!(out, "abd|43");
}

/// Deleting the immediate successor uses the owned fallback anchor after the table grows.
#[test]
fn deleting_the_immediate_successor_then_relocating_resumes_after_the_tombstone() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3, "d" => 4];
$seen = "";
foreach ($a as $k => &$v) {
    $seen = $seen . $k;
    if ($k === "a") {
        unset($a["b"]);
        for ($i = 0; $i < 40; $i = $i + 1) {
            $a["g" . $i] = 0;
        }
    }
}
unset($v);
echo substr($seen, 0, 3), "|", count($a);
"#,
    );
    assert_eq!(out, "acd|43");
}

/// Appending after the walk has already yielded the tail is visited, as PHP does (bug #67633's
/// relative; php-src `foreach_007.phpt`).
///
/// The tail entry returns the post-last cursor with no successor to anchor on, so the walk used
/// to stop before the freshly appended element. It now remembers the tail key and resumes from
/// that key's live insertion-order successor.
#[test]
fn by_ref_foreach_visits_an_element_appended_after_the_tail() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
foreach ($a as &$v) {
    echo "$v\n";
    $a[1] = 2;
}
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// The key binding sees the appended element too.
#[test]
fn by_ref_foreach_key_binding_sees_an_element_appended_after_the_tail() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
foreach ($a as $k => &$v) {
    echo "$k=$v\n";
    $a[1] = 2;
}
"#,
    );
    assert_eq!(out, "0=1\n1=2\n");
}

/// An associative source resumes from the tail key's successor the same way.
#[test]
fn by_ref_foreach_visits_an_associative_element_appended_after_the_tail() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1];
foreach ($a as &$v) {
    echo "$v\n";
    $a["y"] = 2;
}
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// Repeated appends that force several hash grows are all visited, in insertion order.
#[test]
fn by_ref_foreach_visits_every_element_appended_across_repeated_growth() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
foreach ($a as &$v) {
    if (count($a) < 10) {
        $a[] = $v + 1;
    }
}
echo implode(",", $a);
"#,
    );
    assert_eq!(out, "1,2,3,4,5,6,7,8,9,10");
}

/// An associative source with string keys visits every element appended across growth.
#[test]
fn by_ref_foreach_visits_appended_associative_elements_across_growth() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1];
foreach ($a as $k => &$v) {
    echo "$k=$v\n";
    if (count($a) < 6) {
        $a["k" . count($a)] = $v + 1;
    }
}
"#,
    );
    assert_eq!(out, "x=1\nk1=2\nk2=3\nk3=4\nk4=5\nk5=6\n");
}

/// Emits one fixture's assembly for `target` through the CLI and returns its text.
fn emit_assembly_for_target(target: &str, source: &str) -> String {
    let dir = make_cli_test_dir("elephc_foreach_by_ref_target");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write fixture");
    let output = elephc_cli_command(&dir)
        .args(["--emit-asm", "--target", target])
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-asm");
    assert!(
        output.status.success(),
        "{target}: emitting assembly failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s")).expect("read assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Pins the tail-resync branch's per-helper argument registers on both targets.
///
/// `__rt_heap_kind` reads its pointer in `x0`/`rax`, while `__rt_hash_iter_resync_successor`
/// reads the table in `x0`/`rdi`. Loading the source into the wrong register silently disabled
/// the whole tail resume on x86_64 while every aarch64 executable test still passed, so the
/// caller-side sequence is pinned here for both targets.
#[test]
fn tail_resync_branch_passes_each_helper_its_own_argument_register() {
    let source = "<?php\n$a = [1];\nforeach ($a as &$v) { echo \"$v\\n\"; $a[1] = 2; }\n";
    for (target, kind_reg, resync_reg) in [
        ("macos-aarch64", "x0", "x0"),
        ("linux-x86_64", "rax", "rdi"),
    ] {
        let asm = emit_assembly_for_target(target, source);
        let lines: Vec<&str> = asm.lines().collect();
        let label_index = lines
            .iter()
            .position(|line| {
                line.trim_end().ends_with(':') && line.contains("iter_anchor_validate_tail")
            })
            .unwrap_or_else(|| panic!("{target}: tail label absent"));
        let tail = &lines[label_index..];
        let kind_index = tail
            .iter()
            .position(|line| line.contains("__rt_heap_kind"))
            .unwrap_or_else(|| panic!("{target}: heap-kind probe absent"));
        assert!(
            tail[kind_index - 1].contains(&format!("{kind_reg},")),
            "{target}: heap kind needs the source in {kind_reg}, got: {}",
            tail[kind_index - 1]
        );
        let resync_index = tail
            .iter()
            .position(|line| line.contains("__rt_hash_iter_resync_successor"))
            .unwrap_or_else(|| panic!("{target}: successor resync absent"));
        assert!(
            tail[kind_index..resync_index]
                .iter()
                .any(|line| line.contains(&format!("{resync_reg},"))),
            "{target}: successor resync needs the table in {resync_reg}"
        );
    }
}
