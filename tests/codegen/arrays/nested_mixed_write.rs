//! Purpose:
//! Regression coverage for nested writes through an `Array(Mixed)` element
//! (`$a[$i][$j] = ...`) and through an `ArrayAccess` object parent
//! (`$objects[$i][$key] = ...`) — the nested-write shapes the checker accepts
//! (issue #529).
//!
//! Called from:
//! - `cargo test --test codegen_tests arrays::nested_mixed_write`.
//!
//! Key details:
//! - The nested-assign statement used to lower the FULL target as a read and
//!   then replace the resulting Mixed cell in place. `__rt_mixed_array_get`
//!   returns a detached fresh box whenever the slot storage is not already a
//!   boxed Mixed cell (string/int/float slots of a concrete inner array), so
//!   the write mutated a temporary and was silently lost, leaking the
//!   replacement payload. When the slot WAS a boxed cell the write landed, but
//!   the retained cell returned by the read was never released (leak).
//! - The fix recursively fetches parent cells in write mode, COW-normalizes and
//!   stores back root containers, detaches selected zvals from outer aliases,
//!   then writes through the parent (`Op::RuntimeCall` →
//!   `__rt_mixed_array_set`, or `offsetSet` for `ArrayAccess` objects).

use crate::support::{compile_and_run, compile_and_run_with_heap_debug};

/// Issue #529 repro: the inner arrays are homogeneous `array<string>` (16-byte
/// string slots), the outer heterogeneous literal is `array<mixed>`. The write
/// must replace the stored element, not a detached copy, and stay heap-clean.
#[test]
fn test_nested_write_string_slot_inner_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y0'], ['x', 'y1'], ['x', 'y2'], 7];
$a[2][1] = 'patched';
echo $a[2][1] . "\n";
echo $a[1][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\ny1\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Same shape with homogeneous `array<int>` inner arrays (8-byte int slots).
#[test]
fn test_nested_write_int_slot_inner_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [[1, 2], [3, 4], 'z'];
$a[1][0] = 99;
echo $a[1][0] . "\n";
echo $a[0][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "99\n2\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Heterogeneous inner arrays store boxed Mixed cells: the write already
/// propagated on this shape, but the retained cell returned by the target
/// read was never released, leaking one block per write.
#[test]
fn test_nested_write_boxed_cell_inner_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$b = [[1, 'x'], [2, 'y'], 7];
$b[1][0] = 99;
echo $b[1][0] . "\n";
echo $b[1][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "99\ny\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Associative inner container: overwriting an existing string key and adding
/// a brand-new key must both land in the stored hash.
#[test]
fn test_nested_write_assoc_inner_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['k' => 'v'], 7];
$a[0]['k'] = 'patched';
$a[0]['new'] = 'added';
echo $a[0]['k'] . "\n";
echo $a[0]['new'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\nadded\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An associative outer container must expose its stored Mixed cell to the nested writer without
/// changing ordinary hash-read value semantics.
#[test]
fn test_nested_write_through_assoc_outer_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = ['row' => ['key' => 'old'], 'guard' => 7];
$outer['row']['key'] = 'patched';
echo $outer['row']['key'] . "\n";
echo $outer['guard'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\n7\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The write-only hash fetch must not make an ordinary Mixed hash read alias its stored zval.
#[test]
fn test_ordinary_assoc_read_remains_detached() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$outer = ['row' => ['key' => 'old'], 'guard' => 7];
$copy = $outer['row'];
$copy['key'] = 'copy';
echo $outer['row']['key'] . "\n";
echo $copy['key'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "old\ncopy\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Resource values keep PHP's shared resource identity across detached Mixed
/// reads, so releasing a temporary read cannot close storage still owned by the
/// source array before a later read consumes it.
#[test]
fn test_repeated_mixed_resource_reads_retain_the_stream() {
    let out = compile_and_run(
        r#"<?php
function resourceRow(): array {
    $stream = fopen("php://memory", "r+");
    fwrite($stream, "A" . chr(0) . "B");
    rewind($stream);
    return ['stream' => $stream, 'guard' => 7];
}
$values = resourceRow();
echo is_resource($values['stream'])
    ? bin2hex(stream_get_contents($values['stream']))
    : 'not-resource';
fclose($values['stream']);
"#,
    );
    assert_eq!(out, "410042");
}

/// A dynamically typed outer receiver uses the same fetch-for-write contract as statically known
/// indexed and associative containers.
#[test]
fn test_nested_write_through_mixed_receiver() {
    let out = compile_and_run(
        r#"<?php
function patchNested(mixed $outer): mixed {
    $outer[1][0] = 99;
    return $outer;
}
$result = patchNested([[1, 2], [3, 4], 'guard']);
echo $result[1][0] . "\n";
echo $result[0][1] . "\n";
"#,
    );
    assert_eq!(out, "99\n2\n");
}

/// An ordinary read through a dynamically typed receiver returns a detached zval even when the
/// runtime container already stores boxed Mixed cells for heterogeneous values.
#[test]
fn test_ordinary_mixed_receiver_read_remains_detached() {
    let out = compile_and_run(
        r#"<?php
function inspectCopy(mixed $outer): void {
    $copy = $outer[0];
    $copy['key'] = 'copy';
    echo $outer[0]['key'] . "\n";
    echo $copy['key'] . "\n";
}
inspectCopy([['key' => 'old'], 'guard']);
"#,
    );
    assert_eq!(out, "old\ncopy\n");
}

/// Dynamically typed associative storage applies the same split contract: ordinary reads detach,
/// while a later nested write aliases the COW-normalized owning hash entry.
#[test]
fn test_mixed_receiver_assoc_read_and_write_modes() {
    let out = compile_and_run(
        r#"<?php
function inspectAssoc(mixed $outer): void {
    $copy = $outer['row'];
    $copy['key'] = 'copy';
    echo $outer['row']['key'] . "\n";
    $outer['row']['key'] = 'patched';
    echo $outer['row']['key'] . "\n";
}
inspectAssoc(['row' => ['key' => 'old'], 'guard' => 7]);
"#,
    );
    assert_eq!(out, "old\npatched\n");
}

/// Fetch-for-write is an outer-container mutation for COW purposes: aliases must keep their old
/// nested value for both indexed and associative outer storage.
#[test]
fn test_nested_write_splits_outer_container_aliases() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$indexed = [['key' => 'old'], 'guard'];
$indexedAlias = $indexed;
$indexed[0]['key'] = 'patched';
echo $indexed[0]['key'] . "\n";
echo $indexedAlias[0]['key'] . "\n";

$assoc = ['row' => ['key' => 'old'], 'guard' => 7];
$assocAlias = $assoc;
$assoc['row']['key'] = 'patched';
echo $assoc['row']['key'] . "\n";
echo $assocAlias['row']['key'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\nold\npatched\nold\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The same outer COW split applies when the local's static type is Mixed and its runtime payload
/// is an indexed or associative array.
#[test]
fn test_nested_write_splits_mixed_outer_aliases() {
    let out = compile_and_run(
        r#"<?php
function inspectMixedCow(mixed $indexed, mixed $assoc): void {
    $indexedAlias = $indexed;
    $indexed[0]['key'] = 'patched';
    echo $indexed[0]['key'] . "\n";
    echo $indexedAlias[0]['key'] . "\n";

    $assocAlias = $assoc;
    $assoc['row']['key'] = 'patched';
    echo $assoc['row']['key'] . "\n";
    echo $assocAlias['row']['key'] . "\n";
}
inspectMixedCow(
    [['key' => 'old'], 'guard'],
    ['row' => ['key' => 'old'], 'guard' => 7],
);
"#,
    );
    assert_eq!(out, "patched\nold\npatched\nold\n");
}

/// Runtime-produced containers whose outer storage already consists entirely of boxed Mixed cells
/// still detach the selected cell after COW; `json_decode()` exercises that representation.
#[test]
fn test_nested_write_splits_json_mixed_outer_aliases() {
    let out = compile_and_run(
        r#"<?php
function inspectJsonCow(mixed $indexed, mixed $assoc): void {
    $indexedAlias = $indexed;
    $indexed[0]['key'] = 'patched';
    echo $indexed[0]['key'] . "\n";
    echo $indexedAlias[0]['key'] . "\n";

    $assocAlias = $assoc;
    $assoc['row']['key'] = 'patched';
    echo $assoc['row']['key'] . "\n";
    echo $assocAlias['row']['key'] . "\n";
}
inspectJsonCow(
    json_decode('[{"key":"old"},"guard"]', true),
    json_decode('{"row":{"key":"old"},"guard":7}', true),
);
"#,
    );
    assert_eq!(out, "patched\nold\npatched\nold\n");
}

/// A nullable receiver that is non-null at runtime must preserve the same nested-write alias as a
/// non-nullable receiver after the null guard has selected the read path.
#[test]
fn test_nested_write_through_nullable_receiver() {
    let out = compile_and_run(
        r#"<?php
function patchNullable(?array $outer): void {
    $outer[0]['key'] = 'patched';
    echo $outer[0]['key'] . "\n";
}
patchNullable([['key' => 'old']]);
"#,
    );
    assert_eq!(out, "patched\n");
}

/// Compound nested assignment reads the stale element and writes the combined
/// result back through the same nested-write path.
#[test]
fn test_nested_compound_assign() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['x', 'y'], 7];
$a[0][1] .= '!';
echo $a[0][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "y!\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Three-level chain through boxed-cell intermediates: the middle read
/// recursively uses fetch-for-write. Concrete homogeneous intermediates are
/// promoted into the owning slot before the leaf write, so no detached COW
/// generation can absorb and lose the update.
#[test]
fn test_nested_write_three_levels() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [[['x', 1], 5], 7];
$a[0][0][0] = 'patched';
echo $a[0][0][0] . "\n";
echo $a[0][0][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\n1\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The nested write must survive a function-return boundary on the outer array.
#[test]
fn test_nested_write_survives_function_return() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function patch(): array {
    $a = [['x', 'y0'], ['x', 'y1'], 7];
    $a[1][1] = 'patched';
    return $a;
}
$r = patch();
echo $r[1][1] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Object `ArrayAccess` parent: `$boxes[0]` is a concrete object, so the
/// three-operand nested write must dispatch to `offsetSet` (not Mixed cell
/// replacement) and persist into the stored instance.
#[test]
fn test_nested_write_array_access_object_parent() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box implements ArrayAccess {
    private string $x = 'old';
    private int $y = 1;
    public function offsetExists(mixed $offset): bool {
        return $offset === 'x' || $offset === 'y';
    }
    public function offsetGet(mixed $offset): mixed {
        if ($offset === 'x') {
            return $this->x;
        }
        if ($offset === 'y') {
            return $this->y;
        }
        return null;
    }
    public function offsetSet(mixed $offset, mixed $value): void {
        if ($offset === 'x') {
            $this->x = (string)$value;
        }
        if ($offset === 'y') {
            $this->y = (int)$value;
        }
    }
    public function offsetUnset(mixed $offset): void {}
}
$boxes = [new Box()];
$boxes[0]['x'] = 'patched';
echo $boxes[0]['x'] . "\n";
echo $boxes[0]['y'] . "\n";
"#,
    );
    assert_eq!(out.stdout, "patched\n1\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
