//! Purpose:
//! Heap-debug regression tests for the managed reference cells that back by-reference
//! `foreach` over indexed and associative array storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`, which also
//!   trips on a double free or a write into a released block.
//! - A reference entry owns one count on its cell and each live local alias owns another, so
//!   an unbalanced bind/release would show up here as a leak or an invalid free rather than as
//!   silent corruption.
//! - The growth fixtures deliberately cross several table reallocations while an alias is live,
//!   which is the window where a stale iterator table pointer used to be reused after free.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A plain by-reference loop binds and releases one cell per entry with no residue.
#[test]
fn test_by_ref_foreach_releases_every_reference_cell() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $v = $v + 1;
}
unset($v);
echo implode(",", $a), "\n";
"#,
    );
    assert_clean(out, "2,3,4\n");
}

/// Growth during the loop relocates the table many times and must not leak or double free.
#[test]
fn test_by_ref_foreach_growth_keeps_the_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
$appended = 0;
foreach ($a as &$v) {
    if ($appended < 40) {
        $appended = $appended + 1;
        $a[] = 7;
    }
    $v = $v + 1;
}
unset($v);
echo count($a), "\n";
"#,
    );
    assert_clean(out, "43\n");
}

/// Leaving the loop through `break` retires the live alias on the cleanup path.
#[test]
fn test_by_ref_foreach_break_releases_the_live_alias() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $v = $v * 3;
    break;
}
unset($v);
echo implode(",", $a), "\n";
"#,
    );
    assert_clean(out, "3,2,3\n");
}

/// An indexed literal may be destroyed at loop exit while its final alias remains usable.
#[test]
fn indexed_literal_final_alias_outlives_the_foreach_source() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
foreach ([1, 2] as &$v) {
}
$v = "after";
echo $v, "\n";
"#,
    );
    assert_clean(out, "after\n");
}

/// A function-result owner may be released at loop exit without invalidating the final alias.
#[test]
fn indexed_function_result_final_alias_outlives_the_foreach_source() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function values(): array {
    return [10, 20];
}
foreach (values() as &$v) {
}
$v = "after";
echo $v, "\n";
"#,
    );
    assert_clean(out, "after\n");
}

/// Function epilogue cleanup releases a still-bound indexed alias without requiring `unset`.
#[test]
fn indexed_final_alias_is_balanced_when_scope_exits_without_unset() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function consume(): void {
    foreach ([1, 2] as &$v) {
    }
    echo $v, "\n";
}
consume();
"#,
    );
    assert_clean(out, "2\n");
}

/// Type-changing writes through map references persist as boxed values on later reads.
#[test]
fn associative_source_readback_uses_runtime_value_type_after_foreach_write() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = ["key" => 1];
foreach ($values as &$v) {
    $v = "changed";
}
unset($v);
echo $values["key"], "\n";
"#,
    );
    assert_clean(out, "changed\n");
}

/// Property metadata widens with its by-reference element storage contract.
#[test]
fn property_source_readback_uses_runtime_value_type_after_foreach_write() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Holder {
    public array $values = ["key" => 1];
}
$holder = new Holder();
foreach ($holder->values as &$v) {
    $v = "changed";
}
unset($v);
echo $holder->values["key"], "\n";
"#,
    );
    assert_clean(out, "changed\n");
}

/// The enclosing array contract widens when a nested element is iterated by reference.
#[test]
fn nested_indexed_source_readback_uses_runtime_value_type_after_foreach_write() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = [[1]];
foreach ($outer[0] as &$v) {
}
$v = "changed";
echo $outer[0][0], "|";
$outer[0][0] = "replaced";
echo $v, "\n";
"#,
    );
    assert_clean(out, "changed|replaced\n");
}

/// Writing a Mixed value into a sparse slot remains safe after nested source promotion.
#[test]
fn indexed_reference_storage_handles_sparse_gap_overwrite() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = [[1]];
foreach ($outer[0] as &$v) {
}
$outer[0][3] = "tail";
$outer[0][1] = "gap";
echo $outer[0][1], "|", $v, "\n";
"#,
    );
    assert_clean(out, "gap|1\n");
}

/// Hash promotion preserves the managed reference set as write-through storage.
#[test]
fn indexed_alias_survives_source_promotion_to_hash() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = [[1]];
foreach ($outer[0] as &$v) {
}
$outer[0]["named"] = "value";
$outer[0][0] = "through-hash";
echo $v, "|", $outer[0]["named"], "\n";
"#,
    );
    assert_clean(out, "through-hash|value\n");
}

/// Repeated iteration joins the reference set already stored in the promoted entry.
#[test]
fn repeated_indexed_foreach_aliases_join_one_reference_set() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1];
foreach ($a as &$v) {
}
foreach ($a as &$w) {
}
$v = "first";
echo $w, "|", $a[0], "|";
$w = "second";
echo $v, "|", $a[0], "\n";
"#,
    );
    assert_clean(out, "first|first|second|second\n");
}

/// An integer-key write after promotion must use the hash entry's live reference set.
#[test]
fn top_level_integer_write_after_foreach_promotion_remains_write_through() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1];
foreach ($a as &$v) {
}
$a[0] = "changed";
echo $v, "|", $a[0], "\n";
unset($v);
"#,
    );
    assert_clean(out, "changed|changed\n");
}

/// Array union after promotion selects associative lowering from the new flow fact.
#[test]
fn array_union_after_foreach_promotion_never_reinterprets_hash_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2];
foreach ($a as &$v) {
}
unset($v);
$combined = $a + [9, 9, 3];
echo implode(",", $combined), "\n";
"#,
    );
    assert_clean(out, "1,2,3\n");
}

/// A by-reference argument resolves the managed entry cell after foreach hash promotion.
#[test]
fn top_level_element_ref_argument_after_foreach_promotion_remains_write_through() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(mixed &$value): void {
    $value = "changed";
}
$a = [1];
foreach ($a as &$v) {
}
replace($a[0]);
echo $v, "|", $a[0], "\n";
unset($v);
"#,
    );
    assert_clean(out, "changed|changed\n");
}

/// A missing hash element passed by reference is materialized and remains addressable.
#[test]
fn missing_element_ref_argument_after_foreach_promotion_is_inserted() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(mixed &$value): void {
    $value = "inserted";
}
$a = [1];
foreach ($a as &$v) {
}
unset($v);
replace($a[4]);
echo $a[4], "|", count($a), "\n";
"#,
    );
    assert_clean(out, "inserted|2\n");
}

/// Taking an element reference separates a shared promoted hash before mutation.
#[test]
fn promoted_hash_element_ref_argument_preserves_array_copy_on_write() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(mixed &$value): void {
    $value = "changed";
}
$a = [1];
foreach ($a as &$v) {
}
unset($v);
$b = $a;
replace($a[0]);
echo $a[0], "|", $b[0], "\n";
"#,
    );
    assert_clean(out, "changed|1\n");
}

/// A direct alias retains an associative entry cell after its parent is destroyed.
#[test]
fn associative_element_reference_outlives_parent_unset() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["k" => 1];
$b =& $a["k"];
unset($a);
$b = 2;
echo $b, "\n";
"#,
    );
    assert_clean(out, "2\n");
}

/// Replacing the parent does not detach a managed associative element alias.
#[test]
fn associative_element_reference_outlives_parent_replacement() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["k" => 1];
$b =& $a["k"];
$a = ["other" => 9];
$b = 2;
echo $b, "|", $a["other"], "\n";
"#,
    );
    assert_clean(out, "2|9\n");
}

/// Array(Mixed) address-of dispatches through hash storage after a dynamic string-key promotion.
#[test]
fn runtime_promoted_array_element_reference_is_managed() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1];
$key = $argc > 100 ? 0 : "named";
$a[$key] = 7;
$b =& $a[$key];
unset($a);
$b = 9;
echo $b, "\n";
"#,
    );
    assert_clean(out, "9\n");
}

/// Returning a promoted local transfers its hash owner past the Array-typed frame slot.
#[test]
fn function_return_preserves_promoted_local_owner() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function promoted(): array {
    $a = [1, 2];
    foreach ($a as &$v) {
    }
    unset($v);
    return $a;
}
$result = promoted();
echo implode(",", $result), "\n";
"#,
    );
    assert_clean(out, "1,2\n");
}

/// A conditional Array/Assoc join converts the unpromoted edge before later mutations.
#[test]
fn conditional_foreach_promotion_join_supports_set_and_append() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1];
if ($argc > 100) {
    foreach ($a as &$v) {
    }
    unset($v);
}
$a[0] = "changed";
$a[] = "tail";
echo implode(",", $a), "\n";
"#,
    );
    assert_clean(out, "changed,tail\n");
}

/// A missing nested foreach source warns but does not materialize the missing key.
#[test]
fn nested_foreach_source_miss_does_not_create_an_entry() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [[1]];
foreach ($a[0][99] as &$v) {
}
echo count($a[0]), "\n";
"#,
    );
    assert_eq!(out.stdout, "1\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("Undefined array key"), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "stderr: {}", out.stderr);
}

/// A runtime-shaped string key is rematerialized for the correct missing-key warning helper.
#[test]
fn nested_foreach_source_mixed_string_miss_reports_the_string_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["ok" => [1]];
$key = $argc > 100 ? 99 : "missing";
foreach ($a[$key] as &$v) {
}
echo count($a), "\n";
"#,
    );
    assert_eq!(out.stdout, "1\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("Undefined array key \"missing\""),
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "stderr: {}",
        out.stderr
    );
}

/// A present non-array nested source reaches normal foreach warning/fallback behavior.
#[test]
fn nested_foreach_non_array_source_uses_the_value_fallback() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["bad" => 1];
foreach ($a["bad"] as &$v) {
}
echo "done\n";
"#,
    );
    assert_eq!(out.stdout, "done\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("foreach() argument must be of type array|object"), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "stderr: {}", out.stderr);
}

/// A Mixed local origin republishes hash growth through the live iterator source.
#[test]
fn mixed_local_source_growth_reloads_the_promoted_hash() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2];
if ($argc > 100) {
    $a = null;
}
foreach ($a as &$v) {
    $a[] = 9;
    break;
}
$v = "changed";
echo $a[0], "|", count($a), "\n";
"#,
    );
    assert_clean(out, "changed|3\n");
}

/// A Mixed parameter keeps its reference origin synchronized when append growth relocates it.
#[test]
fn mixed_parameter_source_growth_reloads_the_promoted_hash() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mutate(mixed &$a): void {
    foreach ($a as &$v) {
        $a[] = 9;
        break;
    }
    $v = "changed";
}
$a = [1, 2];
mutate($a);
echo $a[0], "|", count($a), "\n";
"#,
    );
    assert_clean(out, "changed|3\n");
}

/// Property growth and foreach relocation share the property's managed reference cell.
#[test]
fn property_source_growth_reloads_through_the_synthetic_origin() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bucket {
    public array $items = [1, 2];
}
$b = new Bucket();
foreach ($b->items as &$v) {
    $b->items[] = 9;
    break;
}
$v = "changed";
echo $b->items[0], "|", count($b->items), "\n";
"#,
    );
    assert_clean(out, "changed|3\n");
}

/// Static-property storage is itself the alias origin, so growth publishes its replacement there.
#[test]
fn static_property_source_growth_reloads_through_the_symbol_origin() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class StaticBucket {
    public static array $items = [1, 2];
}
foreach (StaticBucket::$items as &$v) {
    StaticBucket::$items[] = 9;
    break;
}
$v = "changed";
echo StaticBucket::$items[0], "|", count(StaticBucket::$items), "\n";
"#,
    );
    assert_clean(out, "changed|3\n");
}

/// A nested static-property root keeps the selected child cell addressable across growth.
#[test]
fn nested_static_property_source_growth_and_overwrite_remain_write_through() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class StaticOuter {
    public static array $items = [[1, 2]];
}
foreach (StaticOuter::$items[0] as &$v) {
    StaticOuter::$items[0][] = 9;
    break;
}
StaticOuter::$items[0][0] = "direct";
echo $v, "|", count(StaticOuter::$items[0]), "\n";
"#,
    );
    assert_clean(out, "direct|3\n");
}

/// Nested fetch-for-write follows the same entry cell after the inner hash grows.
#[test]
fn nested_source_growth_and_direct_overwrite_remain_write_through() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = [[1, 2]];
foreach ($outer[0] as &$v) {
    $outer[0][] = 9;
    break;
}
$outer[0][0] = "direct";
echo $v, "|", count($outer[0]), "\n";
"#,
    );
    assert_clean(out, "direct|3\n");
}

/// Two nested element hops repeatedly unbox and republish each Mixed parent entry.
#[test]
fn depth_two_nested_source_keeps_every_parent_writeback_addressable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = [[[1, 2]]];
foreach ($outer[0][0] as &$v) {
    $outer[0][0][] = 9;
    break;
}
$outer[0][0][0] = "direct";
echo $v, "|", count($outer[0][0]), "\n";
"#,
    );
    assert_clean(out, "direct|3\n");
}

/// Mixed direct overwrite honors a tag-11 cell instead of replacing the referenced entry.
#[test]
fn mixed_direct_overwrite_preserves_the_foreach_alias() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1];
if ($argc > 100) {
    $a = null;
}
foreach ($a as &$v) {
}
$a[0] = "direct";
echo $v, "|", $a[0], "\n";
"#,
    );
    assert_clean(out, "direct|direct\n");
}

/// Overwriting the referenced key releases the previous cell payload exactly once.
#[test]
fn test_reference_write_through_releases_the_previous_payload() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["k" => "first", "j" => "second"];
foreach ($a as $key => &$v) {
    if ($key === "k") {
        $a["k"] = "replaced";
    }
}
unset($v);
echo $a["k"], "|", $a["j"], "\n";
"#,
    );
    assert_clean(out, "replaced|second\n");
}

/// A copy that shares a reference element releases the shared cell only when both owners drop it.
#[test]
fn test_shared_reference_entry_survives_one_owner_going_away() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
}
$c = $a;
$v = 99;
unset($a);
echo $c[2], "\n";
"#,
    );
    assert_clean(out, "99\n");
}

/// A closure keeps the cell alive after the source array is destroyed, then releases it once.
#[test]
fn test_escaped_reference_cell_is_released_by_its_last_owner() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
$readers = [];
foreach ($a as &$v) {
    $readers[] = function () use (&$v) { return $v; };
    break;
}
unset($v);
unset($a);
$reader = $readers[0];
echo $reader(), "\n";
"#,
    );
    assert_clean(out, "1\n");
}

/// Unsetting the CURRENT key and then forcing relocation must never touch freed storage.
///
/// The successor-key anchor lets iteration continue here, and this fixture pins the ownership
/// half of that: the freed table is never read, the reference cell of the removed entry is
/// released exactly once, and every cell bound during the continued walk is retired. The
/// value-level continuation is asserted in tests/codegen/arrays/foreach_by_ref_growth.rs.
#[test]
fn unsetting_the_current_key_then_relocating_never_touches_freed_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
foreach ($a as $k => &$v) {
    if ($k === "a") {
        unset($a["a"]);
        for ($i = 0; $i < 40; $i = $i + 1) {
            $a["g" . $i] = 0;
        }
    }
}
unset($v);
echo "done\n";
"#,
    );
    assert_clean(out, "done\n");
}

/// Relocating WITHOUT deleting the current key resumes on the successor, like PHP.
#[test]
fn relocating_without_deleting_the_current_key_resumes_on_the_successor() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$seen = "";
foreach ($a as $k => &$v) {
    $seen = $seen . $k;
    if ($k === "a") {
        for ($i = 0; $i < 40; $i = $i + 1) {
            $a["g" . $i] = 0;
        }
    }
    $v = $v + 1;
}
unset($v);
echo substr($seen, 0, 3), "|", $a["a"], "|", $a["b"], "|", $a["c"], "\n";
"#,
    );
    assert_clean(out, "abc|2|3|4\n");
}

/// Owned string anchors survive immediate-successor deletion and are released after resync.
#[test]
fn deleted_immediate_successor_anchor_is_safe_and_balanced_across_growth() {
    let out = compile_and_run_with_heap_debug(
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
echo substr($seen, 0, 3), "\n";
"#,
    );
    assert_clean(out, "acd\n");
}

/// Breaking while string anchors are populated runs `IterEnd` and releases both retains.
#[test]
fn by_ref_foreach_break_releases_owned_string_anchors() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
foreach ($a as $k => &$v) {
    break;
}
unset($v);
unset($a);
echo "done\n";
"#,
    );
    assert_clean(out, "done\n");
}

/// Re-entering one lowered foreach state repeatedly starts from anchors cleared by `IterEnd`.
#[test]
fn repeated_foreach_state_reuse_keeps_string_anchor_ownership_balanced() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
for ($i = 0; $i < 8; $i = $i + 1) {
    foreach ($a as $k => &$v) {
        break;
    }
    unset($v);
}
unset($a);
echo "done\n";
"#,
    );
    assert_clean(out, "done\n");
}

/// Returning from inside the loop runs the loop-frame `IterEnd` cleanup before function exit.
#[test]
fn by_ref_foreach_return_releases_owned_string_anchors() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function first(array $a): int {
    foreach ($a as $k => &$v) {
        return $v;
    }
    return 0;
}
echo first(["a" => 1, "b" => 2, "c" => 3]), "\n";
"#,
    );
    assert_clean(out, "1\n");
}

/// Late-static dispatch widens every reachable redeclaration before selecting its storage.
#[test]
fn late_static_foreach_promotes_a_child_redeclared_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class LateStaticBase {
    public static array $items = [1, 2];

    public static function mutate(): void {
        foreach (static::$items as &$value) {
            static::$items[] = 30;
            break;
        }
        $value = "changed";
    }
}

class LateStaticChild extends LateStaticBase {
    public static array $items = [10, 20];
}

LateStaticChild::mutate();
LateStaticChild::$items[1] = "direct";
echo LateStaticChild::$items[0], "|", LateStaticChild::$items[1], "|",
    count(LateStaticChild::$items), "|", LateStaticBase::$items[0], "\n";
"#,
    );
    assert_clean(out, "changed|direct|3|1\n");
}

/// A managed element argument keeps a cell lease while a later parameter replaces its parent.
#[test]
fn by_ref_element_argument_survives_parent_replacement_inside_the_callee() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function smash(mixed &$value, array &$owner): mixed {
    $owner = [];
    $value = 9;
    return $value;
}

$a = [1];
foreach ($a as &$value) {
}
unset($value);
echo smash($a[0], $a), "|", count($a), "\n";
"#,
    );
    assert_clean(out, "9|0\n");
}

/// The element cell is retained before a later argument expression can destroy its entry owner.
#[test]
fn by_ref_element_argument_survives_later_argument_parent_replacement() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function detach(array &$owner): array {
    $owner = [];
    return [];
}

function write_after_detach(mixed &$value, array $unused): mixed {
    $value = 9;
    return $value;
}

$a = [1];
foreach ($a as &$value) {
}
unset($value);
echo write_after_detach($a[0], detach($a)), "|", count($a), "\n";
"#,
    );
    assert_clean(out, "9|0\n");
}

/// Constructor lowering acquires a managed element before evaluating later arguments.
#[test]
fn constructor_ref_argument_survives_later_argument_parent_replacement() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function detach_for_constructor(array &$owner): array {
    $owner = [];
    return [];
}

class RefArgumentWriter {
    public function __construct(mixed &$value, array $unused) {
        $value = 9;
        echo $value, "|";
    }
}

$a = [1];
foreach ($a as &$value) {
}
unset($value);
new RefArgumentWriter($a[0], detach_for_constructor($a));
echo count($a), "\n";
"#,
    );
    assert_clean(out, "9|0\n");
}

/// Direct first-class callable calls keep the first managed element alive through later effects.
#[test]
fn closure_ref_argument_survives_later_argument_parent_replacement() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function detach_for_closure(array &$owner): array {
    $owner = [];
    return [];
}

class ClosureRefArgumentWriter {
    public function write(mixed &$value, array $unused): mixed {
        $value = 9;
        return $value;
    }
}

$instance = new ClosureRefArgumentWriter();
$writer = $instance->write(...);
$a = [1];
foreach ($a as &$value) {
}
unset($value);
echo $writer($a[0], detach_for_closure($a)), "|", count($a), "\n";
"#,
    );
    assert_clean(out, "9|0\n");
}

/// Static and nested element references republish COW splits through the original static slot.
#[test]
fn static_and_nested_element_reference_assignments_preserve_cow() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class StaticNestedReferences {
    public static array $items = [[1], [2]];
}

$copy = StaticNestedReferences::$items;
$inner =& StaticNestedReferences::$items[0];
$inner[] = 3;
$value =& StaticNestedReferences::$items[1][0];
$value = 9;
echo StaticNestedReferences::$items[0][0], ",", StaticNestedReferences::$items[0][1],
    "|", StaticNestedReferences::$items[1][0], "|", $copy[0][0], ",", $copy[1][0], "\n";
"#,
    );
    assert_clean(out, "1,3|9|1,2\n");
}

/// A nested static element passed by reference stays connected while COW isolates an earlier copy.
#[test]
fn nested_static_element_by_ref_argument_preserves_cow() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace_static_nested(mixed &$value): void {
    $value = "changed";
}

class StaticNestedArgument {
    public static array $items = [[1]];
}

$copy = StaticNestedArgument::$items;
replace_static_nested(StaticNestedArgument::$items[0][0]);
echo StaticNestedArgument::$items[0][0], "|", $copy[0][0], "\n";
"#,
    );
    assert_clean(out, "changed|1\n");
}

/// A typed reference parameter keeps using a concrete indexed element address.
#[test]
fn typed_ref_argument_uses_a_concrete_indexed_entry() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function increment_entry(int &$value): void {
    $value = $value + 1;
}

$a = [1];
increment_entry($a[0]);
echo $a[0], "\n";
"#,
    );
    assert_clean(out, "2\n");
}

/// Mixed parameters share the canonical managed cell for an associative entry.
#[test]
fn mixed_ref_parameters_share_a_managed_associative_entry() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mutate_and_read(mixed &$value, mixed &$alias): void {
    $value = 2;
    echo $alias, "|";
}

$a = ["k" => 1];
mutate_and_read($a["k"], $a["k"]);
echo $a["k"], "\n";
"#,
    );
    assert_clean(out, "2|2\n");
}

/// An untyped by-reference parameter keeps Mixed storage when its source is associative.
#[test]
fn untyped_ref_parameter_uses_a_managed_associative_entry() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace_untyped(&$value): void {
    $value = "changed";
}

$a = ["k" => 1];
replace_untyped($a["k"]);
echo $a["k"], "\n";
"#,
    );
    assert_clean(out, "changed\n");
}

/// An earlier scalar call cannot specialize an untyped ref parameter away from Mixed storage.
#[test]
fn untyped_ref_parameter_stays_mixed_across_scalar_and_associative_calls() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace_reused(&$value): void {
    $value = 20;
}

$scalar = 1;
replace_reused($scalar);
$a = ["k" => 2];
replace_reused($a["k"]);
echo $scalar, "|", $a["k"], "\n";
"#,
    );
    assert_clean(out, "20|20\n");
}

/// Method, static-method, constructor, and closure untyped refs all use canonical Mixed cells.
#[test]
fn untyped_callable_surfaces_write_through_associative_entries() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class UntypedReferenceSurfaces {
    public function __construct(&$value) {
        $value = "construct";
    }

    public function replace(&$value): void {
        $value = "method";
    }

    public static function replaceStatic(&$value): void {
        $value = "static";
    }
}

$a = ["construct" => 1, "method" => 2, "static" => 3, "closure" => 4];
$instance = new UntypedReferenceSurfaces($a["construct"]);
$instance->replace($a["method"]);
UntypedReferenceSurfaces::replaceStatic($a["static"]);
$replace = function (&$value): void {
    $value = "closure";
};
$replace($a["closure"]);
echo implode("|", $a), "\n";
"#,
    );
    assert_clean(out, "construct|method|static|closure\n");
}

/// A directly resolved function FCC uses the normal live-cell argument path.
#[test]
fn direct_function_fcc_preserves_a_mixed_associative_element_reference() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace_entry(mixed &$value): void {
    $value = "changed";
}

$replace = replace_entry(...);
$a = ["k" => 1];
$replace($a["k"]);
echo $a["k"], "\n";
"#,
    );
    assert_clean(out, "changed\n");
}

/// A directly resolved static-method FCC also bypasses descriptor argument serialization.
#[test]
fn direct_static_method_fcc_preserves_a_mixed_associative_element_reference() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class DirectStaticReference {
    public static function replace(mixed &$value): void {
        $value = "changed";
    }
}

$replace = DirectStaticReference::replace(...);
$a = ["k" => 1];
$replace($a["k"]);
echo $a["k"], "\n";
"#,
    );
    assert_clean(out, "changed\n");
}

/// Widened inferred properties convert concrete associative payloads before their first store.
#[test]
fn inferred_property_initializers_use_mixed_hash_payloads_after_reference_widening() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class InferredReferenceContainers {
    public $instance = ["k" => 1];
    public static $shared = ["k" => 2];
}

$holder = new InferredReferenceContainers();
foreach ($holder->instance as &$instanceValue) {
}
$instanceValue = "instance";
foreach (InferredReferenceContainers::$shared as &$staticValue) {
}
$staticValue = "static";
echo $holder->instance["k"], "|", InferredReferenceContainers::$shared["k"], "\n";
"#,
    );
    assert_clean(out, "instance|static\n");
}

/// Named constructor arguments retire managed cells in source publication order, not ABI order.
#[test]
fn reversed_named_constructor_ref_cells_unwind_in_publication_order() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ConstructorLeaseBomb {
    public function __destruct() {
        echo "destroy|";
        throw new Exception("cleanup");
    }
}

function detach_constructor_owners(array &$a, array &$b): array {
    $a = [];
    $b = [];
    return [];
}

class NamedConstructorSink {
    public function __construct(mixed &$first, mixed &$second, array $unused) {
        echo "call|";
    }
}

$a = ["k" => new ConstructorLeaseBomb()];
$b = ["k" => 2];
try {
    new NamedConstructorSink(
        second: $b["k"],
        first: $a["k"],
        unused: detach_constructor_owners($a, $b)
    );
} catch (Exception $e) {
    echo "caught\n";
}
"#,
    );
    assert_clean(out, "call|destroy|caught\n");
}

/// Named first-class callable arguments use the same source-order managed-cell cleanup.
#[test]
fn reversed_named_callable_ref_cells_unwind_in_publication_order() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CallableLeaseBomb {
    public function __destruct() {
        echo "destroy|";
        throw new Exception("cleanup");
    }
}

function detach_callable_owners(array &$a, array &$b): array {
    $a = [];
    $b = [];
    return [];
}

class NamedCallableSink {
    public function write(mixed &$first, mixed &$second, array $unused): void {
        echo "call|";
    }
}

$sink = new NamedCallableSink();
$callable = $sink->write(...);
$a = ["k" => new CallableLeaseBomb()];
$b = ["k" => 2];
try {
    $callable(
        second: $b["k"],
        first: $a["k"],
        unused: detach_callable_owners($a, $b)
    );
} catch (Exception $e) {
    echo "caught\n";
}
"#,
    );
    assert_clean(out, "call|destroy|caught\n");
}

/// A ref-cell store publishes its replacement before retiring the old object exactly once.
#[test]
fn ref_bound_mixed_overwrite_has_one_publish_then_release_owner() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class RefOverwriteValue {
    public function __construct(public int $id) {}
    public function __destruct() { echo "drop", $this->id, "|"; }
}

function replace_ref_value(mixed &$value): void {
    $value = new RefOverwriteValue(2);
}

$values = ["k" => new RefOverwriteValue(1)];
$alias =& $values["k"];
replace_ref_value($alias);
echo $alias->id, "|";
unset($values, $alias);
echo "done\n";
"#,
    );
    assert_clean(out, "drop1|2|drop2|done\n");
}

/// A copied alias keeps the managed entry's boxed-cell provenance for a later Mixed ref call.
#[test]
fn copied_managed_entry_alias_can_be_replaced_through_mixed_ref_parameter() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CopiedManagedAliasValue {}
function replace_copied_alias(mixed &$value): void { $value = "changed"; }

$values = ["k" => new CopiedManagedAliasValue()];
$alias =& $values["k"];
$copy =& $alias;
replace_copied_alias($copy);
echo $values["k"], "|", $alias, "|", $copy, "\n";
unset($values, $alias, $copy);
$conditionalValues = ["k" => new CopiedManagedAliasValue()];
$conditionalAlias =& $conditionalValues["k"];
if ($argc >= 0) {
    replace_copied_alias($conditionalAlias);
}
echo $conditionalValues["k"], "\n";
unset($conditionalValues, $conditionalAlias);
"#,
    );
    assert_clean(out, "changed|changed|changed\nchanged\n");
}

/// An early borrowed ref-cell argument stays alive while a later argument replaces its alias.
#[test]
fn borrowed_ref_cell_call_argument_is_pinned_across_later_reassignment() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show_ref_snapshot(mixed $first, mixed $unused): void {
    echo $first[0], "|";
}

function replace_ref_snapshot(mixed &$value): mixed {
    $value = ["new"];
    return null;
}

$values = [["old"]];
foreach ($values as &$alias) {}
show_ref_snapshot($alias, replace_ref_snapshot($alias));
echo $alias[0], "\n";
unset($alias);
"#,
    );
    assert_clean(out, "old|new\n");
}

/// Callable return metadata keeps untyped by-reference storage canonical as Mixed.
#[test]
fn returned_function_and_method_callables_write_through_managed_entries() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function returned_ref_target(&$value): void { $value = "function"; }
function make_ref_target(): callable { return returned_ref_target(...); }

class ReturnedRefFactory {
    public function write(&$value): void { $value = "method"; }
    public function make(): callable { return $this->write(...); }
}

$first = ["k" => 1];
$firstAlias =& $first["k"];
$function = make_ref_target();
$function($firstAlias);
echo $first["k"], "|";

$second = ["k" => 2];
$secondAlias =& $second["k"];
$method = (new ReturnedRefFactory())->make();
$method($secondAlias);
echo $second["k"], "\n";
unset($firstAlias, $secondAlias);
"#,
    );
    assert_clean(out, "function|method\n");
}

/// Direct by-reference calls republish nested declared property and static-property roots.
#[test]
fn nested_property_and_static_array_ref_arguments_write_through() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace_nested_ref(mixed &$value): void { $value = "changed"; }

class NestedRefRoots {
    public array $items = [[1]];
    public static array $staticItems = [[2]];
}

$holder = new NestedRefRoots();
replace_nested_ref($holder->items[0][0]);
replace_nested_ref(NestedRefRoots::$staticItems[0][0]);
echo $holder->items[0][0], "|", NestedRefRoots::$staticItems[0][0], "\n";
"#,
    );
    assert_clean(out, "changed|changed\n");
}

/// A capture-free immediate closure uses direct reference argument lowering.
#[test]
fn immediate_closure_writes_through_an_associative_element() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$items = ["k" => 1];
(function (mixed &$value): void { $value = "changed"; })($items["k"]);
echo $items["k"], "\n";
"#,
    );
    assert_clean(out, "changed\n");
}

/// A direct closure result is staged before a managed argument lease can throw in cleanup.
#[test]
fn direct_closure_string_result_survives_throwing_ref_lease_cleanup() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ClosureLeaseBomb {
    public function __destruct() {
        echo "drop|";
        throw new Exception("lease cleanup");
    }
}

function detach_closure_parent(array &$items): mixed {
    $items = [];
    return null;
}

$callback = function (mixed &$value, mixed $unused): string {
    echo "call|";
    return str_repeat("r", 6);
};
$items = ["k" => new ClosureLeaseBomb()];
try {
    echo $callback($items["k"], detach_closure_parent($items));
} catch (Exception $error) {
    echo "caught\n";
}
"#,
    );
    assert_clean(out, "call|drop|caught\n");
}

/// Assignment-expression callable targets keep the same direct ref-argument lowering.
#[test]
fn assigned_closure_and_function_fcc_write_through_associative_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function assign_fcc_target(mixed &$value): void { $value = "fcc"; }

$items = ["closure" => 1, "fcc" => 2];
($closure = function (mixed &$value): void { $value = "closure"; })($items["closure"]);
($target = assign_fcc_target(...))($items["fcc"]);
echo $items["closure"], "|", $items["fcc"], "\n";
"#,
    );
    assert_clean(out, "closure|fcc\n");
}

/// A by-value closure capture records plain storage at creation, not a later alias state.
#[test]
fn plain_closure_capture_stays_direct_after_the_source_is_later_aliased() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$state = 1;
$callback = function (mixed &$value) use ($state): void { $value = $state + 1; };
$state_alias =& $state;
$items = ["k" => 0];
$callback($items["k"]);
echo $items["k"], "\n";
"#,
    );
    assert_clean(out, "2\n");
}

/// Ref-cell replacement roots the incoming owner while the old pointee destructor unwinds.
#[test]
fn throwing_old_pointee_destructor_does_not_leak_the_new_ref_cell_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class OldRefValue {
    public function __destruct() { echo "old|"; throw new Exception("old"); }
}
class NewRefValue {
    public function __destruct() { echo "new|"; }
}
function replace_ref_value(mixed &$value): void { $value = new NewRefValue(); }

$items = ["k" => new OldRefValue()];
try {
    replace_ref_value($items["k"]);
} catch (Exception $error) {
    echo "caught|";
}
unset($items);
echo "after\n";
"#,
    );
    assert_clean(out, "old|caught|new|after\n");
}

/// Promoted typed reference properties preserve the uninitialized marker in their cell payload.
#[test]
fn promoted_reference_properties_keep_uninitialized_reads_and_probes_php_compatible() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class UninitializedRefProperties {
    public array $items;
    public static array $staticItems;
}
function expose_uninitialized(mixed &$value): void {}

$holder = new UninitializedRefProperties();
if ($argc > 100) {
    expose_uninitialized($holder->items[0]);
    expose_uninitialized(UninitializedRefProperties::$staticItems[0]);
}
var_dump(isset($holder->items));
var_dump(isset(UninitializedRefProperties::$staticItems));
$clone = clone $holder;
var_dump(isset($clone->items));
try { $value = $holder->items; } catch (Error $error) { echo "instance|"; }
try { $value = $clone->items; } catch (Error $error) { echo "clone|"; }
try { $value = UninitializedRefProperties::$staticItems; } catch (Error $error) { echo "static\n"; }
"#,
    );
    assert_clean(
        out,
        "bool(false)\nbool(false)\nbool(false)\ninstance|clone|static\n",
    );
}

/// Reference-property replacement publishes first, then retires a hash-backed old array.
#[test]
fn reference_property_replacement_is_reentrant_and_destructor_exception_safe() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ReentrantOldPropertyElement {
    public function __destruct() {
        global $holder;
        echo $holder->items[0], "|";
        $holder->items = ["reentrant"];
        throw new Exception("old property value");
    }
}
class ReentrantReferencePropertyHolder {
    public array $items = [];
}
function expose_reentrant_property(mixed &$value): void {}

$holder = new ReentrantReferencePropertyHolder();
$holder->items = [new ReentrantOldPropertyElement(), "tail"];
if ($argc > 100) {
    expose_reentrant_property($holder->items[0]);
}
krsort($holder->items);
try {
    $holder->items = ["new"];
} catch (Exception $error) {
    echo "caught|";
}
echo $holder->items[0], "\n";
"#,
    );
    assert_clean(out, "new|caught|reentrant\n");
}
