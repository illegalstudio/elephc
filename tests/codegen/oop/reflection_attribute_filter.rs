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
//! - `$flags` is declared for signature parity but only `0` is honoured; anything else is a
//!   compile error, because `ReflectionAttribute::IS_INSTANCEOF` would need a subclass test
//!   on a runtime class name that no AOT builtin answers (#1113).
//! - The rejection is exercised in every spelling that reaches the flag — positional, named,
//!   through a spread, and on a `mixed` receiver — because the first cut read `args[1]` on a
//!   named-class receiver only, and both `getAttributes(...$args)` and a `mixed` receiver
//!   walked past it into the silent subset the rejection exists to prevent.
//! - Three spellings hand the method its arguments with no visible list at all: a first-class
//!   callable, `call_user_func_array`, and a dynamic method name (which the parser desugars to
//!   `call_user_func`). The compile-time rejection cannot see any of them, so the synthesized body
//!   throws a `ReflectionException` — measured at 1 where PHP answers 2 before the throw existed.
//! - The refusals that would be WRONG are tested too. PHP 8.5 accepts exactly `0` and `2` for
//!   `$flags` and raises `ValueError: Argument #2 ($flags) must be a valid attribute filter flag`
//!   for anything else (measured with 1, 3 and 4), so the only valid value elephc turns away is
//!   `IS_INSTANCEOF` itself — every other accepted spelling has to keep working.

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

/// A spread is ONE AST argument holding a runtime array, so an arity test cannot tell a flag from
/// an absent one. Measured before the guard handled it: `getAttributes(...[Base::class, 2])`
/// answered `1` where PHP answers `2` — the silent subset, with no diagnostic.
#[test]
fn test_get_attributes_rejects_a_spread_argument_list() {
    let err = compile_expect_type_error(
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
        err.contains("cannot be read through a spread"),
        "unexpected diagnostic: {}",
        err
    );
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
$args = [Base::class, ReflectionAttribute::IS_INSTANCEOF];
echo count(call_user_func_array([$r, 'getAttributes'], $args)), "\n";
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
