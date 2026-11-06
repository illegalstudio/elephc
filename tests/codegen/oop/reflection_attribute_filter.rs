//! Purpose:
//! End-to-end tests for the `$name` filter on `ReflectionX::getAttributes()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The synthesized method used to declare NO parameters, so `getAttributes(A::class)`
//!   was refused as "expects 0 arguments, got 1" in AOT mode and the eval bridge answered
//!   with every attribute on the target (issue #983).
//! - Every expected string here is real `LC_ALL=C php` 8.5 output.
//! - `$flags` is declared for signature parity and `0` is honoured. `ReflectionAttribute::
//!   IS_INSTANCEOF` would need a subclass test on a runtime class name that no AOT builtin answers
//!   (#1113), so a call that provably requests it for a name is a compile error, and one that
//!   only requests it at run time throws a `ReflectionException` from the synthesized body.
//! - The compile-time refusal is exercised in every spelling that reaches the flag (positional,
//!   named, through a literal spread, and on a `mixed` receiver), because the first cut read
//!   `args[1]` on a named-class receiver only.
//! - Every spelling the checker cannot fold (a runtime flag, a spread of a runtime array, a
//!   first-class callable, `call_user_func_array`, a dynamic method name, which the parser
//!   desugars to `call_user_func`) reaches the body's run-time checks: a `0` filters, `2` with a
//!   non-null name throws, and anything but `0` and `2` raises PHP's
//!   `ValueError: <Owner>::getAttributes(): Argument #2 ($flags) must be a valid attribute filter
//!   flag` (measured with 1, 3, 4 and -1, whatever `$name` holds).

use super::*;

/// The reported repro: the filter must select by exact attribute class name, and a name that
/// matches nothing must come back empty rather than as the whole set.
#[test]
fn test_get_attributes_filters_class_attributes_by_name() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker { public function __construct(public string $v = "x") {} }
#[Attribute] class Other {}

#[Marker("one")]
#[Other]
class Target {}

$r = new ReflectionClass(Target::class);
echo "all:", count($r->getAttributes()), "\n";
echo "filtered:", count($r->getAttributes(Marker::class)), "\n";
echo "other:", count($r->getAttributes(Other::class)), "\n";
echo "none:", count($r->getAttributes("Nope")), "\n";
"#,
    );
    assert_eq!(out, "all:2\nfiltered:1\nother:1\nnone:0\n");
}

/// A filtered element is a real `ReflectionAttribute`, not a repackaged payload: it still
/// answers `getName()` and can still build the attribute instance.
#[test]
fn test_filtered_attribute_still_answers_name_and_new_instance() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker { public function __construct(public string $v = "x") {} }
#[Attribute] class Other {}

#[Marker("one")]
#[Other]
class Target {}

$r = new ReflectionClass(Target::class);
$f = $r->getAttributes(Marker::class);
echo get_class($f[0]), "|", $f[0]->getName(), "|", $f[0]->newInstance()->v, "\n";
"#,
    );
    assert_eq!(out, "ReflectionAttribute|Marker|one\n");
}

/// An unfiltered call is unchanged: the same array, in declaration order.
#[test]
fn test_get_attributes_without_name_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

#[Marker]
#[Other]
class Target {}

$all = (new ReflectionClass(Target::class))->getAttributes();
echo count($all), "|", $all[0]->getName(), $all[1]->getName(), "\n";
"#,
    );
    assert_eq!(out, "2|MarkerOther\n");
}

/// A repeatable attribute matches as many times as it appears.
#[test]
fn test_get_attributes_filter_keeps_repeated_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::IS_REPEATABLE)] class Tag {}

#[Tag]
#[Tag]
class Target {}

$r = new ReflectionClass(Target::class);
echo "all:", count($r->getAttributes()), "|one:", count($r->getAttributes(Tag::class)), "\n";
"#,
    );
    assert_eq!(out, "all:2|one:2\n");
}

/// A class carrying no attributes answers empty either way.
#[test]
fn test_get_attributes_filter_on_class_without_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}

class Target {}

$r = new ReflectionClass(Target::class);
echo count($r->getAttributes()), "|", count($r->getAttributes(Marker::class)), "\n";
"#,
    );
    assert_eq!(out, "0|0\n");
}

/// `ReflectionMethod` owns the same filter.
#[test]
fn test_get_attributes_filter_on_method() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

class Target { #[Marker] #[Other] public function m(): void {} }

$r = new ReflectionMethod(Target::class, "m");
echo "all:", count($r->getAttributes()), "|filtered:", count($r->getAttributes(Marker::class)), "\n";
"#,
    );
    assert_eq!(out, "all:2|filtered:1\n");
}

/// `ReflectionProperty` owns the same filter.
#[test]
fn test_get_attributes_filter_on_property() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

class Target { #[Marker] #[Other] public int $p = 1; }

$r = new ReflectionProperty(Target::class, "p");
echo "all:", count($r->getAttributes()), "|filtered:", count($r->getAttributes(Marker::class)), "\n";
"#,
    );
    assert_eq!(out, "all:2|filtered:1\n");
}

/// `ReflectionFunction` owns the same filter.
#[test]
fn test_get_attributes_filter_on_function() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

#[Marker] #[Other] function target(): void {}

$r = new ReflectionFunction("target");
echo "all:", count($r->getAttributes()), "|filtered:", count($r->getAttributes(Marker::class)), "\n";
"#,
    );
    assert_eq!(out, "all:2|filtered:1\n");
}

/// PHP folds ASCII case when it compares the filter to the attribute's class name, the way it
/// compares every class name — but it does NOT resolve a leading separator. Measured on 8.5.10:
/// `markerone` and `MARKERONE` both find `#[MarkerOne]`, and `\MarkerOne` finds nothing.
#[test]
fn test_get_attributes_filter_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class MarkerOne {}

#[MarkerOne]
class Target {}

$r = new ReflectionClass(Target::class);
echo "exact:", count($r->getAttributes("MarkerOne")), "\n";
echo "lower:", count($r->getAttributes("markerone")), "\n";
echo "upper:", count($r->getAttributes("MARKERONE")), "\n";
echo "slash:", count($r->getAttributes("\\MarkerOne")), "\n";
"#,
    );
    assert_eq!(out, "exact:1\nlower:1\nupper:1\nslash:0\n");
}

/// An explicit `$flags = 0` is PHP's default and stays accepted; it is the value the
/// synthesized body implements.
#[test]
fn test_get_attributes_filter_accepts_explicit_zero_flags() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}

#[Marker]
class Target {}

echo count((new ReflectionClass(Target::class))->getAttributes(Marker::class, 0)), "\n";
"#,
    );
    assert_eq!(out, "1\n");
}

/// The filter reaches the eval bridge too, which is the path the issue measured: there the
/// Reflection owner is materialized by libelephc-magician rather than by the AOT emitter, and
/// its `__attrs` array carries boxed Mixed elements instead of bare object pointers.
#[test]
fn test_get_attributes_filter_through_eval() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker { public function __construct(public string $v = "x") {} }
#[Attribute] class Other {}

#[Marker("one")]
#[Other]
class Target {}

eval('$r = new ReflectionClass("Target"); echo "all:", count($r->getAttributes()), "|filtered:", count($r->getAttributes("Marker")), "\n";');
"#,
    );
    assert_eq!(out, "all:2|filtered:1\n");
}

/// A namespaced attribute keeps the name PHP reports, with no leading separator, so the filter
/// matches the `::class` constant and the plain literal but not a leading-separator spelling.
#[test]
fn test_get_attributes_filter_matches_a_namespaced_attribute() {
    let out = compile_and_run(
        r#"<?php
namespace App;

use Attribute;

#[Attribute] class Marker {}

#[\App\Marker]
class Target {}

$r = new \ReflectionClass(Target::class);
echo "fqcn:", count($r->getAttributes(Marker::class)), "\n";
echo "literal:", count($r->getAttributes("App\\Marker")), "\n";
echo "leading:", count($r->getAttributes("\\App\\Marker")), "\n";
echo "name0:", $r->getAttributes()[0]->getName(), "\n";
"#,
    );
    assert_eq!(out, "fqcn:1\nliteral:1\nleading:0\nname0:App\\Marker\n");
}

/// PHP ignores `$flags` entirely when `$name` is null — nothing is filtered, so there is no
/// subclass test to do. That spelling stays accepted and answers with everything.
#[test]
fn test_get_attributes_null_name_ignores_the_flag() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
echo count($r->getAttributes(null, ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// Named arguments put `$flags` at either index, so the guard matches by parameter name rather
/// than position — and `flags: 0` is still the value the body implements.
#[test]
fn test_get_attributes_filter_accepts_named_arguments() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

#[Marker]
#[Other]
class Target {}

$r = new ReflectionClass(Target::class);
echo "named:", count($r->getAttributes(name: Marker::class, flags: 0)), "\n";
echo "reversed:", count($r->getAttributes(flags: 0, name: Marker::class)), "\n";
echo "name-only:", count($r->getAttributes(name: Marker::class)), "\n";
"#,
    );
    assert_eq!(out, "named:1\nreversed:1\nname-only:1\n");
}

/// The flag reaches `$flags` through a named argument at index 0, where a positional read of
/// `args[1]` would find the NAME. Measured before the guard read named arguments: this answered
/// `1` where PHP answers `2`.
#[test]
fn test_get_attributes_rejects_a_named_is_instanceof_flag() {
    let err = compile_expect_type_error(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
echo count($r->getAttributes(flags: ReflectionAttribute::IS_INSTANCEOF, name: Base::class)), "\n";
"#,
    );
    assert!(
        err.contains("getAttributes(): the $flags argument is not supported yet"),
        "unexpected diagnostic: {}",
        err
    );
}

/// A spread is ONE AST argument holding a runtime array, so the checker cannot tell a flag from
/// an absent one. It compiles, and the body refuses `IS_INSTANCEOF` at run time instead of
/// answering with the silent subset (`1` where PHP answers `2`, measured before any refusal).
#[test]
fn test_get_attributes_spread_argument_list_refuses_the_flag_at_run_time() {
    let out = compile_and_run_expect_failure(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
$args = [Base::class, ReflectionAttribute::IS_INSTANCEOF];
echo count($r->getAttributes(...$args)), "\n";
"#,
    );
    assert!(
        out.contains("ReflectionAttribute::IS_INSTANCEOF is not supported yet"),
        "expected the runtime refusal, got: {}",
        out
    );
}

/// A spread of an array LITERAL is readable at compile time, positionally or by name, so it is
/// neither refused nor waved through: `...[Other::class]` and a literal zero flag filter like
/// the written arguments do. Expected output measured on PHP 8.5.10.
#[test]
fn test_get_attributes_reads_a_literal_spread() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

#[Marker, Other, Marker]
class C {}

$r = new ReflectionClass('C');
echo count($r->getAttributes(...[Other::class])), "|";
echo count($r->getAttributes(...[Marker::class, 0])), "|";
echo count($r->getAttributes(...['name' => Marker::class, 'flags' => 0])), "\n";
"#,
    );
    assert_eq!(out, "1|2|2\n");
}

/// A runtime array wrapped in a literal (`...[...$args]`) is no more readable than `...$args`:
/// the literal's element index is no longer the argument index, and the flag it carries is only
/// known at run time, so the body refuses it like the bare spread.
#[test]
fn test_get_attributes_runtime_array_nested_in_a_literal_spread_refuses_at_run_time() {
    let out = compile_and_run_expect_failure(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}
#[Derived] #[Base] class Target {}
$r = new ReflectionClass(Target::class);
$args = [Base::class, ReflectionAttribute::IS_INSTANCEOF];
echo count($r->getAttributes(...[...$args])), "\n";
"#,
    );
    assert!(
        out.contains("ReflectionAttribute::IS_INSTANCEOF is not supported yet"),
        "expected the runtime refusal, got: {}",
        out
    );
}

/// A literal spread carrying `IS_INSTANCEOF` is refused at compile time exactly like the written
/// `getAttributes(Marker::class, 2)`, instead of compiling and throwing at run time.
#[test]
fn test_get_attributes_rejects_the_flag_through_a_literal_spread() {
    let err = compile_expect_type_error(
        r#"<?php
#[Attribute] class Marker {}
#[Marker] class C {}
echo count((new ReflectionClass('C'))->getAttributes(...[Marker::class, ReflectionAttribute::IS_INSTANCEOF])), "\n";
"#,
    );
    assert!(err.contains("the $flags argument is not supported yet"), "unexpected diagnostic: {}", err);
}

/// A `mixed` receiver dispatches on the runtime class id over every class declaring the method,
/// so a Reflection owner is among the candidates and the flag has to be refused there too.
/// Measured before this: `1` where PHP answers `2`.
#[test]
fn test_get_attributes_rejects_the_flag_on_a_mixed_receiver() {
    let err = compile_expect_type_error(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

function pick(int $i): mixed {
    return $i >= 0 ? new ReflectionClass(Target::class) : null;
}

$r = pick(1);
echo count($r->getAttributes(Base::class, ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert!(
        err.contains("getAttributes(): the $flags argument is not supported yet"),
        "unexpected diagnostic: {}",
        err
    );
}

/// A first-class callable hands the method its arguments at a call site the checker cannot tie
/// back to `getAttributes`, so the compile-time rejection never sees the flag. The body throws
/// instead of answering with the subset.
#[test]
fn test_get_attributes_first_class_callable_throws_on_the_flag() {
    let out = compile_and_run_expect_failure(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
$f = $r->getAttributes(...);
echo count($f(Base::class, ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert!(
        out.contains("ReflectionAttribute::IS_INSTANCEOF is not supported yet"),
        "expected the runtime refusal, got: {}",
        out
    );
}

/// `call_user_func_array` hands over a runtime array, which is the same blind spot.
///
/// The argument list is written INLINE rather than through a variable. A variable one makes the
/// call take the descriptor-invoker path, which currently refuses an array-of-Reflection-objects
/// result at compile time (#1233) — an upstream limitation this fixture must not be blocked by,
/// and must not enshrine as expected either.
#[test]
fn test_get_attributes_call_user_func_array_throws_on_the_flag() {
    let out = compile_and_run_expect_failure(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
echo count(call_user_func_array([$r, 'getAttributes'], [Base::class, ReflectionAttribute::IS_INSTANCEOF])), "\n";
"#,
    );
    assert!(
        out.contains("ReflectionAttribute::IS_INSTANCEOF is not supported yet"),
        "expected the runtime refusal, got: {}",
        out
    );
}

/// The runtime check sits BELOW the null-name early return, so a call that filters nothing still
/// answers with everything however `$flags` is spelled — through an indirection as well.
#[test]
fn test_get_attributes_null_name_through_an_indirection_ignores_the_flag() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
$f = $r->getAttributes(...);
echo count($f(null, ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// A dynamic method name desugars to `call_user_func([$r, $m], ...)` in the parser, so no method
/// inference runs and the compile-time rejection cannot see the call at all. The body's throw is
/// what makes it loud.
#[test]
fn test_get_attributes_dynamic_method_name_throws_on_the_flag() {
    let out = compile_and_run_expect_failure(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
$m = 'getAttributes';
echo count($r->$m(Base::class, ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert!(
        out.contains("ReflectionAttribute::IS_INSTANCEOF is not supported yet"),
        "expected the runtime refusal, got: {}",
        out
    );
}

/// A named `flags:` can stand alone, and then `$name` takes its `null` default — so PHP filters
/// nothing and the flag is inert. `getAttributes(flags: 2)` and `getAttributes(null, 2)` are the
/// same call; refusing the first while allowing the second would be a compile error on a working
/// program.
#[test]
fn test_get_attributes_named_flag_without_a_name_is_accepted() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
#[Base]
class Target {}

$r = new ReflectionClass(Target::class);
echo count($r->getAttributes(flags: ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// `$flags` is an `int` parameter, so PHP coerces a `false` argument to `0` and answers as `0`
/// does. The rejection folds it the same way rather than refusing a working program.
#[test]
fn test_get_attributes_false_flag_folds_to_zero() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

#[Marker]
#[Other]
class Target {}

echo count((new ReflectionClass(Target::class))->getAttributes(Marker::class, false)), "\n";
"#,
    );
    assert_eq!(out, "1\n");
}

/// The `mixed`-receiver refusal must not reach a program that has its OWN `getAttributes`: that
/// class may well be the runtime target, and refusing it is a compile error on valid PHP. The
/// first cut refused this as soon as the program also used reflection anywhere.
#[test]
fn test_get_attributes_mixed_receiver_allows_a_foreign_method() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}

#[Marker]
class Target {}

class Own {
    public function getAttributes(?string $name = null, int $flags = 0): array
    {
        return $flags === 0 ? ["a"] : ["a", "b"];
    }
}

function pick(int $i): mixed {
    return $i >= 0 ? new Own() : null;
}

$r = new ReflectionClass(Target::class);
echo "refl:", count($r->getAttributes(Marker::class)), "\n";
$o = pick(1);
echo "own:", count($o->getAttributes("x", 2)), "\n";
"#,
    );
    assert_eq!(out, "refl:1\nown:2\n");
}

/// A flag-free `call_user_func_array` must still go through: the body only throws on the flag, and
/// the compile-time rejection never sees this shape at all.
#[test]
fn test_get_attributes_call_user_func_array_without_a_flag_filters() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}

#[Marker]
#[Other]
class Target {}

$r = new ReflectionClass(Target::class);
echo count(call_user_func_array([$r, 'getAttributes'], ['Marker'])), "\n";
"#,
    );
    assert_eq!(out, "1\n");
}

/// `ReflectionAttribute::IS_INSTANCEOF` is declared — PHP has exactly that one constant — so the
/// name resolves; the CALL is what is refused, and loudly. Answering it by exact name would
/// return a subset of PHP's answer with no diagnostic, and this call was a compile error before
/// the filter existed anyway.
#[test]
fn test_get_attributes_rejects_is_instanceof_flag() {
    let err = compile_expect_type_error(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}

#[Derived]
class Target {}

$r = new ReflectionClass(Target::class);
echo count($r->getAttributes(Base::class, ReflectionAttribute::IS_INSTANCEOF)), "\n";
"#,
    );
    assert!(
        err.contains("getAttributes(): the $flags argument is not supported yet"),
        "unexpected diagnostic: {}",
        err
    );
}

/// Verifies a STATIC associative spread is read rather than refused.
///
/// The guard treated every spread as opaque, so `getAttributes(...['name' => M::class])` was a
/// compile error for a list the shared call planner expands into named arguments before anything
/// else looks at it — `src/types/call_args/mod.rs` says in its own header that all call surfaces
/// must go through that planner. Measured against host PHP 8.5.10: both spellings answer `1` on a
/// class carrying two attributes, and the plain call answers `2`.
#[test]
fn test_get_attributes_reads_a_static_associative_spread() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class Marker { public function __construct(public int $n = 0) {} }

#[Attribute]
class Other {}

#[Marker(1)]
#[Other]
class Subject {}

$r = new ReflectionClass(Subject::class);
echo count($r->getAttributes());
echo count($r->getAttributes(Marker::class));
echo count($r->getAttributes(name: Marker::class));
echo count($r->getAttributes(...['name' => Marker::class]));
echo count($r->getAttributes(...['name' => Marker::class, 'flags' => 0]));
"#,
    );

    assert_eq!(out, "21111");
}

/// Verifies a spread whose contents are a RUNTIME array reaches the body, which filters by name
/// when the array carries no flag (#1335). PHP 8.5.10 answers `1`.
#[test]
fn test_get_attributes_filters_through_a_runtime_spread() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}

#[Marker]
class Subject {}

$r = new ReflectionClass(Subject::class);
$args = [];
$args['name'] = Marker::class;
echo count($r->getAttributes(...$args)), "\n";
"#,
    );
    assert_eq!(out, "1\n");
}

/// A flag the checker cannot fold reaches the body, which implements a run-time zero exactly
/// like a literal one (#1335): a variable `0`, a runtime-null name next to `IS_INSTANCEOF` (PHP
/// ignores the flag when nothing is filtered), and runtime spreads carrying a zero flag,
/// positionally and by name. Expected output measured on PHP 8.5.10.
#[test]
fn test_get_attributes_accepts_flags_the_body_checks_at_run_time() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Attribute] class Other {}
#[Marker, Other]
class Target {}

$r = new ReflectionClass(Target::class);
$zero = 0;
echo count($r->getAttributes(Marker::class, $zero)), "|";
$n = null;
echo count($r->getAttributes($n, ReflectionAttribute::IS_INSTANCEOF)), "|";
$args = [Marker::class, 0];
echo count($r->getAttributes(...$args)), "|";
$named = ['flags' => 0, 'name' => Marker::class];
echo count($r->getAttributes(...$named)), "\n";
"#,
    );
    assert_eq!(out, "1|2|1|1\n");
}

/// `IS_INSTANCEOF` in a variable next to a real name compiles, and the body refuses it at run
/// time with the same `ReflectionException` the indirect spellings get.
#[test]
fn test_get_attributes_runtime_is_instanceof_flag_is_refused_at_run_time() {
    let out = compile_and_run_expect_failure(
        r#"<?php
#[Attribute] class Base {}
#[Attribute] class Derived extends Base {}
#[Derived] #[Base] class Target {}
$r = new ReflectionClass(Target::class);
$flag = ReflectionAttribute::IS_INSTANCEOF;
echo count($r->getAttributes(Base::class, $flag)), "\n";
"#,
    );
    assert!(
        out.contains("ReflectionAttribute::IS_INSTANCEOF is not supported yet"),
        "expected the runtime refusal, got: {}",
        out
    );
}

/// A literal `null` flag is refused at compile time: PHP coerces it to `0` behind a deprecation
/// notice, and the synthesized method would receive the null itself.
#[test]
fn test_get_attributes_refuses_a_literal_null_flag() {
    let err = compile_expect_type_error(
        r#"<?php
#[Attribute] class Marker {}
#[Marker] class Target {}
echo count((new ReflectionClass(Target::class))->getAttributes(Marker::class, null)), "\n";
"#,
    );
    assert!(
        err.contains("passing null to the $flags argument is not supported"),
        "unexpected diagnostic: {}",
        err
    );
}

/// Every value but `0` and `IS_INSTANCEOF` raises PHP's `ValueError`, whatever `$name` holds and
/// however the call reaches the method (#1336): written directly (a literal flag compiles now and
/// is checked at run time), with a null name, through a first-class callable,
/// `call_user_func_array`, and a dynamic method name. The message names the class declaring the
/// method: `ReflectionFunctionAbstract` for a method. Expected output measured on PHP 8.5.10.
#[test]
fn test_get_attributes_invalid_flags_raise_value_error() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker {}
#[Marker]
class Target { #[Marker] public function m() {} }

$r = new ReflectionClass(Target::class);
try { echo count($r->getAttributes(Marker::class, 3)), "\n"; }
catch (ValueError $e) { echo "direct: ", $e->getMessage(), "\n"; }
try { echo count($r->getAttributes(null, 1)), "\n"; }
catch (ValueError $e) { echo "null-name: ", $e->getMessage(), "\n"; }
$f = $r->getAttributes(...);
try { echo count($f(Marker::class, 3)), "\n"; }
catch (ValueError $e) { echo "first-class: ", $e->getMessage(), "\n"; }
try { echo count(call_user_func_array([$r, 'getAttributes'], [Marker::class, 1])), "\n"; }
catch (ValueError $e) { echo "call_user_func_array: ", $e->getMessage(), "\n"; }
$m = 'getAttributes';
try { echo count($r->$m(null, 4)), "\n"; }
catch (ValueError $e) { echo "dynamic: ", $e->getMessage(), "\n"; }
$method = new ReflectionMethod(Target::class, 'm');
try { echo count($method->getAttributes(null, -1)), "\n"; }
catch (ValueError $e) { echo "method: ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "direct: ReflectionClass::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag\n\
         null-name: ReflectionClass::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag\n\
         first-class: ReflectionClass::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag\n\
         call_user_func_array: ReflectionClass::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag\n\
         dynamic: ReflectionClass::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag\n\
         method: ReflectionFunctionAbstract::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag\n"
    );
}

/// `ReflectionEnum::getAttributes()` returns `ReflectionAttribute` objects like every other owner
/// (#1337), and an enum's own attributes are reported at all. Expected output measured on PHP
/// 8.5.10.
#[test]
fn test_reflection_enum_get_attributes_returns_reflection_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Attribute] class Marker { public function __construct(public string $tag = "none") {} }
#[Attribute] class Other {}
#[Marker("suit"), Other]
enum Suit { case Hearts; }

$r = new ReflectionEnum(Suit::class);
foreach ($r->getAttributes() as $attribute) {
    echo $attribute->getName(), "|";
}
$filtered = $r->getAttributes(Marker::class);
echo count($filtered), "|", $filtered[0]->newInstance()->tag, "\n";
"#,
    );
    assert_eq!(out, "Marker|Other|1|suit\n");
}

/// The filter, the element's `getName()`, and `newInstance()` work on every remaining owner
/// (#1338): object, parameter, class constant, enum (through `ReflectionEnum` and
/// `ReflectionClass`), and unit and backed enum cases. A parameter attribute used to have no
/// factory, so its `newInstance()` returned `null`. Expected output measured on PHP 8.5.10.
#[test]
fn test_get_attributes_filter_on_remaining_owners() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_ALL)]
class Marker { public function __construct(public string $tag = "none") {} }
#[Attribute(Attribute::TARGET_ALL)]
class Other {}
#[Marker("class"), Other]
class Target {
    #[Marker("const"), Other] public const C = 1;
    public function m(#[Marker("param"), Other] int $x) {}
}
#[Marker("enum"), Other]
enum Suit { #[Marker("case"), Other] case Hearts; }
enum Level: int { #[Marker("bcase"), Other] case Low = 1; }

function show(string $label, array $all, array $filtered): void {
    echo $label, ": ", count($all), " ", count($filtered), " ", $filtered[0]->getName(), " ",
        $filtered[0]->newInstance()->tag, "\n";
}
$object = new ReflectionObject(new Target());
show("object", $object->getAttributes(), $object->getAttributes(Marker::class));
$parameter = new ReflectionParameter([Target::class, 'm'], 0);
show("parameter", $parameter->getAttributes(), $parameter->getAttributes(Marker::class));
$constant = new ReflectionClassConstant(Target::class, 'C');
show("constant", $constant->getAttributes(), $constant->getAttributes(Marker::class));
$enum = new ReflectionEnum(Suit::class);
show("enum", $enum->getAttributes(), $enum->getAttributes(Marker::class));
$enumClass = new ReflectionClass(Suit::class);
show("enum-class", $enumClass->getAttributes(), $enumClass->getAttributes(Marker::class));
$case = new ReflectionEnumUnitCase(Suit::class, 'Hearts');
show("case", $case->getAttributes(), $case->getAttributes(Marker::class));
$backed = new ReflectionEnumBackedCase(Level::class, 'Low');
show("backed-case", $backed->getAttributes(), $backed->getAttributes(Marker::class));
"#,
    );
    assert_eq!(
        out,
        "object: 2 1 Marker class\n\
         parameter: 2 1 Marker param\n\
         constant: 2 1 Marker const\n\
         enum: 2 1 Marker enum\n\
         enum-class: 2 1 Marker enum\n\
         case: 2 1 Marker case\n\
         backed-case: 2 1 Marker bcase\n"
    );
}

/// An element the filter returns inside eval still answers `getName()` and builds its attribute
/// through `newInstance()`, for class, object, method, property, and class-constant owners
/// (#1338); the earlier eval test only counted. Expected output measured on PHP 8.5.10.
#[test]
fn test_get_attributes_filtered_eval_elements_answer_name_and_new_instance() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_ALL)] class Marker { public function __construct(public string $v = "x") {} }
#[Attribute(Attribute::TARGET_ALL)] class Other {}

#[Marker("class"), Other]
class Target {
    #[Marker("const"), Other] public const C = 1;
    #[Marker("prop"), Other] public int $p = 0;
    #[Marker("method"), Other] public function run() {}
}

eval('foreach ([
    "class" => new ReflectionClass("Target"),
    "object" => new ReflectionObject(new Target()),
    "method" => new ReflectionMethod("Target", "run"),
    "property" => new ReflectionProperty("Target", "p"),
    "constant" => new ReflectionClassConstant("Target", "C"),
] as $label => $owner) {
    $filtered = $owner->getAttributes("Marker");
    echo $label, ":", count($owner->getAttributes()), ":", count($filtered), ":",
        $filtered[0]->getName(), ":", $filtered[0]->newInstance()->v, "\n";
}');
"#,
    );
    assert_eq!(
        out,
        "class:2:1:Marker:class\n\
         object:2:1:Marker:class\n\
         method:2:1:Marker:method\n\
         property:2:1:Marker:prop\n\
         constant:2:1:Marker:const\n"
    );
}

/// A top-level function's parameter attribute answers `newInstance()` with the built attribute,
/// reached through `ReflectionFunction::getParameters()` and through a directly constructed
/// `ReflectionParameter`, filtered or not; a parameter without attributes answers empty. Function
/// parameters feed the attribute factories through their own path, separate from method
/// parameters. Expected output measured on PHP 8.5.10.
#[test]
fn test_function_parameter_attribute_new_instance_builds_the_attribute() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_ALL)]
class Marker { public function __construct(public string $tag = "none") {} }
#[Attribute(Attribute::TARGET_ALL)]
class Other {}
function tagged(#[Marker("fparam"), Other] int $x, int $plain = 0) {}

$params = (new ReflectionFunction('tagged'))->getParameters();
$first = $params[0]->getAttributes();
echo count($first), " ", $first[0]->getName(), " ", $first[0]->newInstance()->tag, "\n";
echo count($params[1]->getAttributes()), "\n";
$direct = new ReflectionParameter('tagged', 'x');
$filtered = $direct->getAttributes(Marker::class);
echo count($filtered), " ", $filtered[0]->newInstance()->tag, "\n";
"#,
    );
    assert_eq!(out, "2 Marker fparam\n0\n1 fparam\n");
}

/// An enum method's own attributes are reported, filtered, and instantiated, for an instance
/// method, a static method, and in the class's `getMethods()` listing; a method without
/// attributes answers empty, and the method still runs. Expected output measured on PHP 8.5.10.
#[test]
fn test_enum_method_get_attributes_reports_declared_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_ALL)]
class Marker { public function __construct(public string $tag = "none") {} }
#[Attribute(Attribute::TARGET_ALL)]
class Other {}
enum Suit {
    case Hearts;
    #[Marker("label"), Other] public function label(): string { return "hearts"; }
    #[Marker("make")] public static function make(): self { return self::Hearts; }
    public function plain(): string { return "plain"; }
}
$label = new ReflectionMethod(Suit::class, 'label');
$all = $label->getAttributes();
echo count($all), " ", $all[0]->getName(), " ", $all[1]->getName(), "\n";
$filtered = $label->getAttributes(Marker::class);
echo count($filtered), " ", $filtered[0]->newInstance()->tag, "\n";
$make = new ReflectionMethod(Suit::class, 'make');
echo count($make->getAttributes()), " ", $make->getAttributes()[0]->newInstance()->tag, "\n";
echo count((new ReflectionMethod(Suit::class, 'plain'))->getAttributes()), "\n";
foreach ((new ReflectionClass(Suit::class))->getMethods() as $method) {
    if ($method->getName() === 'label') { echo "listed ", count($method->getAttributes()), "\n"; }
}
echo Suit::Hearts->label(), "\n";
"#,
    );
    assert_eq!(out, "2 Marker Other\n1 label\n1 make\n0\nlisted 2\nhearts\n");
}
