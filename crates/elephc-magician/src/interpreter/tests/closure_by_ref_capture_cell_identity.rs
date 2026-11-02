//! Purpose:
//! Pins `use (&$x)` closure captures to the CELL `$x` names at closure-creation time, not the
//! NAME `$x`: rebinding or unsetting `$x` afterward must not affect the closure's own captured
//! storage, and multiple closures created from the same variable name across loop iterations
//! must never contaminate each other's captured cell.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::closure_by_ref_capture_cell_identity`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The bug: `eval_call_arg_value`'s `LoadVar` arm always manufactured a fresh
//!   `EvalReferenceTarget::Variable { scope, name }` node for a by-reference capture, even when
//!   `name` was already itself a reference (bound via `$x = &$arr[];` or a by-reference
//!   `foreach`). Write-back then re-resolved the caller-side target by NAME at call time instead
//!   of by the CELL captured at closure-creation time, so a later rebind or unset of that name
//!   redirected -- or lost -- a write the closure had already earned. This is the read/capture
//!   mirror of `90c3d5a5ec`'s write-side fix (`write_back_method_ref_target`'s `Variable` arm
//!   now follows `scope.reference_target(name)` before writing).

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse closure by-ref capture fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute closure by-ref capture fragment");
    values.output.clone()
}

/// Verifies the baseline shape: a closure capturing `&$closure`, where `$closure` is itself a
/// reference to an appended array slot, replaces itself in that slot on first call.
///
/// `php -n` 8.5.6 prints `call1=first:x` then `call2=later:y`.
#[test]
fn a_single_by_ref_capture_self_replaces_through_its_appended_slot() {
    assert_eq!(
        out(
            br#"$optimized = [];
$closure = &$optimized[];
$closure = static function (string $a) use (&$closure): string {
    $closure = static fn (string $b): string => "later:" . $b;
    return "first:" . $a;
};
echo "call1=", $optimized[0]("x"), "\n";
echo "call2=", $optimized[0]("y");"#
        ),
        "call1=first:x\ncall2=later:y",
    );
}

/// Verifies `unset($closure)` after capture does not sever the closure's OWN hold on the cell
/// it captured: the self-replacement written during call 1 must still reach slot 0 for call 2.
///
/// `php -n` 8.5.6 prints `call1=first:x` then `call2=later:y`. elephc's capture followed the
/// NAME `closure`, which `unset` had already detached from the appended slot by the time
/// write-back ran, so the self-replacement was silently lost and call 2 re-ran the FIRST
/// closure's body instead of the second, printing `call2=first:y`.
#[test]
fn a_by_ref_capture_survives_unset_of_the_alias_that_named_it() {
    assert_eq!(
        out(
            br#"$optimized = [];
$closure = &$optimized[];
$closure = static function (string $a) use (&$closure): string {
    $closure = static fn (string $b): string => "later:" . $b;
    return "first:" . $a;
};
unset($closure);
echo "call1=", $optimized[0]("x"), "\n";
echo "call2=", $optimized[0]("y");"#
        ),
        "call1=first:x\ncall2=later:y",
    );
}

/// Verifies rebinding `$closure` to a NEW appended slot on a second loop iteration does not
/// redirect the FIRST closure's already-captured reference to the new slot: each closure must
/// keep writing back to the slot it was created for.
///
/// `php -n` 8.5.6 prints `a1=first1:x a2=later1:y b1=first2:x b2=later2:y`. elephc's capture
/// resolved the caller-side target by re-reading the NAME `closure` at write-back time, so by
/// the second iteration `closure` named slot 1 -- the FIRST closure's self-replacement during
/// call `a1` landed in slot 1 instead of slot 0, producing `a2=first1:y` (slot 0 untouched) and
/// `b1=later1:x` (slot 1 clobbered by the wrong closure).
#[test]
fn rebinding_the_alias_across_loop_iterations_does_not_cross_contaminate_slots() {
    assert_eq!(
        out(
            br#"$optimized = [];
foreach ([1, 2] as $n) {
    $closure = &$optimized[];
    $closure = static function (string $a) use (&$closure, $n): string {
        $closure = static fn (string $b): string => "later" . $n . ":" . $b;
        return "first" . $n . ":" . $a;
    };
}
echo "a1=", $optimized[0]("x"), " ";
echo "a2=", $optimized[0]("y"), " ";
echo "b1=", $optimized[1]("x"), " ";
echo "b2=", $optimized[1]("y");"#
        ),
        "a1=first1:x a2=later1:y b1=first2:x b2=later2:y",
    );
    // Explicit cross-contamination check beyond the flat string comparison above: slot 0's
    // replacement must be built from n=1, slot 1's from n=2 -- neither may hold the other's.
    let first_slot_replacement = out(
        br#"$optimized = [];
foreach ([1, 2] as $n) {
    $closure = &$optimized[];
    $closure = static function (string $a) use (&$closure, $n): string {
        $closure = static fn (string $b): string => "later" . $n . ":" . $b;
        return "first" . $n . ":" . $a;
    };
}
$optimized[0]("prime");
echo $optimized[0]("check");"#,
    );
    assert_eq!(first_slot_replacement, "later1:check");
}

/// Verifies a captured second alias (a by-reference `foreach` loop variable, itself a reference)
/// re-entering the OLD closure body on a stale target does not clobber a later slot either: two
/// by-reference captures inside one loop iteration must both resolve to their own cell.
///
/// `php -n` 8.5.6 prints `call1=first:x:made` then `call2=later:y`. elephc's stale write-back
/// for `&$closure` left `$optimized[0]` holding the FIRST closure, so `call2` re-entered its
/// body, which then called the by-then-string `$listener[0]` as a function and fataled with an
/// unnamed `eval() fragment uses an unsupported construct` -- a downstream symptom of the same
/// root cause, not a second mechanism.
#[test]
fn two_by_ref_captures_in_one_loop_iteration_self_replace_without_reinvoking_the_old_closure() {
    assert_eq!(
        out(
            br#"$listeners = [[static fn (): string => "made", "call"]];
$optimized = [];
foreach ($listeners as &$listener) {
    $closure = &$optimized[];
    $closure = static function (string $a) use (&$listener, &$closure): string {
        $listener[0] = ($listener[0])();
        $closure = static fn (string $b): string => "later:" . $b;
        return "first:" . $a . ":" . $listener[0];
    };
}
unset($listener);
unset($closure);
echo "call1=", $optimized[0]("x"), "\n";
echo "call2=", $optimized[0]("y");"#
        ),
        "call1=first:x:made\ncall2=later:y",
    );
}

/// Verifies the captured cell can be an appended element of a NESTED (multi-level) array, not
/// just a top-level one -- the shape a property array like `$this->optimized[$eventName][]`
/// reduces to once the object-property layer is stripped away.
///
/// `php -n` 8.5.6 prints `call1=first:x` then `call2=later:y`.
#[test]
fn a_by_ref_capture_of_a_nested_array_appended_element_survives_unset() {
    assert_eq!(
        out(
            br#"$boxes = ["bucket" => []];
$closure = &$boxes["bucket"][];
$closure = static function (string $a) use (&$closure): string {
    $closure = static fn (string $b): string => "later:" . $b;
    return "first:" . $a;
};
unset($closure);
echo "call1=", $boxes["bucket"][0]("x"), "\n";
echo "call2=", $boxes["bucket"][0]("y");"#
        ),
        "call1=first:x\ncall2=later:y",
    );
}

/// Verifies a by-reference capture sees the defining variable's CURRENT value: a reassignment
/// made after the closure was created, and every write from earlier calls.
///
/// `php -n` 8.5.6 prints `5 3 2,3,3 10`. elephc bound the cell captured at creation on every
/// call, so later calls restarted from it (`2`, `2,2,2`), and a reassignment in the defining
/// function released that cell underneath the closure.
#[test]
fn a_by_ref_capture_reads_the_defining_variable_as_it_is_now() {
    assert_eq!(
        out(
            br#"function rebind() { $n = 1; $c = function () use (&$n) { return $n; }; $n = 5; return $c(); }
function inc() { $n = 1; $c = function () use (&$n) { $n++; }; $c(); $c(); return $n; }
function incread() { $n = 1; $c = function () use (&$n) { return ++$n; }; $a = $c(); $b = $c(); return "$a,$b,$n"; }
function two() { $n = 0; $a = function () use (&$n) { $n += 10; }; $b = function () use (&$n) { return $n; }; $a(); return $b(); }
echo rebind(), " ", inc(), " ", incread(), " ", two();"#
        ),
        "5 3 2,3,3 10",
    );
}

/// Verifies a by-reference capture outlives the function that declared the variable, keeping
/// its own value across calls -- `LazyString::fromCallable()`'s `use (&$callback, &$arguments)`.
///
/// `php -n` 8.5.6 prints `123 string:abc x!`. elephc kept a raw pointer to the returned
/// function's scope and wrote back through it after every call.
#[test]
fn a_by_ref_capture_outlives_its_defining_function() {
    assert_eq!(
        out(
            br#"function counter() { $n = 0; return function () use (&$n) { return ++$n; }; }
function peek($s) { return static function () use (&$s) { return gettype($s) . ":" . (is_string($s) ? $s : "?"); }; }
function holder($cb, ...$args) { return static function () use (&$cb, &$args) { $r = $cb(...$args); $cb = "done"; $args = null; return $r; }; }
$c = counter();
$p = peek("abc");
$h = holder(fn($a) => $a . "!", "x");
echo $c(), $c(), $c(), " ", $p(), " ", $h();"#
        ),
        "123 string:abc x!",
    );
}
