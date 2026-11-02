//! Purpose:
//! Interpreter tests for php's `static $x` STORAGE model: one slot per function, shared by every
//! activation of it -- including one already on the stack.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::static_locals`.
//!
//! Key details:
//! - A static used to be COPIED into the activation and written back on return, so a recursive
//!   call read the value the frame below it started with. `function f() { static $a = 1; echo $a;
//!   $a++; f(); }` printed `1` forever instead of counting, and ran until the heap gave out.
//! - The slot now lives in one scope the context owns, reached the way the GLOBAL scope already
//!   is -- by raw pointer -- because a write arrives through `set_scope_cell`, which only ever
//!   holds a shared context. `RefCell` was the first attempt and is wrong: it is not
//!   `RefUnwindSafe`, and the FFI crosses a hundred `catch_unwind` boundaries holding this
//!   context.
//! - Six behaviours were measured separately rather than treated as one thing, and four of them
//!   were ALREADY right. Only recursion and closure-object identity were broken. The four that
//!   worked are pinned here too, because the change moved the storage under all six.
//! - `php -n` 8.5.6 prints `1 2 3 4 5 |123|123|121|12|3` for the whole set.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse static locals fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute static locals fragment");
    values.output.clone()
}

/// Verifies recursion shares ONE slot.
///
/// `php -n` 8.5.6 prints `1 2 3 4 5 `. This is the case the whole change exists for: every frame
/// must see what the frame below it wrote, which a copy taken at activation cannot do. Before, the
/// guard `$a < 6` was never satisfied because `$a` was 1 in every frame, so this fragment did not
/// merely print the wrong thing -- it recursed until the heap was exhausted.
#[test]
fn recursion_shares_one_slot() {
    assert_eq!(
        out(
            br#"function rec() {
    static $a = 1;
    echo $a, " ";
    $a++;
    if ($a < 6) { rec(); }
}
rec();"#
        ),
        "1 2 3 4 5 ",
    );
}

/// Verifies the frame that started the recursion reads what the deepest frame wrote.
///
/// `php -n` 8.5.6 prints `3`. The other direction of the same rule: a WRITE reaching the slot is
/// not enough if the outer frame then reads a stale copy. An implementation that persisted on
/// return would pass the counting test above and fail this one.
#[test]
fn the_outer_frame_reads_what_the_recursion_wrote() {
    assert_eq!(
        out(
            br#"function outer2() {
    static $s = 0;
    $s++;
    if ($s < 3) { outer2(); }
    return $s;
}
echo outer2();"#
        ),
        "3",
    );
}

/// Verifies each closure OBJECT gets its own slot.
///
/// `php -n` 8.5.6 prints `121`: `$f` counts 1 then 2, and `$g` -- a second closure from the same
/// factory -- starts again at 1. The slot key used to be the parser's name for the closure
/// LITERAL, which every object made from it shares, so the two counted together and printed `123`.
/// The key is now the unique name `define_closure` mints per object.
#[test]
fn each_closure_object_gets_its_own_slot() {
    assert_eq!(
        out(
            br#"$make = function () {
    return function () { static $c = 0; $c++; return $c; };
};
$f = $make();
$g = $make();
echo $f(), $f(), $g();"#
        ),
        "121",
    );
}

/// Verifies the three slot identities that were already correct, which the change had to preserve.
///
/// `php -n` 8.5.6 prints `123|123|12`:
/// - a plain function keeps its slot across separate calls;
/// - `Child::tick()` returns 3, so an inherited static method shares the DECLARING method's slot
///   rather than getting one per class -- measured, because "per class" would have been the
///   natural guess and is wrong since php 8.1;
/// - two instances share one slot for a non-static method, because the slot belongs to the
///   method and not to the object.
#[test]
fn function_method_and_inheritance_slot_identities_are_unchanged() {
    assert_eq!(
        out(
            br#"function counter() { static $n = 0; $n++; return $n; }
echo counter(), counter(), counter(), "|";
class Base { public static function tick() { static $t = 0; $t++; return $t; } }
class Child extends Base {}
echo Base::tick(), Base::tick(), Child::tick(), "|";
class Holder { public function bump() { static $b = 0; $b++; return $b; } }
$h1 = new Holder();
$h2 = new Holder();
echo $h1->bump(), $h2->bump();"#
        ),
        "123|123|12",
    );
}

/// Verifies the slot OWNS its cell and hands the displaced one back.
///
/// The counting fixture panics on an over-release naming the handle, so a slot that stored a cell
/// it did not own -- or dropped one without giving it back -- fails here rather than corrupting a
/// later run. The static is reassigned repeatedly on purpose: every reassignment displaces the
/// previous cell, which is the moment ownership is either handled or lost.
#[test]
fn the_slot_owns_its_cell_across_reassignment() {
    assert_eq!(
        out(
            br#"function churn($v) {
    static $held = "seed";
    $previous = $held;
    $held = $v . "!";
    return $previous;
}
echo churn("a"), churn("b"), churn("c");"#
        ),
        "seeda!b!",
    );
}
