//! Purpose:
//! Regression tests for issue #690: a by-reference `foreach` over an object property reached
//! through a receiver that names no storage of its own — a runtime-named property, an object
//! held in an array element, a call result.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - #642 covered the direct `$o->x` receiver. Its fix reads the property FOR WRITE, which hands
//!   the loop the property's own container BORROWED — so the object that owns the slot has to
//!   outlive the loop. A stable receiver does that by itself; these three do not, so the loop
//!   holds the receiver and releases it on every way out.
//! - The failure modes differed by receiver, which is why all three are pinned separately: the
//!   runtime-named and call-result forms FREED the property's container (the read-back printed
//!   nothing and `count()` returned a heap address), while the array-element form merely
//!   iterated a copy and lost the writes.
//! - Each mutating fixture has a by-VALUE twin: the same source must NOT be mutated there, which
//!   is what keeps the fix from turning every `foreach` over a property into a by-reference one.

use super::*;

/// Regression for issue #690: a RUNTIME-NAMED property source must mutate the property in place.
///
/// `$o->$n` never reached the fetch-for-write read at all — the source dispatch only recognized
/// the static spelling — so the loop took the ordinary retaining read, whose loop-exit release
/// consumed the reference `__rt_array_ensure_unique` had already taken from the property. The
/// read-back printed nothing.
#[test]
fn test_regression_690_by_ref_foreach_over_a_dynamically_named_property() {
    let out = compile_and_run(
        r#"<?php
class C { public array $x = [1, 2]; }
$o = new C(); $n = 'x';
foreach ($o->$n as &$v) { $v *= 2; }
unset($v);
echo implode(',', $o->x), ' count=', count($o->x);
"#,
    );
    assert_eq!(out, "2,4 count=2");
}

/// Regression for issue #690: a property of an object held in an ARRAY ELEMENT.
///
/// This receiver kept the container intact and merely lost the writes: the element read is a
/// temporary, so the source fell back to the retaining read and the loop iterated the copy
/// `IterStart` made.
#[test]
fn test_regression_690_by_ref_foreach_over_a_property_of_an_array_element() {
    let out = compile_and_run(
        r#"<?php
class C { public array $x = [1, 2]; }
$arr = [new C()];
foreach ($arr[0]->x as &$v) { $v *= 2; }
unset($v);
echo implode(',', $arr[0]->x), ' count=', count($arr[0]->x);
"#,
    );
    assert_eq!(out, "2,4 count=2");
}

/// Regression for issue #690: a property of an object returned by a METHOD.
///
/// The receiver is an owning temporary. Releasing it as soon as the property had been read freed
/// the object whose slot owns the container the iterator was walking, so the loop wrote into
/// freed storage and the read-back printed nothing.
#[test]
fn test_regression_690_by_ref_foreach_over_a_property_of_a_call_result() {
    let out = compile_and_run(
        r#"<?php
class Inner { public array $x = [1, 2]; }
class Outer {
    private Inner $i;
    function __construct() { $this->i = new Inner(); }
    function get(): Inner { return $this->i; }
}
$o = new Outer();
foreach ($o->get()->x as &$v) { $v *= 2; }
unset($v);
echo implode(',', $o->get()->x), ' count=', count($o->get()->x);
"#,
    );
    assert_eq!(out, "2,4 count=2");
}

/// Verifies the mutation is visible to the receiver DURING the loop, not only after it.
///
/// A loop writing into a copy that is republished at the end would pass the read-back tests
/// above while still being the wrong semantics. PHP mutates the container as it goes.
#[test]
fn test_regression_690_writes_are_visible_through_the_receiver_during_the_loop() {
    let out = compile_and_run(
        r#"<?php
class C { public array $x = [1, 2, 3]; }
$arr = [new C()];
foreach ($arr[0]->x as &$v) {
    $v *= 2;
    echo $arr[0]->x[0], ';';
}
unset($v);
echo implode(',', $arr[0]->x);
"#,
    );
    assert_eq!(out, "2;2;2;2,4,6");
}

/// Guard for issue #690: a by-VALUE `foreach` over the same three receivers must NOT mutate.
///
/// The fix widened which sources take the fetch-for-write read; by-value loops must keep the
/// ordinary retaining read, whose copy is exactly the semantics PHP has there.
#[test]
fn test_regression_690_by_value_foreach_over_the_same_receivers_does_not_mutate() {
    let out = compile_and_run(
        r#"<?php
class Inner { public array $x = [1, 2]; }
class Outer {
    private Inner $i;
    function __construct() { $this->i = new Inner(); }
    function get(): Inner { return $this->i; }
}
$o = new Inner(); $n = 'x';
foreach ($o->$n as $v) { $v *= 2; }
echo implode(',', $o->x), '|';
$arr = [new Inner()];
foreach ($arr[0]->x as $v) { $v *= 2; }
echo implode(',', $arr[0]->x), '|';
$outer = new Outer();
foreach ($outer->get()->x as $v) { $v *= 2; }
echo implode(',', $outer->get()->x);
"#,
    );
    assert_eq!(out, "1,2|1,2|1,2");
}

/// Verifies the receiver the loop holds is released exactly once, on every way out.
///
/// The loop takes a reference on an unstable receiver so the property slot keeps owning the
/// container being written. `break`, `return` and `throw` all skip the loop's own exit block, so
/// each one has to drop that reference through the loop frame instead — a miss leaks one object
/// per iteration of the outer loop, and a double release frees a live one.
#[test]
fn test_regression_690_holding_the_receiver_stays_heap_clean_on_every_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $x = [1, 2, 3]; }
function run(array $arr): int {
    foreach ($arr[0]->x as &$v) { $v *= 2; return $v; }
    return 0;
}
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $arr = [new C()];
    foreach ($arr[0]->x as &$a) { $a *= 2; break; }
    unset($a);
    $total += $arr[0]->x[0];
    $total += run([new C()]);
    try {
        foreach ($arr[0]->x as &$b) { throw new RuntimeException('stop'); }
    } catch (RuntimeException $e) {
        $total += 1;
    }
    unset($b);
}
echo $total;
"#,
    );
    assert_eq!(out.stdout, "200", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("leak summary: clean"),
        "holding the receiver must stay balanced on every exit: {}",
        out.stderr
    );
}

/// Verifies a receiver that is null at run time raises PHP's `Error` instead of being read.
///
/// An element read that MISSES answers the null sentinel with the element's declared type, so a
/// statically non-null receiver can still be null here. PHP evaluates a by-reference `foreach`
/// source in a WRITE context, where that is a fatal `Error` — not the warning a plain read
/// produces, which is what elephc used to print before continuing.
#[test]
fn test_regression_690_null_receiver_raises_phps_error() {
    let out = compile_and_run_expect_failure(
        r#"<?php
class C { public array $x = [1, 2]; }
$arr = [new C()];
foreach ($arr[5]->x as &$v) { $v *= 2; }
echo "not reached";
"#,
    );
    assert!(
        out.contains("Attempt to modify property \"x\" on null"),
        "expected PHP's write-context Error, got: {out}"
    );
    assert!(
        !out.contains("not reached"),
        "the Error must stop the program, got: {out}"
    );
}

/// Verifies a throw CAUGHT INSIDE the loop leaves the loop's own storage alone.
///
/// The catch is inside the loop, so control resumes there and iteration continues — PHP does not
/// leave the loop at all. Releasing what the loop holds on the way to that catch frees the
/// container it is still writing into, and the loop's normal termination then releases it a
/// second time. Measured before the fix: the read-back printed nothing for a property receiver,
/// and the element receiver segfaulted.
///
/// The array-element, direct-property and plain-element receivers are covered together,
/// including the two that predate issue #690: the defect was in how a throw decides which loops
/// it leaves, so it reached every by-reference source, not just the ones this change added. The
/// runtime-named receiver is left to its own fixture above — a by-reference call in this one
/// blocks the propagation that folds its name, and an unfolded name is a separate gap.
#[test]
fn test_regression_690_a_throw_caught_inside_the_loop_keeps_iterating() {
    let out = compile_and_run(
        r#"<?php
class C { public array $x = [1, 2, 3]; }
function step(&$v) {
    try {
        if ($v === 2) { throw new RuntimeException('mid'); }
    } catch (RuntimeException $e) {
        echo "caught;";
    }
    $v *= 10;
}

$arr = [new C()];
foreach ($arr[0]->x as &$a) { step($a); }
unset($a);
echo implode(',', $arr[0]->x), "|";

$o = new C();
foreach ($o->x as &$b) { step($b); }
unset($b);
echo implode(',', $o->x), "|";

$plain = [[1, 2, 3]];
foreach ($plain[0] as &$e) { step($e); }
unset($e);
echo implode(',', $plain[0]);
"#,
    );
    assert_eq!(out, "caught;10,20,30|caught;10,20,30|caught;10,20,30");
}

/// Verifies an unmatched catch inside the loop still releases what the loop holds.
///
/// The exception continues outward, so the rethrow DOES leave the loop — the opposite of the
/// fixture above, and the reason the decision cannot simply be "a `try` is active". Leaking there
/// would be one object per throw; releasing twice would free a live one.
#[test]
fn test_regression_690_an_unmatched_catch_rethrows_out_of_the_loop_and_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $x = [1, 2, 3]; }
$hits = 0;
for ($i = 0; $i < 30; $i++) {
    $arr = [new C()];
    try {
        foreach ($arr[0]->x as &$v) {
            try {
                throw new RuntimeException('inner');
            } catch (LogicException $e) {
                echo "wrong;";
            }
        }
    } catch (RuntimeException $e) {
        $hits++;
    }
    unset($v);
    $hits += $arr[0]->x[0];
}
echo $hits;
"#,
    );
    assert_eq!(out.stdout, "60", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("leak summary: clean"),
        "a rethrow that leaves the loop must release exactly once: {}",
        out.stderr
    );
}
