//! Purpose:
//! Regression tests for a borrowed typed array being widened in place at an argument
//! boundary, which rewrote the CALLER's array.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Passing `array<P>` to a parameter declared `array` emits `__rt_array_to_mixed`, which
//!   CONSUMES an owner slot: it splits through `__rt_array_ensure_unique`, and that helper
//!   only clones when the refcount says the array is shared. A borrowed array therefore
//!   reached it looking unique and had its element slots rewritten in place, so the caller
//!   read boxed cells as raw object pointers AFTER the call — on data the callee never
//!   touched, with no diagnostic.
//! - Only arrays of OBJECTS are affected; int, string and associative arrays came through
//!   unchanged. Measured, not assumed.

use crate::support::*;

/// A borrowed array of objects survives being passed to a gradual `array` parameter.
///
/// Every result is USED on purpose. `f($pts);` as a bare statement is elided as dead and
/// emits no call at all, so a probe written that way reports success while exercising
/// nothing — three of the seven cases below looked correct for exactly that reason before
/// the results were bound.
///
/// The seven cases are the ones that separate the defect from its neighbours: the direct
/// call and the method call were both broken, the callee handing the array back was broken,
/// and int/string/associative arrays plus an owned temporary were not. An owned temporary
/// must keep converting in place — it has no other reader, so cloning it would be pure cost.
#[test]
fn test_borrowed_array_of_objects_survives_a_gradual_array_parameter() {
    let out = compile_and_run(
        r#"<?php
class P { public function __construct(public float $x) {} }
class Holder { public function take(array $a): int { return 1; } }
function f(array $a): float { return 1.0; }
function g(array $a): array { return $a; }
function ints(array $a): int { return 1; }
function strs(array $a): int { return 1; }
function assoc(array $a): int { return 1; }

$pts = [new P(1.0), new P(2.0), new P(3.0)];
$r1 = f($pts);
echo $pts[0]->x, ",", $pts[1]->x, ",", $pts[2]->x, "|";

$pts2 = [new P(4.0)];
$out = g($pts2);
echo $pts2[0]->x, "|";

$nums = [10, 20, 30];
$r2 = ints($nums);
echo $nums[0], ",", $nums[1], ",", $nums[2], "|";

$names = ["ab", "cd"];
$r3 = strs($names);
echo $names[0], ",", $names[1], "|";

$map = ["k" => new P(9.0)];
$r4 = assoc($map);
echo $map["k"]->x, "|";

$h = new Holder();
$pts3 = [new P(7.0)];
$r5 = $h->take($pts3);
echo $pts3[0]->x, "|";

echo f([new P(5.0)]);
"#,
    );
    assert_eq!(out, "1,2,3|4|10,20,30|ab,cd|9|7|1");
}

/// The clone the caller now makes is released once the callee returns.
///
/// The fix makes a borrowed array visibly shared so the widening conversion clones it
/// instead of rewriting the original. That clone is caller-owned, so the boundary that
/// increfs and the one that releases have to stay in step — an incref with no cleanup slot
/// leaks the clone, and a cleanup slot with no incref releases the caller's own array.
#[test]
fn test_widening_a_borrowed_array_does_not_leak_the_clone() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public float $x) {} }
function f(array $a): float { return 1.0; }
$pts = [new P(1.0), new P(2.0)];
$r = f($pts);
echo $pts[0]->x;
"#,
    );
    let report = format!("{}{}", out.stdout, out.stderr);
    assert!(
        report.contains("leak summary: clean"),
        "widening a borrowed array must not leak the clone it makes:\n{report}"
    );
}
