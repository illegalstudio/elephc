//! Purpose:
//! Regression tests for issue #1041: a read-modify-write on a typed static
//! property, or on an element of an object property's array, leaked one boxed
//! Mixed cell per execution.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Both halves share one shape: lowering picked the "moving store" discipline
//!   for a store the backend actually performs as an *independent* one. The
//!   static-property store narrows the Mixed box out into a payload word, and
//!   the indexed-array store increfs the box into the slot; either way the
//!   producer's own reference is still outstanding after the store, and nothing
//!   released it.
//! - Every fixture runs under `--heap-debug` and asserts `leak summary: clean`,
//!   so a release that is emitted twice fails here as a double free rather than
//!   passing silently.
//! - The controls (`mixed` slots, `float`/`string` slots, assoc-keyed property
//!   arrays, plain non-compound writes) pin the disciplines that must NOT gain a
//!   release: `__rt_hash_set` and a Mixed→Mixed store both consume their value,
//!   and releasing there would be a use-after-free.
//! - The aliasing and read-back fixtures cover copy-on-write and ownership: the
//!   snapshot taken before the loop must not observe the element writes, and a
//!   value read out between two compound writes must survive the release of the
//!   box it came from.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Issue #1041 repro, first half: `T::$s += 1` on an `int` static property.
/// `ichecked_add` returns a boxed Mixed (it may overflow to float), the store
/// narrows it with `__rt_mixed_cast_int`, and the box was never released — one
/// 40-byte cell per iteration.
#[test]
fn test_issue_1041_static_int_property_compound_add_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static int $s = 0; }
for ($i = 0; $i < 20; $i++) { T::$s += $i; }
echo T::$s, "\n";
"#,
    );
    assert_clean(out, "190\n");
}

/// The increment spelling is irrelevant to the defect: `++` lowers through the
/// same checked-add producer, so both forms leaked identically.
#[test]
fn test_issue_1041_static_int_property_increment_forms_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static int $s = 0; }
for ($i = 0; $i < 20; $i++) { T::$s++; ++T::$s; }
echo T::$s, "\n";
"#,
    );
    assert_clean(out, "40\n");
}

/// A nullable-int slot is a `TaggedScalar`, not a Mixed cell: the store unboxes
/// into a payload/tag register pair, which owns nothing of the box either.
#[test]
fn test_issue_1041_static_nullable_int_property_compound_add_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static ?int $s = 0; }
for ($i = 0; $i < 20; $i++) { T::$s += $i; }
var_dump(T::$s);
"#,
    );
    assert_clean(out, "int(190)\n");
}

/// A *borrowed* Mixed narrowed into an int slot leaked the other way round: the
/// moving-store discipline `Acquire`d it first, and that retain had no matching
/// release once the slot only kept a payload word.
#[test]
fn test_issue_1041_static_int_property_from_borrowed_mixed_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static int $s = 0; }
function bump(mixed $m): void { T::$s = $m; }
for ($i = 0; $i < 20; $i++) { bump($i); }
echo T::$s, "\n";
"#,
    );
    assert_clean(out, "19\n");
}

/// A bool slot narrows through `__rt_mixed_cast_bool`, the sibling arm of the
/// int cast, and leaked the same single cell per assignment.
#[test]
fn test_issue_1041_static_bool_property_from_mixed_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static bool $b = false; }
function make(int $i): mixed { return $i % 2 === 0; }
for ($i = 0; $i < 20; $i++) { T::$b = make($i); }
var_dump(T::$b);
"#,
    );
    assert_clean(out, "bool(false)\n");
}

/// The expression form of the same compound assignment: the released box must
/// not be the one the surrounding expression reads back.
#[test]
fn test_issue_1041_static_int_property_compound_as_expression_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static int $s = 0; }
$last = 0;
for ($i = 0; $i < 20; $i++) { $last = (T::$s += 2); }
echo $last, ' ', T::$s, "\n";
"#,
    );
    assert_clean(out, "40 40\n");
}

/// Use-after-free guard: a value read out of the slot between two compound
/// writes must still be readable after the second write released its box.
#[test]
fn test_issue_1041_static_int_property_value_survives_release() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static int $s = 0; }
T::$s += 5;
$kept = T::$s;
T::$s += 5;
var_dump($kept, T::$s);
"#,
    );
    assert_clean(out, "int(5)\nint(10)\n");
}

/// Control: a `float` slot takes the value in a register with no box at all, so
/// it allocated nothing before the fix and must allocate nothing after it.
#[test]
fn test_static_float_property_compound_add_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static float $f = 0.0; }
for ($i = 0; $i < 20; $i++) { T::$f += $i * 1.5; }
var_dump(T::$f);
"#,
    );
    assert_clean(out, "float(285)\n");
}

/// Control: a `string` slot fed a string concatenation stays on the persistent
/// string path, which never boxed and must not gain a release.
#[test]
fn test_static_string_property_compound_concat_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public static string $s = ''; }
for ($i = 0; $i < 20; $i++) { T::$s .= 'a'; }
echo strlen(T::$s), "\n";
"#,
    );
    assert_clean(out, "20\n");
}

/// Issue #1041 repro, second half: `$o->items[0] += 1` on an object property's
/// array. `__rt_array_set_mixed` increfs the boxed value into the slot, so the
/// producer's reference stayed outstanding — one cell per write.
#[test]
fn test_issue_1041_property_array_element_compound_add_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 2]; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->items[0] += 1; $o->items[1] += 2; }
echo $o->items[0], ' ', $o->items[1], "\n";
"#,
    );
    assert_clean(out, "21 42\n");
}

/// The increment spelling, again irrelevant: it is the read-modify-write that
/// produces the owned box, not the operator.
#[test]
fn test_issue_1041_property_array_element_increment_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 2]; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->items[0]++; }
echo $o->items[0], "\n";
"#,
    );
    assert_clean(out, "21\n");
}

/// Spelled out by hand, which is what proves the defect is the store and not
/// the compound-assignment lowering: `$o->items[0] = $o->items[0] + 1` leaked
/// exactly as `+=` did.
#[test]
fn test_issue_1041_property_array_element_explicit_read_modify_write_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 2]; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->items[0] = $o->items[0] + 1; }
echo $o->items[0], "\n";
"#,
    );
    assert_clean(out, "21\n");
}

/// The append sibling: `__rt_array_push_refcounted` retains an already-boxed
/// Mixed value just like the indexed setter, so a pushed owned temporary leaked
/// through the same hole.
#[test]
fn test_issue_1041_property_array_push_owned_mixed_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 'x']; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->items[] = $o->items[0] + 1; }
echo count($o->items), ' ', $o->items[21], "\n";
"#,
    );
    assert_clean(out, "22 2\n");
}

/// Any already-boxed producer leaks the same way, not just arithmetic: a call
/// returning `mixed` hands the store an owned cell too.
#[test]
fn test_issue_1041_property_array_element_from_mixed_call_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(int $i): mixed { return $i + 1; }
class T { public array $items = [1, 'x']; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->items[0] = f($i); }
var_dump($o->items[0], $o->items[1]);
"#,
    );
    assert_clean(out, "int(20)\nstring(1) \"x\"\n");
}

/// Control: the string-keyed sibling goes through `__rt_hash_set`, which only
/// retains a value that cannot hand over its own reference. It was already
/// clean, and adding a release there would double-free.
#[test]
fn test_assoc_property_array_element_compound_add_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $m = ['k' => 1, 'j' => 'x']; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->m['k'] += 1; }
var_dump($o->m['k'], $o->m['j']);
"#,
    );
    assert_clean(out, "int(21)\nstring(1) \"x\"\n");
}

/// Control: a concrete value written into a Mixed-element array is boxed by
/// codegen, and that boxing path releases the box it made. The element value
/// must stay readable, so no second release may be emitted for it.
#[test]
fn test_property_array_element_boxed_array_value_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 'x']; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $o->items[0] = [$i, $i + 1]; }
var_dump($o->items[0]);
"#,
    );
    assert_clean(
        out,
        "array(2) {\n  [0]=>\n  int(19)\n  [1]=>\n  int(20)\n}\n",
    );
}

/// The same guard for an object element: the stored object must survive every
/// overwrite and still be dereferenceable at the end.
#[test]
fn test_property_array_element_object_value_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public int $v = 0; }
class T { public array $items = [1, 'x']; }
$o = new T();
for ($i = 0; $i < 20; $i++) { $c = new C(); $c->v = $i; $o->items[0] = $c; }
var_dump($o->items[0]->v);
"#,
    );
    assert_clean(out, "int(19)\n");
}

/// Copy-on-write: a snapshot taken before the loop keeps PHP value semantics —
/// it must not observe the element writes — and releasing the producer's box
/// must not reach through the snapshot's own copy.
#[test]
fn test_issue_1041_property_array_snapshot_unaffected_by_element_writes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 2]; }
$o = new T();
$snapshot = $o->items;
for ($i = 0; $i < 20; $i++) { $o->items[0] += 1; }
var_dump($snapshot[0], $o->items[0]);
"#,
    );
    assert_clean(out, "int(1)\nint(21)\n");
}

/// Use-after-free guard for the array half: a value read between two compound
/// writes must survive the release that follows the second one, and the
/// untouched sibling element must be intact.
#[test]
fn test_issue_1041_property_array_element_value_survives_release() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 2]; }
$o = new T();
$o->items[0] += 1;
$kept = $o->items[0];
$o->items[0] += 1;
var_dump($kept, $o->items[0], $o->items[1]);
"#,
    );
    assert_clean(out, "int(2)\nint(3)\nint(2)\n");
}

/// The issue's own fixture, verbatim: a fresh object every iteration, so the
/// leak had to be per-write rather than a fixed process-lifetime cost.
#[test]
fn test_issue_1041_reported_property_array_fixture_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [1, 2]; }
for ($i = 0; $i < 50; $i++) { $o = new T(); $o->items[0] += 1; }
echo $o->items[0];
"#,
    );
    assert_clean(out, "2");
}
