//! Purpose:
//! Regression coverage for the ref-cell by-value return bug: once `$r = &$x`
//! promotes `$x` to a ref-cell, loads of `$x`/`$r` are `Op::LoadRefCell` whose
//! codegen leaves the dereferenced payload in the result registers. The return
//! path must acquire that payload before the epilogue's ref-cell owner cleanup
//! runs, or the caller receives a snapshot aliasing freed storage (count reads
//! 0) and a second call double-frees the block, corrupting the free list into
//! a cycle that hangs the binary.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - By-value returns of ref-bound locals hand the caller an owned snapshot
//!   (incref for containers, persist for strings); the epilogue then releases
//!   only the cell's own reference.
//! - By-reference-returning functions (`function &f()`) hand back the cell
//!   pointer instead, so they must NOT acquire the payload — covered by
//!   `test_byref_return_function_still_works`.

use super::*;

/// `return $x;` after `$r = &$x` returns an owned one-element array; the caller
/// observes `count == 1` (PHP semantics) rather than the freed-block length
/// word that the pre-fix code exposed.
#[test]
fn test_refcell_return_via_original() {
    let out = compile_and_run(
        r#"<?php
function f(): array {
    $x = ['a'];
    $r = &$x;
    return $x;
}
echo count(f());
"#,
    );
    assert_eq!(out, "1");
}

/// `return $r;` returns the alias's payload after a push through the original
/// variable; the snapshot owned by the caller reflects the two elements
/// present at the return point and is not corrupted by the epilogue cleanup.
#[test]
fn test_refcell_return_via_alias() {
    let out = compile_and_run(
        r#"<?php
function f(): array {
    $x = ['a'];
    $r = &$x;
    $x[] = 'b';
    return $r;
}
echo count(f());
"#,
    );
    assert_eq!(out, "2");
}

/// Two calls into the same by-value ref-cell-returning function must not hang
/// the binary (pre-fix: the first call's freed block's link was clobbered by
/// the second call's free, producing a cyclic free list). Both snapshots
/// survive independently.
#[test]
fn test_refcell_return_two_calls_no_hang() {
    let out = compile_and_run(
        r#"<?php
function f(): array {
    $x = ['a'];
    $r = &$x;
    $x[] = 'b';
    return $r;
}
$one = f();
$two = f();
echo count($one) . count($two);
"#,
    );
    assert_eq!(out, "22");
}

/// A ref-bound string returned by value must be persisted into an independent
/// heap copy the caller owns; two calls produce two intact snapshots and do
/// not double-free the shared interned/heap string.
#[test]
fn test_refcell_return_string_payload() {
    let out = compile_and_run(
        r#"<?php
function g(): string {
    $s = 'abc';
    $r = &$s;
    return $s;
}
$a = g();
$b = g();
echo $a . '|' . $b;
"#,
    );
    assert_eq!(out, "abc|abc");
}

/// Returning a by-reference parameter by value hands the caller an owned
/// snapshot isolated from the caller's own storage: a later mutation of the
/// caller's array is not visible through the returned snapshot, and vice
/// versa.
#[test]
fn test_refcell_return_byref_param() {
    let out = compile_and_run(
        r#"<?php
function h(array &$p): array {
    return $p;
}
$arr = ['x','y'];
$c = h($arr);
$arr[] = 'z';
echo count($c) . '|' . count($arr);
"#,
    );
    assert_eq!(out, "2|3");
}

/// Guard for the `!ctx.by_ref_return` gate in `acquire_borrowed_return_value`:
/// a `function &f()` returning a property by reference must still hand the
/// caller the cell pointer so `$ref = &sel($b)` aliases the property and a
/// push through `$ref` is observed through `$b->items`. Acquiring the payload
/// here would break the alias.
#[test]
fn test_byref_return_function_still_works() {
    let out = compile_and_run(
        r#"<?php
class B { public array $items = ['i']; }
function &sel(B $b): array { return $b->items; }
$b = new B();
$ref = &sel($b);
$ref[] = 'j';
echo count($b->items);
"#,
    );
    assert_eq!(out, "2");
}
