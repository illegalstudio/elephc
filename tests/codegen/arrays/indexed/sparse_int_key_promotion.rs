//! Purpose:
//! Regression tests for PHP's array-promotion rule: writing an integer key that is not the
//! next packed index (or is negative) must APPEND `(key => value)` at the end of insertion
//! order and, if the destination was packed, convert it to a hash — never pad the gap with
//! nulls. `count()` counts entries, never slots.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A LITERAL integer-key write (`$a[10] = "x"`) is lowered straight to a hash op at IR-build
//!   time and was already correct before this fix; the defect lives in the shared runtime
//!   helpers used whenever the key is not a compile-time integer literal — a plain `int`
//!   variable, a nested autovivified array, or a boxed Mixed container reached through a
//!   property or `eval()`. Every fixture below that uses `$p`/a property key is therefore the
//!   one that actually exercises the buggy runtime path; the literal-key fixtures are
//!   regression guards proving the already-correct compile-time path keeps working.
//! - Verified against `LC_ALL=C php` 8.5.6 output for every fixture in this file.

use super::*;

/// Verifies every literal-int-key shape from the measurement matrix stays correct: PHP never
/// pads a gap, and elephc's array-literal lowering already avoids the runtime bug for a
/// compile-time-literal key. Regression guard — must not regress when the variable-key path
/// (`test_variable_int_key_write_on_empty_array_promotes_to_hash` etc.) is fixed.
#[test]
fn test_literal_int_key_writes_match_php_and_never_regress() {
    let out = compile_and_run(
        r#"<?php
function show(string $label, array $a): void
{
    echo $label, " count=", count($a), " keys=", implode("/", array_keys($a)), "\n";
}

$a = [];
$a[10] = "x";
show("lit_empty", $a);

$c = [];
$c[10][] = "x";
show("lit_autoviv", $c);

$d = [];
$d[5] = "x";
$d[3] = "y";
show("lit_sparse", $d);

$f = [];
$f["s"] = "x";
$f[10] = "y";
show("mixed_string_then_int", $f);

$g = [];
$g[] = "x";
$g[] = "y";
show("appends", $g);

$h = [3 => "x"];
show("literal_int3", $h);
"#,
    );
    assert_eq!(
        out,
        "lit_empty count=1 keys=10\n\
         lit_autoviv count=1 keys=10\n\
         lit_sparse count=2 keys=5/3\n\
         mixed_string_then_int count=2 keys=s/10\n\
         appends count=2 keys=0/1\n\
         literal_int3 count=1 keys=3\n"
    );
}

/// THE failing case (pre-fix): `$a[$p] = "x"` on an empty array, where `$p` is a plain `int`
/// local holding a value beyond the array's current length. Pre-fix, elephc's
/// `__rt_array_set_str` (reached through `lower_array_set`) unconditionally grows the packed
/// indexed representation and zero-fills every slot up to the target index instead of
/// converting to a hash: `count()` reported 11 with nine null gap slots and `array_keys()`
/// reported `0..10` instead of the single key `10`.
#[test]
fn test_variable_int_key_write_on_empty_array_promotes_to_hash() {
    let out = compile_and_run(
        r#"<?php
function show(string $label, array $a): void
{
    echo $label, " count=", count($a), " keys=", implode("/", array_keys($a)), "\n";
}
$p = 10;
$b = [];
$b[$p] = "x";
show("var_empty", $b);
"#,
    );
    assert_eq!(out, "var_empty count=1 keys=10\n");
}

/// Companion to the sparse write above: PHP's next append index after a sparse integer-key
/// write is `max(int keys) + 1`, not "packed length". A packed-representation bug that pads to
/// index 10 would make the following `[] = "y"` land at index 11 with twelve total entries
/// instead of two. Also exercises a THIRD independent call site sharing the family's missing
/// promotion condition: `__rt_array_push_str`/`__rt_array_push_int`/`__rt_array_push_refcounted`
/// (the append helpers `$a[] = value` lowers to) must recognize an already-promoted hash
/// destination and delegate to `__rt_hash_append`, or the append silently lands nowhere.
///
/// Routed through `show(string $label, array $a)` like the other cases in this file: reading
/// `count()`/`array_keys()` directly on a local whose static type stays a non-Mixed `Array(Str)`
/// after a runtime promotion is a SEPARATE, pre-existing gap in those builtins' read path
/// (out of scope here — see the module doc comment) that a bare `echo count($w)` would trip
/// over even after the write side is correct; crossing the `array $a` parameter boundary forces
/// the same `__rt_array_to_mixed` conversion the read path already gets right.
#[test]
fn test_append_after_sparse_variable_int_key_uses_max_key_plus_one() {
    let out = compile_and_run(
        r#"<?php
function show(string $label, array $a): void
{
    echo $label, " count=", count($a), " keys=", implode("/", array_keys($a)), "\n";
}
$p = 10;
$w = [];
$w[$p] = "x";
$w[] = "y";
show("append_after_sparse", $w);
"#,
    );
    assert_eq!(out, "append_after_sparse count=2 keys=10/11\n");
}

/// A second write into an array that a first sparse write already promoted to a hash: PHP
/// inserts in insertion order regardless of key ordering (`5/3`, not sorted). This exercises
/// the "already hash" branch a fix must add — routing straight to `__rt_hash_set` instead of
/// re-entering the packed-array COW/grow machinery that only understands indexed storage.
/// Routed through `show()`; see the comment on the append test above for why.
#[test]
fn test_second_variable_int_key_write_inserts_into_already_promoted_hash() {
    let out = compile_and_run(
        r#"<?php
function show(string $label, array $a): void
{
    echo $label, " count=", count($a), " keys=", implode("/", array_keys($a)), "\n";
}
$p1 = 5;
$p2 = 3;
$d = [];
$d[$p1] = "x";
$d[$p2] = "y";
show("second_write", $d);
"#,
    );
    assert_eq!(out, "second_write count=2 keys=5/3\n");
}

/// Nested autovivification through a variable key: `$d[$p][] = "x"` where `$p` is a plain int
/// local. Reaches the fetch-for-write autovivification helper rather than the direct setter,
/// so it is a distinct call site sharing the same missing promotion condition.
#[test]
fn test_variable_int_key_nested_autoviv_promotes_to_hash() {
    let out = compile_and_run(
        r#"<?php
function show(string $label, array $a): void
{
    echo $label, " count=", count($a), " keys=", implode("/", array_keys($a)), "\n";
}
$p = 10;
$d = [];
$d[$p][] = "x";
show("var_autoviv", $d);
"#,
    );
    assert_eq!(out, "var_autoviv count=1 keys=10\n");
}

/// The Symfony `EventDispatcher::addListener` shape: a bare-`array` PROPERTY reached through a
/// string key (autovivifying a nested array) and then written at an integer PRIORITY key that
/// is not the next packed index. This is the exact mechanism that blocked `--web`: the
/// property's nested container is a boxed-Mixed array reached through
/// `__rt_mixed_array_set`/`__rt_mixed_array_get_for_write`, a separate implementation of the
/// identical missing promotion rule.
#[test]
fn test_property_nested_sparse_priority_key_matches_symfony_shape() {
    let out = compile_and_run(
        r#"<?php
class C {
    private array $listeners = [];
    public function add(string $event, int $priority, string $v): void {
        $this->listeners[$event][$priority][] = $v;
    }
    public function dump(string $event): array { return $this->listeners[$event]; }
}
$c = new C();
$c->add("evt", 10, "a");
$c->add("evt", 5, "b");
$c->add("evt", 1, "c");
$r = $c->dump("evt");
echo count($r), " ", implode("/", array_keys($r));
"#,
    );
    assert_eq!(out, "3 10/5/1");
}

// NOTE: repo memory (`literal-int-key-write-densifies-and-crashes`) reports `$a[0] = 3` on an
// already-hash array (`['x' => 1, 'y' => 2]`) crashing with SIGBUS. Measured again here: a
// standalone CLI compile of that exact fixture answers PHP's `3 x/y/0` correctly, but the same
// source through this suite's `compile_and_run` (fixed 8 MiB heap, no `--with-regex`) reliably
// SIGSEGVs before any output completes. Neither this defect's write path
// (`__rt_array_set_*`/`__rt_mixed_array_set`/`__rt_array_push_*`) nor the fix above touches the
// literal-associative-key setter this fixture exercises (`$z` is `AssocArray`-typed from an
// associative literal, which lowers to the hash setter directly, never `lower_array_set`), and
// the crash reproduces identically on this branch before and after this change — not fixed, not
// regressed. Deliberately NOT added as a permanent test here: it would leave the suite red for a
// pre-existing, unrelated, heap-layout-sensitive defect this change does not touch.

/// Interpreted-path twin of `test_variable_int_key_write_on_empty_array_promotes_to_hash`:
/// `eval()` always represents every PHP value as a boxed `Mixed` cell (`__elephc_eval_value_array_set`
/// dispatches through `__rt_mixed_array_set`, the SAME runtime helper reached by a compiled
/// write into a bare-`array` property), so this reproduces the identical defect through the
/// eval bridge instead of direct compilation.
#[test]
fn test_eval_interpreted_variable_int_key_write_promotes_to_hash() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'CODE'
$p = 10;
$a = [];
$a[$p] = "x";
echo count($a), " ", implode(",", array_keys($a));
CODE;
eval($code);
"#,
    );
    assert_eq!(out, "1 10");
}
