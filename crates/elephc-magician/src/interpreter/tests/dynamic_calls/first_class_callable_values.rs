//! Purpose:
//! Pins `EXPR(...)` first-class-callable syntax for every runtime VALUE shape `EXPR` can
//! evaluate to, not just the syntactic forms the parser special-cases (plain function name,
//! `$obj->method`, `Class::method`).
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::dynamic_calls::first_class_callable_values`.
//!
//! Key details:
//! - `plain(...)`, `$obj->m(...)`, `Cls::s(...)`, `$obj(...)` on an invokable, and
//!   `strtoupper(...)` already worked and stay pinned here as a regression guard alongside the
//!   three shapes that did not: `$closureVar(...)` where the variable already holds a Closure,
//!   `$arrayCallableVar(...)` where the variable holds `[$obj, "method"]`, and
//!   `$stringNameVar(...)` where the variable holds a function name string. All eight are
//!   PHP `Closure` objects with `php -n` 8.5.6.
//! - The bug: every `EXPR(...)` unconditionally built an `InvokableObject { object }` closure
//!   target regardless of what `EXPR` evaluated to. For a Closure-valued variable, calling the
//!   result asked the runtime for the wrapped object's native class -- the placeholder
//!   `stdClass` the vessel is allocated as, never `Closure` -- and raised `Error: Object of type
//!   stdClass is not callable`. For an array or string callable, dispatch had no case for a
//!   non-object "invokable object" at all and failed with `unsupported DynamicCall expression`.

use super::super::super::*;
use super::super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse first-class-callable-values fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute first-class-callable-values fragment");
    values.output.clone()
}

/// Verifies every `EXPR(...)` shape produces a working `Closure`, working and broken forms
/// together so a fix that repairs one by breaking another cannot pass.
///
/// Expected values are `php -n` 8.5.6's exact output on the same fragment (measured against the
/// probe fixtures under `scratchpad/r200/p5.php` and `p6.php`).
#[test]
fn execute_program_first_class_callable_syntax_covers_every_runtime_value_shape() {
    assert_eq!(
        out(
            br#"function eval_fcc_val_plain($t) { return "plain:" . $t; }
class EvalFccValHolder {
    public function m($t) { return "m:" . $t; }
    public static function s($t) { return "s:" . $t; }
    public function __invoke($t) { return "inv:" . $t; }
}
$h = new EvalFccValHolder();

$a = eval_fcc_val_plain(...);
echo "fn:" . get_class($a) . ":" . $a("1") . "|";

$b = $h->m(...);
echo "method:" . get_class($b) . ":" . $b("2") . "|";

$c = EvalFccValHolder::s(...);
echo "static:" . get_class($c) . ":" . $c("3") . "|";

$d = $h(...);
echo "invokable:" . get_class($d) . ":" . $d("4") . "|";

$g = strtoupper(...);
echo "builtin:" . get_class($g) . ":" . $g("z") . "|";

$arrow = static function ($t) { return "arrow:" . $t; };
$closureVar = $arrow;
$e = $closureVar(...);
echo "closurevar:" . get_class($e) . ":" . $e("6") . "|";

$pair = [$h, "m"];
$arrayCallableVar = $pair;
$f = $arrayCallableVar(...);
echo "arrayvar:" . get_class($f) . ":" . $f("7") . "|";

$name = "strtolower";
$stringNameVar = $name;
$i = $stringNameVar(...);
echo "stringvar:" . get_class($i) . ":" . $i("EIGHT");"#
        ),
        "fn:Closure:plain:1|\
method:Closure:m:2|\
static:Closure:s:3|\
invokable:Closure:inv:4|\
builtin:Closure:Z|\
closurevar:Closure:arrow:6|\
arrayvar:Closure:m:7|\
stringvar:Closure:eight",
    );
}

/// Verifies calling a first-class callable made from a Closure-valued variable still runs the
/// SAME underlying closure logic (not a new, disconnected one) after being reassigned to yet
/// another variable and invoked through a normal dynamic call, matching the shape
/// `EventDispatcher::optimizeListeners()` uses: `($closure = $listener(...))(...$args)`.
#[test]
fn execute_program_first_class_callable_from_closure_variable_reassigns_and_calls() {
    assert_eq!(
        out(
            br#"$listener = static function (string $t): string { return "high:" . $t; };
($closure = $listener(...))("x");
echo $closure("x");"#
        ),
        "high:x",
    );
}
