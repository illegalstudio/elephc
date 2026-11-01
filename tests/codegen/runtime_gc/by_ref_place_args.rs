//! Purpose:
//! Heap-debug coverage for calls whose by-reference argument needs a caller-visible temporary,
//! including property/static/container places and gradual scalar locals. Those calls lower as
//! `$tmp = <place>; f($tmp, ...); <place> = $tmp;`, which adds a synthetic local, a
//! copy-on-write separation, and a write-back that releases the property's previous occupant.
//! Every one of those steps has to stay balanced.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`, so an
//!   unreleased separated copy or an unreleased synthetic local shows up as a leak. That holds
//!   for the static-property fixtures too: static storage is released when the process exits,
//!   so a value written back into a static property ends clean like any other.
//! - The aliased fixtures also assert PHP's copy-on-write result, because an over-release of
//!   the pre-sort array would surface as a use-after-free in the alias rather than as a leak.
//! - Expected stdout values are real `LC_ALL=C php` 8.4 output for the same fixtures.

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

/// Mutating an instance property leaves no live heap blocks: each separated copy is written
/// back into the property, the property's previous occupant is released exactly once, and the
/// synthetic local that carried the array is released at scope exit.
///
/// The fixture deliberately avoids `usort()` with a closure comparator: that combination
/// leaks eight blocks on a plain local too, so it would assert a pre-existing defect rather
/// than this lowering's ownership balance.
#[test]
fn test_property_mutators_leave_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
array_push($b->items, 9);
array_unshift($b->items, 0);
sort($b->items);
array_pop($b->items);
echo implode(",", $b->items);
"#,
    );
    assert_clean(out, "0,1,2,3");
}

/// Inferred native property layouts use copy/write-back instead of a boxed live reference cell.
#[test]
fn test_array_push_inferred_properties_use_heap_clean_writeback() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class IntPushBox { public $items = [1]; }
$ints = new IntPushBox();
$intCount = array_push($ints->items, 2);
class StringPushBox { public $items = ["a"]; }
$strings = new StringPushBox();
$stringCount = array_push($strings->items, "b");
class StaticPushBox { public static $items = [1]; }
$staticCount = array_push(StaticPushBox::$items, 2);
class AssocPushBox { public $items = ["a" => 1]; }
$assoc = new AssocPushBox();
$assocCount = array_push($assoc->items, 2);
class StaticAssocPushBox { public static $items = ["a" => 1]; }
$staticAssocCount = array_push(StaticAssocPushBox::$items, 2);
class LateAssocPushBox { public $items; }
$lateAssoc = new LateAssocPushBox();
$lateAssoc->items = ["a" => 1];
$lateAssocCount = array_push($lateAssoc->items, 2);
class NestedAssocPushBox { public $items = ["a" => [1]]; }
$nestedAssoc = new NestedAssocPushBox();
$nestedCount = array_push($nestedAssoc->items["a"], 2);
echo $intCount, "|", json_encode($ints->items), "\n";
echo $stringCount, "|", json_encode($strings->items), "\n";
echo $staticCount, "|", json_encode(StaticPushBox::$items), "\n";
echo $assocCount, "|", json_encode($assoc->items), "\n";
echo $staticAssocCount, "|", json_encode(StaticAssocPushBox::$items), "\n";
echo $lateAssocCount, "|", json_encode($lateAssoc->items), "\n";
echo $nestedCount, "|", json_encode($nestedAssoc->items), "\n";
"#,
    );
    assert_clean(
        out,
        "2|[1,2]\n2|[\"a\",\"b\"]\n2|[1,2]\n2|{\"a\":1,\"0\":2}\n2|{\"a\":1,\"0\":2}\n2|{\"a\":1,\"0\":2}\n2|{\"a\":[1,2]}\n",
    );
}

/// The aliased property case: the pre-sort array is still owned by `$copy`, so the
/// write-back's release of the property's previous occupant must not free it.
#[test]
fn test_sort_on_aliased_instance_property_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
$copy = $b->items;
sort($b->items);
echo implode(",", $b->items), "|", implode(",", $copy);
"#,
    );
    assert_clean(out, "1,2,3|3,1,2");
}

/// A static-property load is a borrowed pointer, so the synthetic local has to retain it
/// before the sort separates a copy; otherwise the write-back frees the aliased original.
/// The sorted array stays live in the property until static storage is released at exit, so a
/// write-back that left the property pointing at freed storage shows up as a double release.
#[test]
fn test_sort_on_aliased_static_property_keeps_the_sorted_array_live() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B { public static $items = [3,1,2]; }
$copy = B::$items;
sort(B::$items);
echo implode(",", B::$items), "|", implode(",", $copy);
"#,
    );
    assert_clean(out, "1,2,3|3,1,2");
}

/// A nested container element receiver, where the write-back goes through `hash_set` rather
/// than a property store.
#[test]
fn test_sort_on_aliased_array_element_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = ["k" => [3,1,2]];
$copy = $m["k"];
sort($m["k"]);
echo implode(",", $m["k"]), "|", implode(",", $copy);
"#,
    );
    assert_clean(out, "1,2,3|3,1,2");
}

/// Repeated mutation of one property inside a loop: each iteration reads, separates, and
/// writes back, so an unbalanced release or retain accumulates instead of staying flat.
#[test]
fn test_repeated_property_mutation_in_loop_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
for ($i = 0; $i < 5; $i++) {
    array_push($b->items, $i);
    sort($b->items);
}
echo implode(",", $b->items);
"#,
    );
    assert_clean(out, "0,1,1,2,2,3,3,4");
}

/// User functions, instance methods, and static methods share the same non-local l-value
/// semantics. Each call must write the separated array back while preserving pre-call aliases;
/// a side-effectful element index must also be evaluated exactly once.
#[test]
fn test_declared_calls_write_back_non_local_array_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function append_value(array &$values, int $value): void { $values[] = $value; }
function next_key(): string { echo "K"; return "slot"; }

class Mutator {
    public function append(array &$values, int $value): void { $values[] = $value; }
    public static function appendStatic(array &$values, int $value): void { $values[] = $value; }
}
class Box {
    public array $items = [1];
    public static array $shared = [4];
}

$box = new Box();
$mutator = new Mutator();
$itemsCopy = $box->items;
$sharedCopy = Box::$shared;
$map = ["slot" => [7]];
append_value($box->items, 2);
$mutator->append($box->items, 3);
Mutator::appendStatic(Box::$shared, 5);
$mutator->append($map[next_key()], 8);
echo "|", implode(",", $box->items), "|", implode(",", $itemsCopy);
echo "|", implode(",", Box::$shared), "|", implode(",", $sharedCopy);
echo "|", implode(",", $map["slot"]);
"#,
    );
    assert_clean(out, "K|1,2,3|1|4,5|4|7,8");
}

/// Declared reference parameters adapt gradual caller storage in both directions: a boxed value
/// is checked before a concrete parameter receives its address, while a nullable parameter can
/// widen a previously concrete caller variable and publish null back into its boxed frame slot.
#[test]
fn test_declared_scalar_refs_adapt_gradual_caller_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function runtime_value(): mixed { return 4; }
class RefMutator {
    public static function increment(int &$value): void { $value++; }
    public function clear(?int &$value): void { $value = null; }
}

$value = runtime_value();
RefMutator::increment($value);
$number = 7;
$mutator = new RefMutator();
$mutator->clear($number);
echo $value, "|", $number === null ? "null" : "value";
"#,
    );
    assert_clean(out, "5|null");
}

/// An uninitialized local used as an output destination starts as null instead of inheriting
/// unrelated frame bytes, and the object used after that call remains alive.
#[test]
fn test_uninitialized_output_local_is_zero_initialized() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Cursor {
    private int $offset = 0;

    public function read(string $input): string {
        preg_match('/([A-Z_]+)/A', $input, $output, 0, $this->offset);
        $this->offset += strlen($output[0]);
        return $output[1] . ':' . $this->offset;
    }
}

echo (new Cursor())->read('APP_ENV=value');
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "APP_ENV:7", "stderr: {}", out.stderr);
    assert!(
        !out.stderr.contains("heap debug detected"),
        "unexpected heap corruption: {}",
        out.stderr
    );
}
