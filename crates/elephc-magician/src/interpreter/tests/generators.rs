//! Purpose:
//! Interpreter tests for PHP generators: the object a function containing `yield` returns, its
//! resumption protocol, key numbering, `yield from` delegation and the errors PHP raises.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::generators`.
//!
//! Key details:
//! - A named `function g() { yield 1; }` used to die with
//!   `call to undefined function __elephc_eval_yield()`, because only closures were lowered and
//!   they were lowered EAGERLY into an `ArrayIterator`, which re-evaluated every yielded
//!   expression and could not model an infinite generator, `send()` or `yield from`.
//! - The key rules are the ones that catch a re-implementation: an explicit key does not advance
//!   the auto-increment counter, `yield from` preserves the INNER keys and leaves the outer
//!   counter alone, and every generator in a delegation chain numbers its own.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse generator fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute generator fragment");
    values.output.clone()
}

/// Runs one fragment expected to raise, returning what it echoed before the throw.
fn throws(fragment: &[u8]) -> (String, EvalStatus) {
    let program = parse_fragment(fragment).expect("parse generator fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let status = execute_program(&program, &mut scope, &mut values)
        .expect_err("fragment should raise");
    (values.output.clone(), status)
}

/// Verifies a named generator function iterates in `foreach`.
///
/// `php -n` 8.5.6 prints `12`. This is the reported failure: a named function containing `yield`
/// reached the runtime with the parser's marker intact and died on an undefined function.
#[test]
fn a_named_generator_function_iterates() {
    assert_eq!(
        out(br#"function g() { yield 1; yield 2; } foreach (g() as $v) { echo $v; }"#),
        "12",
    );
}

/// Verifies auto-increment and explicit keys follow PHP's numbering.
///
/// `php -n` 8.5.6 prints `0=a,k=b,1=c,`: the explicit `'k'` does NOT advance the counter, so the
/// third yield takes key 1 rather than 2.
#[test]
fn explicit_keys_do_not_advance_the_auto_increment_counter() {
    assert_eq!(
        out(
            br#"function k() { yield 'a'; yield 'k' => 'b'; yield 'c'; }
foreach (k() as $key => $v) { echo "$key=$v,"; }"#
        ),
        "0=a,k=b,1=c,",
    );
}

/// Verifies `current()`, `key()`, `next()` and `valid()` drive the generator by hand.
///
/// `php -n` 8.5.6 prints `10;21;done;`.
#[test]
fn the_manual_protocol_walks_the_generator() {
    assert_eq!(
        out(
            br#"function g() { yield 1; yield 2; }
$it = g();
echo $it->current(), $it->key(), ";";
$it->next();
echo $it->current(), $it->key(), ";";
$it->next();
echo $it->valid() ? "valid" : "done", ";";"#
        ),
        "10;21;done;",
    );
}

/// Verifies `getReturn()` hands back what the body returned.
///
/// `php -n` 8.5.6 prints `R`.
#[test]
fn get_return_reports_the_bodys_return_value() {
    assert_eq!(
        out(
            br#"function r() { yield 1; return "R"; }
$x = r();
foreach ($x as $v) {}
echo $x->getReturn();"#
        ),
        "R",
    );
}

/// Verifies `send()` delivers a value to the expression the generator is suspended on.
///
/// `php -n` 8.5.6 prints `1,got:A,2,got:B,`. This needs `yield` in expression position, which
/// the parser refused entirely before this change.
#[test]
fn send_delivers_a_value_into_the_suspended_yield() {
    assert_eq!(
        out(
            br#"function e() { $a = yield 1; echo "got:$a,"; $b = yield 2; echo "got:$b,"; }
$s = e();
echo $s->current(), ",";
$s->send("A");
echo $s->current(), ",";
$s->send("B");"#
        ),
        "1,got:A,2,got:B,",
    );
}

/// Verifies `yield from` over an array preserves the inner keys and leaves the outer counter.
///
/// `php -n` 8.5.6 prints `0:0,0:10,1:20,1:99,`. The delegated array contributes keys 0 and 1 of
/// its own, and the outer generator's counter is untouched by the delegation, so the yield after
/// it takes key 1 rather than 3.
#[test]
fn yield_from_an_array_preserves_inner_keys() {
    assert_eq!(
        out(
            br#"function yf() { yield 0; yield from [10, 20]; yield 99; }
foreach (yf() as $key => $v) { echo "$key:$v,"; }"#
        ),
        "0:0,0:10,1:20,1:99,",
    );
}

/// Verifies a three-level `yield from` chain, where every generator numbers its own keys.
///
/// `php -n` 8.5.6 prints `0/a1,0/b1,0/c1,1/c2,1/b2,1/a2,`.
#[test]
fn a_three_level_yield_from_chain_numbers_each_generator_separately() {
    assert_eq!(
        out(
            br#"function l3() { yield "c1"; yield "c2"; }
function l2() { yield "b1"; yield from l3(); yield "b2"; }
function l1() { yield "a1"; yield from l2(); yield "a2"; }
foreach (l1() as $key => $v) { echo "$key/$v,"; }"#
        ),
        "0/a1,0/b1,0/c1,1/c2,1/b2,1/a2,",
    );
}

/// Verifies a yield inside a `for` and an `if` suspends and resumes correctly.
///
/// `php -n` 8.5.6 prints `024`. The loop counter has to survive each suspension, which is what
/// the frame's own scope is for.
#[test]
fn a_yield_inside_a_loop_and_a_conditional_resumes() {
    assert_eq!(
        out(
            br#"function loopy($n) { for ($i = 0; $i < $n; $i++) { if ($i % 2 == 0) { yield $i; } } }
foreach (loopy(5) as $v) { echo $v; }"#
        ),
        "024",
    );
}

/// Verifies a generator declared as a method works the same way.
///
/// `php -n` 8.5.6 prints `78`.
#[test]
fn a_method_can_be_a_generator() {
    assert_eq!(
        out(
            br#"class C { public function m() { yield 7; yield 8; } }
foreach ((new C())->m() as $v) { echo $v; }"#
        ),
        "78",
    );
}

/// Verifies a generator body does not run until it is first asked for a value.
///
/// `php -n` 8.5.6 prints `before,after,body,` — calling the function produces the object without
/// executing a line of it.
#[test]
fn the_body_does_not_run_until_the_first_request() {
    assert_eq!(
        out(
            br#"function g() { echo "body,"; yield 1; }
echo "before,";
$x = g();
echo "after,";
$x->current();"#
        ),
        "before,after,body,",
    );
}

/// Verifies traversing a finished generator raises PHP's exception.
///
/// `php -n` 8.5.6 reports `Exception: Cannot traverse an already closed generator`.
#[test]
fn traversing_a_closed_generator_raises() {
    let (output, status) = throws(
        br#"function g() { yield 1; yield 2; }
$a = g();
foreach ($a as $v) {}
foreach ($a as $v) {}"#,
    );
    assert_eq!((output.as_str(), status), ("", EvalStatus::UncaughtThrowable));
}

/// Verifies rewinding a generator that has advanced raises PHP's exception.
///
/// `php -n` 8.5.6 reports `Exception: Cannot rewind a generator that was already run`.
#[test]
fn rewinding_an_advanced_generator_raises() {
    let (_, status) = throws(
        br#"function g() { yield 1; yield 2; }
$b = g();
$b->next();
$b->rewind();"#,
    );
    assert_eq!(status, EvalStatus::UncaughtThrowable);
}

/// Verifies `getReturn()` before the generator finished raises PHP's exception.
///
/// `php -n` 8.5.6 reports `Exception: Cannot get return value of a generator that hasn't returned`.
#[test]
fn get_return_before_finishing_raises() {
    let (_, status) = throws(
        br#"function g() { yield 1; yield 2; }
$c = g();
$c->getReturn();"#,
    );
    assert_eq!(status, EvalStatus::UncaughtThrowable);
}

/// Verifies `yield from` over a scalar raises PHP's error.
///
/// `php -n` 8.5.6 reports `Error: Can use "yield from" only with arrays and Traversables`.
#[test]
fn yield_from_a_non_iterable_raises() {
    let (_, status) = throws(
        br#"function bad() { yield from 5; }
foreach (bad() as $v) {}"#,
    );
    assert_eq!(status, EvalStatus::UncaughtThrowable);
}

/// Verifies a closure and an arrow-bodied closure can be generators too.
///
/// `php -n` 8.5.6 prints `56`. Closures used to be lowered EAGERLY into an `ArrayIterator`,
/// which re-evaluated every yielded expression on each call.
#[test]
fn a_closure_can_be_a_generator() {
    assert_eq!(
        out(
            br#"$g = function () { yield 5; yield 6; };
foreach ($g() as $v) { echo $v; }"#
        ),
        "56",
    );
}

/// Verifies a `finally` runs after a `try` body that yielded and completed.
///
/// `php -n` 8.5.6 prints `abfin;after;`: both values come out of the try body, the finally runs
/// when the body ends, and the statement after the try still runs.
#[test]
fn a_finally_runs_after_a_try_body_that_yielded() {
    assert_eq!(
        out(
            br#"function t1() {
    try { yield 'a'; yield 'b'; } finally { echo "fin;"; }
    echo "after;";
}
foreach (t1() as $v) { echo $v; }"#
        ),
        "abfin;after;",
    );
}

/// Verifies a throw raised after a resumption reaches the catch clause covering it.
///
/// `php -n` 8.5.6 prints `1,caught:boom;3,fin;`: the generator suspends inside the `try`, the
/// throw on the NEXT resumption is caught by the clause that was open when it suspended, the
/// catch body yields in its turn, and the finally runs last.
#[test]
fn a_throw_after_a_resumption_reaches_the_catch_that_covered_the_yield() {
    assert_eq!(
        out(
            br#"function t2() {
    try { yield 1; throw new RuntimeException("boom"); yield 2; }
    catch (RuntimeException $e) { echo "caught:"; echo $e->getMessage(); echo ";"; yield 3; }
    finally { echo "fin;"; }
}
foreach (t2() as $v) { echo $v; echo ","; }"#
        ),
        "1,caught:boom;3,fin;",
    );
}

/// Verifies an unmatched throw runs the `finally` and then carries on outward.
///
/// `php -n` 8.5.6 prints `1fin;` and then the exception escapes the generator uncaught.
#[test]
fn an_unmatched_throw_runs_the_finally_before_leaving_the_generator() {
    let (output, status) = throws(
        br#"function t9() {
    try { yield 1; throw new RuntimeException("boom"); }
    catch (LogicException $e) { echo "wrong;"; }
    finally { echo "fin;"; }
}
foreach (t9() as $v) { echo $v; }"#,
    );
    assert_eq!(output, "1fin;");
    assert_eq!(status, EvalStatus::UncaughtThrowable);
}

/// Verifies a `try` opened inside a loop runs its `finally` on every pass.
///
/// `php -n` 8.5.6 prints `1f1;2f2;`.
#[test]
fn a_try_inside_a_loop_runs_its_finally_every_pass() {
    assert_eq!(
        out(
            br#"function t7() {
    foreach ([1, 2] as $i) {
        try { yield $i; } finally { echo "f"; echo $i; echo ";"; }
    }
}
foreach (t7() as $v) { echo $v; }"#
        ),
        "1f1;2f2;",
    );
}

/// Verifies a `return` inside a `try` runs the `finally` and still reports its value.
///
/// `php -n` 8.5.6 prints `1fin8;ret=R`: the returned expression is evaluated first, the finally
/// runs next, and `getReturn()` still answers what the `return` computed.
#[test]
fn a_return_inside_a_try_runs_the_finally_and_keeps_its_value() {
    assert_eq!(
        out(
            br#"function t8() {
    try { yield 1; return 'R'; } finally { echo "fin8;"; }
}
$g = t8();
foreach ($g as $v) { echo $v; }
echo "ret="; echo $g->getReturn();"#
        ),
        "1fin8;ret=R",
    );
}

/// Verifies a `break` out of a loop runs the `finally` of the `try` it leaves.
///
/// `php -n` 8.5.6 prints `1fin;|after` for the same shape: leaving the loop leaves the try, so
/// its finally runs on the way out.
#[test]
fn a_break_out_of_a_try_inside_a_loop_runs_the_finally() {
    assert_eq!(
        out(
            br#"function t10() {
    foreach ([1, 2] as $i) {
        try { yield $i; if ($i == 1) { break; } } finally { echo "fin;"; }
    }
    echo "|after";
}
foreach (t10() as $v) { echo $v; }"#
        ),
        "1fin;|after",
    );
}

/// Verifies abandoning a generator mid-iteration still runs the `finally` it was suspended in.
///
/// `php -n` 8.5.6 prints `1fin3;|after` for this shape: dropping the last reference to a
/// suspended generator runs the `finally` of the `try` its yield was inside.
///
/// The reference is dropped with `unset` rather than by abandoning a `foreach`, because a
/// `foreach` subject that is a temporary is never released by the loop at all — a pre-existing
/// ownership gap that predates generators and would make this test measure that instead.
#[test]
fn abandoning_a_generator_runs_the_finally_it_was_suspended_in() {
    assert_eq!(
        out(
            br#"function t3() {
    try { yield 1; yield 2; } finally { echo "fin3;"; }
}
$g = t3();
echo $g->current();
unset($g);
echo "|after";"#
        ),
        "1fin3;|after",
    );
}

/// Verifies nested `finally` blocks run innermost first when a generator is abandoned.
///
/// `php -n` 8.5.6 prints `1inner;outer;|after2`.
#[test]
fn abandoning_a_generator_unwinds_nested_finallys_innermost_first() {
    assert_eq!(
        out(
            br#"function nested() {
    try { try { yield 1; } finally { echo "inner;"; } } finally { echo "outer;"; }
}
$h = nested();
echo $h->current();
unset($h);
echo "|after2";"#
        ),
        "1inner;outer;|after2",
    );
}

/// Verifies a generator that never started runs no `finally` when it is destroyed.
///
/// `php -n` 8.5.6 prints `created;|after3`: the body never entered the `try`, so there is
/// nothing to unwind.
#[test]
fn destroying_a_generator_that_never_started_runs_no_finally() {
    assert_eq!(
        out(
            br#"function unfinished() {
    try { yield 1; } finally { echo "never-abandoned;"; }
}
$g = unfinished();
echo "created;";
unset($g);
echo "|after3";"#
        ),
        "created;|after3",
    );
}

/// Verifies a `switch` inside a generator matches, falls through and breaks as PHP does.
///
/// `php -n` 8.5.6 prints `two,two-b,end,|other,end,` — the matched arm runs, `break` leaves the
/// switch rather than the generator, and the statement after the switch still yields.
#[test]
fn a_switch_arm_can_yield_and_break_leaves_only_the_switch() {
    assert_eq!(
        out(
            br#"function s1($n) {
    switch ($n) {
        case 1: yield 'one'; break;
        case 2: yield 'two'; yield 'two-b'; break;
        default: yield 'other';
    }
    yield 'end';
}
foreach (s1(2) as $v) { echo $v; echo ","; }
echo "|";
foreach (s1(9) as $v) { echo $v; echo ","; }"#
        ),
        "two,two-b,end,|other,end,",
    );
}

/// Verifies an arm without `break` falls into the next arm's body, and a miss runs nothing.
///
/// `php -n` 8.5.6 prints `low,three,|four,|`: `case 1` is empty and falls into `case 2`, whose
/// body falls on into `case 3` because neither breaks; `case 4` is last and yields alone; and
/// `7` matches no arm at all in a switch with no `default`.
#[test]
fn a_switch_arm_without_break_falls_into_the_next_one() {
    assert_eq!(
        out(
            br#"function s2($n) {
    switch ($n) {
        case 1:
        case 2: yield 'low';
        case 3: yield 'three'; break;
        case 4: yield 'four';
    }
}
foreach (s2(1) as $v) { echo $v; echo ","; }
echo "|";
foreach (s2(4) as $v) { echo $v; echo ","; }
echo "|";
foreach (s2(7) as $v) { echo $v; echo ","; }"#
        ),
        "low,three,|four,|",
    );
}

/// Verifies `continue 2` counts the switch as a level and reaches the enclosing loop.
///
/// `php -n` 8.5.6 prints `13`: the switch is one level, so `continue 2` skips the rest of the
/// loop body for `2` only.
#[test]
fn continue_two_from_a_switch_reaches_the_enclosing_loop() {
    assert_eq!(
        out(
            br#"function s3() {
    foreach ([1, 2, 3] as $i) {
        switch ($i) {
            case 2: continue 2;
        }
        yield $i;
    }
}
foreach (s3() as $v) { echo $v; }"#
        ),
        "13",
    );
}

/// Verifies a `match` arm can be the yield that suspends, and `send()` becomes the arm's value.
///
/// `php -n` 8.5.6 prints `a,r=S` for the matched arm and `d,r=T` for the default: the generator
/// suspends inside the arm, and what `send()` delivers is what the whole `match` evaluates to.
#[test]
fn a_match_arm_can_be_the_yield_that_suspends() {
    assert_eq!(
        out(
            br#"function m1($n) {
    $r = match ($n) { 1 => yield 'a', 2 => yield 'b', default => yield 'd' };
    yield 'r=' . $r;
}
$g = m1(1);
echo $g->current(); echo ",";
$g->send('S');
echo $g->current(); echo ";";
$h = m1(7);
echo $h->current(); echo ",";
$h->send('T');
echo $h->current();"#
        ),
        "a,r=S;d,r=T",
    );
}

/// Verifies an arm with no yield still produces its value, and a miss raises PHP's error.
///
/// `php -n` 8.5.6 prints `r=plain` for the plain arm and raises
/// `UnhandledMatchError: Unhandled match case 9` when nothing matches.
#[test]
fn a_match_without_a_matching_arm_raises_unhandled_match_error() {
    assert_eq!(
        out(
            br#"function m2($n) {
    $r = match ($n) { 1 => yield 'x', 2 => 'plain' };
    yield 'r=' . $r;
}
$g = m2(2);
echo $g->current();"#
        ),
        "r=plain",
    );
    let (output, status) = throws(
        br#"function m3($n) {
    $r = match ($n) { 1 => yield 'x', 2 => 'plain' };
    yield 'r=' . $r;
}
$g = m3(9);
echo $g->current();"#,
    );
    assert_eq!(output, "");
    assert_eq!(status, EvalStatus::UncaughtThrowable);
}

/// Verifies a generator method reads `$this->property` AND calls another `$this` method,
/// resumed across suspensions -- the receiver has to survive past the call that created the
/// generator, which is what `eval_dynamic_method_with_values_and_ref_mode` retains it for
/// (`ScopeCellOwnership::Owned` instead of `Borrowed` when the body is a generator).
///
/// `php -n` 8.5.6 prints `11,21,31,`.
#[test]
fn a_generator_method_reads_this_property_and_calls_a_this_method_across_resumes() {
    assert_eq!(
        out(
            br#"class Box {
    private array $items = [10, 20, 30];
    public function items() {
        foreach ($this->items as $i) {
            yield $i + $this->bonus();
        }
    }
    private function bonus(): int { return 1; }
}
$b = new Box();
foreach ($b->items() as $v) { echo $v, ","; }"#
        ),
        "11,21,31,",
    );
}

/// Verifies the object a generator function returns answers `instanceof` for `Generator`,
/// `Traversable` and `Iterator` -- the interfaces PHP wires it through.
///
/// `php -n` 8.5.6 prints `GTI`: all three checks are true.
#[test]
fn a_generator_object_is_instanceof_generator_traversable_and_iterator() {
    assert_eq!(
        out(
            br#"function g() { yield 1; }
$x = g();
echo $x instanceof Generator ? "G" : "-";
echo $x instanceof Traversable ? "T" : "-";
echo $x instanceof Iterator ? "I" : "-";"#
        ),
        "GTI",
    );
}

/// Verifies a bare `yield;` (no key, no value) yields `null` and still takes the next
/// auto-increment key, exactly like a `yield` with a value.
///
/// `php -n` 8.5.6 prints `0=1;1=null;2=3;`.
#[test]
fn a_bare_yield_produces_null_and_still_advances_the_auto_increment_key() {
    assert_eq!(
        out(
            br#"function g() { yield 1; yield; yield 3; }
foreach (g() as $k => $v) { echo $k, "=", $v ?? "null", ";"; }"#
        ),
        "0=1;1=null;2=3;",
    );
}

/// Verifies a `static` generator method resolves late static binding (`static::class`) from
/// the CALLED class, not the declaring one.
///
/// `php -n` 8.5.6 prints `1,D,`: the second yield reports `D`, the subclass through which the
/// static method was called, not `C` where it is declared.
#[test]
fn a_static_generator_method_resolves_late_static_binding() {
    assert_eq!(
        out(
            br#"class C {
    public static function make() {
        yield 1;
        yield static::class;
    }
}
class D extends C {}
foreach (D::make() as $v) { echo $v, ","; }"#
        ),
        "1,D,",
    );
}

/// Verifies an exception thrown directly from a generator body -- with no enclosing try/catch
/// in the body itself -- propagates out through the consuming `foreach` to a catch that wraps
/// the loop, exactly like an ordinary function call that throws.
///
/// `php -n` 8.5.6 prints `1,caught:boom`: the first yielded value runs one loop iteration, then
/// the thrown exception unwinds the foreach into the surrounding catch.
#[test]
fn an_uncaught_throw_in_the_body_propagates_to_the_consumers_catch() {
    assert_eq!(
        out(
            br#"function g() {
    yield 1;
    throw new RuntimeException("boom");
    yield 2;
}
try {
    foreach (g() as $v) { echo $v, ","; }
} catch (RuntimeException $e) {
    echo "caught:", $e->getMessage(), "\n";
}"#
        ),
        "1,caught:boom\n",
    );
}
