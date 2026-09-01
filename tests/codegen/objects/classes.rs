//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object classes, including class empty, class object aliasing, and class constructor calls method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that PHP 8 semi-reserved keywords are usable as member names end-to-end: an
/// instance method (`self`), a static method (`new`), an instance method via `->parent()`,
/// a property accessed as `->list`, and a class constant `FN` accessed via `::`. Mirrors PHP,
/// which outputs "3|7|9|11|5".
#[test]
fn test_keyword_named_members() {
    let out = compile_and_run(
        r#"<?php
class Widget {
    public $list = 7;
    const FN = 5;
    public function self() { return 3; }
    public function parent() { return 9; }
    public static function new() { return 11; }
}
$w = new Widget();
echo $w->self(), "|";
echo $w->list, "|";
echo $w->parent(), "|";
echo Widget::new(), "|";
echo Widget::FN;
"#,
    );
    assert_eq!(out, "3|7|9|11|5");
}

/// Verifies that an empty class (no properties or methods) can be instantiated and
/// emits the expected "ok" output, confirming object allocation works for minimal classes.
#[test]
fn test_class_empty() {
    let out = compile_and_run(
        r#"<?php
class Blank {}
$e = new Blank();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a named class can be instantiated without constructor parentheses.
#[test]
fn test_class_instantiation_without_constructor_parentheses() {
    let out = compile_and_run(
        r#"<?php
class B {
    public string $tag = "ok";
}
$b = new B;
echo $b->tag;
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that assigning an object to a second variable shares the same instance.
/// Both variables reference the same heap object, so mutating via one is visible via the other.
#[test]
fn test_class_dynamic_instantiation() {
    // `new $variable()` dispatches known class names through the same AOT
    // allocation path as `new ClassName()`. Known names yield object Mixed cells.
    let out = compile_and_run(
        r#"<?php
class Foo {}
class Bar {}
$cls = "Foo";
$obj = new $cls();
$cls2 = "Bar";
$obj2 = new $cls2();
echo gettype($obj) . "|" . gettype($obj2);
"#,
    );
    assert_eq!(out, "object|object");
}

/// Verifies a class-string variable can be instantiated without constructor parentheses.
#[test]
fn test_class_dynamic_instantiation_without_constructor_parentheses() {
    let out = compile_and_run(
        r#"<?php
class DynamicBox {
    public int $n = 7;
}
$cls = "DynamicBox";
$o = new $cls;
echo $o->n;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies compiled PHP output for class dynamic instantiation runs property defaults.
#[test]
fn test_class_dynamic_instantiation_runs_property_defaults() {
    // `new $var()` must apply declared property defaults through the same
    // allocation path as `new ClassName()`. Previously these read back as
    // 0/null.
    let out = compile_and_run(
        r#"<?php
class C {
    public int $n = 7;
    public string $s = "hi";
    public float $f = 1.5;
    public bool $b = true;
    public array $a = [1, 2, 3];
}
$cls = "C";
$o = new $cls();
echo $o->n . "|" . $o->s . "|" . $o->f . "|" . ($o->b ? "T" : "F") . "|" . count($o->a);
"#,
    );
    assert_eq!(out, "7|hi|1.5|T|3");
}

/// Verifies that dynamic instantiation forwards constructor arguments.
#[test]
fn test_class_dynamic_instantiation_runs_constructor_args() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $x = 1;
    public function __construct(int $x) { $this->x = $x; }
}
$cls = "C";
$o = new $cls(7);
echo $o->x;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies PHP userland constructors accept surplus positional arguments while still evaluating
/// them in source order before entering the constructor body.
#[test]
fn test_userland_constructor_accepts_extra_positional_arguments() {
    let out = compile_and_run(
        r#"<?php
function extra_constructor_argument(): int {
    echo "argument;";
    return 7;
}
class ExtraArgumentConstructor {
    public function __construct() {
        echo "constructor;";
    }
}
new ExtraArgumentConstructor(extra_constructor_argument());
echo "done";
"#,
    );
    assert_eq!(out, "argument;constructor;done");
}

/// Verifies a userland class without a declared constructor also accepts surplus positional
/// arguments and evaluates them even though there is no constructor body to receive them.
#[test]
fn test_constructorless_userland_class_accepts_extra_positional_arguments() {
    let out = compile_and_run(
        r#"<?php
function unused_constructor_argument(): int {
    echo "argument;";
    return 7;
}
class ConstructorlessExtraArgument {}
new ConstructorlessExtraArgument(unused_constructor_argument());
echo "done";
"#,
    );
    assert_eq!(out, "argument;done");
}

/// Verifies an override may make an inherited required parameter optional, as PHP's method
/// compatibility rules permit callers to use the child method with fewer arguments.
#[test]
fn test_method_override_may_make_parent_parameter_optional() {
    let out = compile_and_run(
        r#"<?php
class RequiredParentParameter {
    public function render(string $value): string { return $value; }
}
class OptionalChildParameter extends RequiredParentParameter {
    public function render(string $value = "default"): string { return $value; }
}
echo (new OptionalChildParameter())->render();
"#,
    );
    assert_eq!(out, "default");
}

/// Verifies an override may append optional and variadic parameters without changing the
/// inherited callable prefix, matching PHP's contravariant parameter-count rules.
#[test]
fn test_method_override_may_append_optional_and_variadic_parameters() {
    let out = compile_and_run(
        r#"<?php
class ShortParentSignature {
    public function render(string $value): string { return $value; }
}
class WiderChildSignature extends ShortParentSignature {
    public function render(string $value, string $suffix = "!", mixed ...$rest): string {
        return $value . $suffix . count($rest);
    }
}
echo (new WiderChildSignature())->render("ok", "?", 1, 2);
"#,
    );
    assert_eq!(out, "ok?2");
}

/// Verifies a child class can narrow one object member inside a declared return union.
#[test]
fn test_method_override_covariant_self_member_inside_union() {
    let out = compile_and_run(
        r#"<?php
class UnionReturnBase {
    public static function make(bool $ok): UnionReturnBase|false {
        return $ok ? new UnionReturnBase() : false;
    }
}
class UnionReturnChild extends UnionReturnBase {
    public static function make(bool $ok): UnionReturnChild|false {
        return $ok ? new UnionReturnChild() : false;
    }
}
echo UnionReturnChild::make(true) instanceof UnionReturnChild ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies weak PHP scalar coercion stores booleans as `0`/`1` in an int-typed property,
/// covering DateInterval's public integer fields such as `$invert`.
#[test]
fn test_bool_assignment_coerces_to_int_property_storage() {
    let out = compile_and_run(
        r#"<?php
class IntegerProperty {
    public int $value = 0;
}
$box = new IntegerProperty();
$box->value = true;
echo $box->value, "|";
$box->value = false;
echo $box->value;
"#,
    );
    assert_eq!(out, "1|0");
}

/// Verifies DateTimeInterface operands and `DateTime|false` factory results use php-src's
/// instant comparison for equality and ordering across mutable and immutable instances.
#[test]
fn test_datetime_interface_and_factory_union_comparisons() {
    let out = compile_and_run(
        r#"<?php
function compare_dates(DateTimeInterface $left, DateTimeInterface $right): void {
    echo $left == $right ? "E" : "N";
    echo $left < $right ? "L" : "G";
    echo $right > $left ? "R" : "X";
}
$left = new DateTime("@1448889063.3531");
$right = new DateTimeImmutable("@1448889063.5216");
compare_dates($left, $right);
$factoryLeft = DateTime::createFromFormat("U.u", "1448889063.3531");
$factoryRight = DateTimeImmutable::createFromFormat("U.u", "1448889063.5216");
echo "|", ($factoryLeft <=> $factoryRight);
"#,
    );
    assert_eq!(out, "NLR|-1");
}

/// Verifies ext/date bases and subclasses store dynamic properties, deprecate only first
/// creation, honor `@`, and let an explicit `AllowDynamicProperties` attribute silence it.
#[test]
fn test_datetime_internal_classes_store_and_clone_dynamic_properties() {
    let out = compile_and_run_capture(
        r#"<?php
class DateChild extends DateTime {}
#[AllowDynamicProperties]
class QuietDateChild extends DateTime {}
function set_suppressed_date_property(DateTime $date): void {
    $date->suppressed = 1;
}
$base = new DateTime("@0");
$base->label = "epoch";
$base->label = "updated";
@set_suppressed_date_property($base);
$child = new DateChild("@0");
$child->count = 2;
$clone = clone $child;
$quiet = new QuietDateChild("@0");
$quiet->silent = 3;
echo $base->label, "|", $base->suppressed, "|", $clone->count, "|", $quiet->silent;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "updated|1|2|3");
    assert_eq!(
        out.stderr
            .matches("Creation of dynamic property DateTime::$label is deprecated")
            .count(),
        1
    );
    assert_eq!(
        out.stderr
            .matches("Creation of dynamic property DateChild::$count is deprecated")
            .count(),
        1
    );
    assert!(!out.stderr.contains("$suppressed"));
    assert!(!out.stderr.contains("QuietDateChild::$silent"));
}

/// Verifies that dynamic instantiation uses SPL-specific runtime storage initialization.
#[test]
fn test_class_dynamic_instantiation_uses_spl_storage() {
    let out = compile_and_run(
        r#"<?php
$cls = "SplFixedArray";
$a = new $cls(2);
$a[0] = "x";
echo count($a) . ":" . $a[0];
"#,
    );
    assert_eq!(out, "2:x");
}

/// Verifies that dynamic class-string lookup follows PHP's case-insensitive class rules.
#[test]
fn test_class_dynamic_instantiation_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
function pick_class(): string {
    return "mixedcase";
}
class MixedCase {
    public int $x = 0;
    public function __construct(int $x) { $this->x = $x; }
}
$cls = pick_class();
$o = new $cls(12);
echo gettype($o) . ":" . $o->x;
"#,
    );
    assert_eq!(out, "object:12");
}

/// Verifies `new ClassName();` is valid as a standalone expression statement and preserves
/// constructor side effects.
#[test]
fn test_new_object_expression_statement_runs_constructor() {
    let out = compile_and_run(
        r#"<?php
class SideEffectNew {
    public function __construct() { echo "constructed"; }
}
new SideEffectNew();
"#,
    );
    assert_eq!(out, "constructed");
}

/// Verifies `new $className();` is valid as a standalone expression statement and preserves
/// constructor side effects.
#[test]
fn test_dynamic_new_expression_statement_runs_constructor() {
    let out = compile_and_run(
        r#"<?php
class DynamicSideEffectNew {
    public function __construct() { echo "dynamic"; }
}
$className = "DynamicSideEffectNew";
new $className();
"#,
    );
    assert_eq!(out, "dynamic");
}

/// Verifies `new $c(...)` fills in a builtin constructor's DEFAULT arguments.
///
/// The checker pads a constructor call with its declared defaults only when it can see which
/// constructor it is; `new $c(["a" => 1])` names its class in a string, so it cannot. Codegen knows
/// the class at run time, but by then the arguments are materialized values rather than
/// expressions, and it used to refuse any candidate whose arity did not match exactly — the site
/// fell to the generic allocation path and came back with the constructor never run: `ArrayObject`
/// answered a count of `0` where PHP answers `1`. A padding thunk lowered per (class, argc) pair
/// closes that gap.
///
/// Both arities are here because they fail differently. The zero-argument form has to synthesise
/// EVERY argument, and calling the two-parameter constructor with none of them left `$flags`
/// holding whatever the register happened to carry — a fatal about an impossible array size.
///
/// The reads go through the METHODS, not `count($o)` and `$o["a"]`: those two dispatch through a
/// `mixed` value, which does not reach a synthetic class's `Countable`/`ArrayAccess` at all, and a
/// statically built `ArrayObject` fails them identically. That is a separate defect, and pinning it
/// here would tie this test to it.
#[test]
fn test_dynamic_new_pads_builtin_constructor_defaults() {
    let out = compile_and_run(
        r#"<?php
$c = "ArrayObject";
$one = new $c(["a" => 1]);
echo $one->count(), ":", $one->offsetGet("a"), "
";
$none = new $c();
$none->append(7);
echo $none->count(), ":", $none->offsetGet(0), "
";
"#,
    );
    assert_eq!(out, "1:1
1:7
");
}

/// Verifies dynamic instantiation of an unknown class exits with PHP's class-not-found fatal.
#[test]
fn test_dynamic_instantiation_missing_class_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Has { public int $x = 9; }
$missing = "Nope";
$bad = new $missing();
"#,
    );
    assert!(err.contains("Fatal error: Uncaught Error: Class \"Nope\" not found"), "{err}");
}

/// Verifies a standalone dynamic-new statement uses the same class-not-found fatal.
#[test]
fn test_dynamic_new_expression_statement_missing_class_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$missing = "Nope";
new $missing();
"#,
    );
    assert!(err.contains("Fatal error: Uncaught Error: Class \"Nope\" not found"), "{err}");
}

/// Verifies `new $c()` reaches an SPL class the program never spells.
///
/// The SPL surface is registered pay-for-use, and the gate used to ask only whether some name in
/// the program REFERENCED one of those classes. `new $c` carries its class in a value, so nothing
/// was referenced, nothing was registered, and the program compiled and then died with
/// `Class "ArrayObject" not found` where php constructs the object.
///
/// The name is assembled out of an array so no constant fold can put the literal `ArrayObject`
/// back into the program: with the literal present the gate opens for the OLD reason and the test
/// passes without exercising the fix.
#[test]
fn test_dynamic_new_reaches_unspelled_spl_class() {
    let out = compile_and_run(
        r#"<?php
$parts = ["Array", "Object"];
$c = $parts[0] . $parts[1];
$o = new $c(["a" => 1]);
echo get_class($o), ":", $o->count(), "
";
"#,
    );
    assert_eq!(out, "ArrayObject:1\n");
}

/// Verifies `new $c()` refuses a USER constructor it cannot satisfy, with php's wording.
///
/// A static `new K()` is a compile error, but `new $c()` cannot be checked that way, and the site
/// fell through to the runtime fallback — which allocates by name and never runs the constructor.
/// The object came back built out of its property defaults, so this answered `K v='defaut'` where
/// php raises `ArgumentCountError`: no diagnostic, wrong object.
#[test]
fn test_dynamic_new_too_few_arguments_is_argument_count_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class K { public $v = "defaut"; function __construct($x) { $this->v = $x; } }
$c = "K";
$o = new $c();
echo $o->v, "
";
"#,
    );
    assert!(
        err.contains(
            "Fatal error: Uncaught ArgumentCountError: Too few arguments to function \
             K::__construct(), 0 passed in"
        ),
        "{err}"
    );
    assert!(err.contains("and exactly 1 expected"), "{err}");
}

/// Verifies the same refusal for a BUILTIN constructor, which php words differently.
///
/// php prints `expects at least 1 argument, 0 given` for an internal class and
/// `Too few arguments to function …` for a user one, so both shapes are reproduced rather than
/// one being used for everything. `at least` rather than `exactly` because `IteratorIterator`
/// declares an optional second parameter.
#[test]
fn test_dynamic_new_too_few_arguments_builtin_uses_internal_wording() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$parts = ["Iterator", "Iterator"];
$c = $parts[0] . $parts[1];
$o = new $c();
"#,
    );
    assert!(
        err.contains(
            "Fatal error: Uncaught ArgumentCountError: \
             IteratorIterator::__construct() expects at least 1 argument, 0 given"
        ),
        "{err}"
    );
}

/// Verifies `new $c(...)` RUNS a variadic constructor, at every arity.
///
/// A variadic collector is the signature's final parameter and the lowered callee takes it as ONE
/// array — `int ...$r` becomes a single `array<int>` parameter — while the site passes N separate
/// arguments. No site arity ever matched that frame, so the class dropped out of the dynamic-new
/// ladder entirely and `__rt_new_by_name` allocated it with the constructor NEVER RUN: the object
/// came back holding its property defaults, silently. Measured wrong at 11 of 12 shape/arity
/// combinations, against a static `new V(...)` that works.
///
/// The fix routes every arity through the padding thunk, whose body is PHP the normal call
/// lowering handles — the same path the working static call takes.
#[test]
fn test_dynamic_new_runs_a_variadic_constructor() {
    let out = compile_and_run(
        r#"<?php
class V { public $s = "JAMAIS"; function __construct(...$r) { $this->s = "n=" . count($r); } }
$c = "V";
$a = new $c();
$b = new $c(1);
$d = new $c(1, 2);
$e = new $c(1, 2, 3);
echo $a->s, "|", $b->s, "|", $d->s, "|", $e->s, "
";
"#,
    );
    assert_eq!(out, "n=0|n=1|n=2|n=3\n");
}

/// Verifies the regular parameters keep their own values when a collector follows them.
///
/// The thunk pads only up to the last REGULAR parameter; the collector takes what is left rather
/// than a default. Padding it too would have passed a default expression as the first collected
/// element.
#[test]
fn test_dynamic_new_variadic_keeps_regular_parameters() {
    let out = compile_and_run(
        r#"<?php
class V {
    public $s = "JAMAIS";
    function __construct($a = 5, ...$r) { $this->s = "a=$a n=" . count($r); }
}
$c = "V";
$none = new $c();
$one = new $c(9);
$three = new $c(9, 8, 7);
echo $none->s, "|", $one->s, "|", $three->s, "
";
"#,
    );
    assert_eq!(out, "a=5 n=0|a=9 n=0|a=9 n=2\n");
}

/// Verifies a TYPED collector still collects when the site supplies its element type.
///
/// The guard that stops a mismatched argument reaching the collector is deliberately about the
/// SITE's types, not the declaration's: refusing every typed collector would give up
/// `new $c(7)` on `int ...$r`, which is exactly right. An earlier cut placed the guard where only
/// the declaration was visible and lost this; a second cut compared representations without
/// exempting `Mixed` and lost four more rows, because `Mixed` is the one slot codegen BOXES into
/// rather than reinterpreting.
#[test]
fn test_dynamic_new_typed_variadic_collects_matching_arguments() {
    let out = compile_and_run(
        r#"<?php
class V {
    public $s = "JAMAIS";
    function __construct(int ...$r) { $this->s = "n=" . count($r) . " v0=" . ($r[0] ?? "rien"); }
}
$c = "V";
$none = new $c();
$two = new $c(7, 8);
echo $none->s, "|", $two->s, "
";
"#,
    );
    assert_eq!(out, "n=0 v0=rien|n=2 v0=7\n");
}

/// Verifies a TYPED collector COERCES what php coerces and refuses what php refuses.
///
/// Three answers were wrong here in turn. Materializing an overflow argument AS the declared
/// element type performs no conversion, so `new $c("x")` on `int ...$r` came back holding
/// `4370954896` — the string's ADDRESS read as an integer. Dropping the class from the ladder
/// stopped that but left the site building an object from its property defaults with the
/// constructor never run. Refusing every mismatch reported it, but also refused `new $c("7")` and
/// `new $c(1.5)`, which php CONSTRUCTS.
///
/// The overflow now crosses as `Mixed` — the one slot codegen boxes rather than reinterprets — and
/// the padding thunk casts it down in PHP, where php's coercion rules can be spelled out: anything
/// numeric or boolean reaches an `int` collector, and a value php cannot read as a number raises a
/// `TypeError`, the class php raises too.
///
/// php also prints `Deprecated: Implicit conversion from float 1.5 to int loses precision` for the
/// lossy cases. elephc emits no runtime deprecation notice ANYWHERE, so that is a gap of its own
/// and not of this path; the constructed value matches.
#[test]
fn test_dynamic_new_typed_variadic_refuses_a_mismatched_argument() {
    // The arity where nothing reaches the collector is unaffected and still runs.
    let out = compile_and_run(
        r#"<?php
class V {
    public $s = "SANS-CONSTRUCTEUR";
    function __construct(string $a = "d", int ...$r) { $this->s = "a=$a n=" . count($r); }
}
$c = "V";
$safe = new $c("x");
echo $safe->s, "
";
"#,
    );
    assert_eq!(out, "a=x n=0\n");

    // What php COERCES, this coerces: a numeric string, a lossy float, a bool. The values are
    // php's own, checked against it rather than against what the cast happened to produce.
    let coerced = compile_and_run(
        r#"<?php
class W { public $s = "JAMAIS"; function __construct(int ...$r) { $this->s = implode(",", $r); } }
$c = "W";
echo (new $c("7"))->s, "|", (new $c(1.5))->s, "|", (new $c(2.0))->s, "|", (new $c(true))->s, "|", (new $c(7, "8"))->s, "
";
"#,
    );
    assert_eq!(coerced, "7|1|2|1|7,8\n");

    // What php REFUSES, this refuses, with the class php uses. A cast without the guard would have
    // made this a silent `(int)"x" === 0`, which is the whole reason the guard exists.
    let caught = compile_and_run(
        r#"<?php
class X { public $s = "SANS-CONSTRUCTEUR"; function __construct(int ...$r) { $this->s = "ran"; } }
$c = "X";
foreach (["x", null] as $bad) {
    try {
        $o = new $c($bad);
        echo $o->s, "
";
    } catch (TypeError $e) {
        echo get_class($e), ":", (strpos($e->getMessage(), "X::__construct(): Argument #1") === 0 ? "prefixe-php" : "autre"), "
";
    }
}
"#,
    );
    assert_eq!(caught, "TypeError:prefixe-php\nTypeError:prefixe-php\n");
}

/// Verifies the arity refusal counts a VARIADIC signature the way the checker does.
///
/// A variadic collector is the signature's final parameter and carries no default expression, so
/// counting "parameters without a default" makes `...$rest` look required. That count would refuse
/// `new $c()` on `__construct(...$rest)` — a call php ACCEPTS — so the refusal reuses the checker's
/// own `regular_param_count` rather than restating the rule.
///
/// `__construct($a, ...$rest)` is the boundary that proves the count is right rather than merely
/// switched off for variadics: php DOES refuse this one, because `$a` is required, and the wording
/// is `at least` rather than `exactly` because the collector can still take more.
///
/// The zero-required side of that boundary — `new $c()` on `__construct(...$rest)` — is NOT
/// asserted here. It reaches a separate, pre-existing hole: a variadic constructor is left out of
/// the dynamic-new ladder at EVERY arity and never runs, so the object comes back built from its
/// property defaults. Measured at 11 of 12 shape/arity combinations. Pinning today's answer would
/// pin that bug, which is what this suite has been burned by before.
#[test]
fn test_dynamic_new_arity_refusal_excludes_the_variadic_collector() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class V { function __construct($a, ...$rest) {} }
$c = "V";
new $c();
"#,
    );
    assert!(
        err.contains(
            "Fatal error: Uncaught ArgumentCountError: Too few arguments to function \
             V::__construct(), 0 passed in"
        ),
        "{err}"
    );
    assert!(err.contains("and at least 1 expected"), "{err}");
}

/// Verifies the refusal is CATCHABLE, which is what makes it an error object and not a fatal.
///
/// php raises `ArgumentCountError`, a `TypeError` subclass, so all four of these catch it. A bare
/// fatal would satisfy the message assertions above while failing every program that guards.
#[test]
fn test_dynamic_new_argument_count_error_is_catchable() {
    let out = compile_and_run(
        r#"<?php
class K { function __construct($x) {} }
$c = "K";
try {
    new $c();
} catch (TypeError $e) {
    echo get_class($e), "
";
}
"#,
    );
    assert_eq!(out, "ArgumentCountError\n");
}

/// Verifies dynamic instantiation rejects non-string class expressions instead of returning null.
#[test]
fn test_dynamic_instantiation_non_string_class_name_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$missing = 123;
$bad = new $missing();
"#,
    );
    assert!(
        err.contains("Fatal error: Uncaught Error: Class name must be a valid object or a string"),
        "{err}"
    );
}

/// Verifies compiled PHP output for class object aliasing.
#[test]
fn test_class_object_aliasing() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }
$a = new Box();
$a->val = 42;
$b = $a;
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a constructor can call another method on the same object,
/// ensuring that `$this` is valid and method dispatch works during construction.
#[test]
fn test_class_constructor_calls_method() {
    let out = compile_and_run(
        r#"<?php
class Init { public $ready = 0;
    public function __construct() { $this->setup(); }
    public function setup() { $this->ready = 1; }
}
$i = new Init();
echo $i->ready;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that two classes composing each other (Address held inside Person) work correctly,
/// including cross-object method calls and string concatenation with an embedded object property.
#[test]
fn test_class_multiple_classes_composing() {
    let out = compile_and_run(
        r#"<?php
class Address { public $city;
    public function __construct($c) { $this->city = $c; }
}
class Person { public $name; public $address;
    public function __construct($n, $addr) { $this->name = $n; $this->address = $addr; }
    public function info() { return $this->name . " from " . $this->address->city; }
}
$addr = new Address("Rome");
$p = new Person("Marco", $addr);
echo $p->info();
"#,
    );
    assert_eq!(out, "Marco from Rome");
}

/// Verifies that a class property initialized to an empty string behaves correctly:
/// `strlen()` returns 0, concatenation produces the expected pipe-delimited output.
#[test]
fn test_class_empty_string_property() {
    let out = compile_and_run(
        r#"<?php
class Tag { public $label = "";
    public function __construct($l) { $this->label = $l; }
}
$t = new Tag("");
echo strlen($t->label) . "|" . $t->label . "|done";
"#,
    );
    assert_eq!(out, "0||done");
}

/// Verifies that a class property holding a 1000-character string is stored and retrieved
/// correctly, with `strlen()` returning the correct length.
#[test]
fn test_class_long_string_property() {
    let out = compile_and_run(
        r#"<?php
class Buffer { public $data;
    public function __construct($d) { $this->data = $d; }
}
$b = new Buffer(str_repeat("x", 1000));
echo strlen($b->data);
"#,
    );
    assert_eq!(out, "1000");
}

/// Verifies that a method can concatenate multiple string properties and return the result,
/// ensuring `$this` property reads and string concatenation work inside methods.
#[test]
fn test_class_string_concat_in_method() {
    let out = compile_and_run(
        r#"<?php
class Row { public $a; public $b; public $c;
    public function __construct($a, $b, $c) { $this->a = $a; $this->b = $b; $this->c = $c; }
    public function csv() { return $this->a . "," . $this->b . "," . $this->c; }
}
$r = new Row("x", "y", "z");
echo $r->csv();
"#,
    );
    assert_eq!(out, "x,y,z");
}

/// Verifies that a boolean property can be used in a ternary expression,
/// returning the correct branch ("yes" / "no") based on the stored `true` value.
#[test]
fn test_class_bool_property() {
    let out = compile_and_run(
        r#"<?php
class Flag { public $on;
    public function __construct($v) { $this->on = $v; }
}
$f = new Flag(true);
echo $f->on ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies that a class property holding an array works with `count()` inside a method,
/// confirming array property reads and the builtin `count()` function work correctly.
#[test]
fn test_class_array_property() {
    let out = compile_and_run(
        r#"<?php
class Stack { public $items;
    public function __construct() { $this->items = [1, 2, 3]; }
    public function size() { return count($this->items); }
}
$s = new Stack();
echo $s->size();
"#,
    );
    assert_eq!(out, "3");
}

/// Stress test: creates 1000 object instances in a loop, updating a reference each time.
/// Verifies that object allocation and last-instance tracking work correctly across many iterations.
#[test]
fn test_class_1000_objects_in_loop() {
    let out = compile_and_run(
        r#"<?php
class Obj { public $id;
    public function __construct($id) { $this->id = $id; }
}
$last = new Obj(0);
for ($i = 1; $i < 1000; $i++) {
    $last = new Obj($i);
}
echo $last->id;
"#,
    );
    assert_eq!(out, "999");
}

/// Verifies that a class with 10 properties initialized in the constructor
/// sums them correctly via a method, ensuring multi-property reads and integer arithmetic.
#[test]
fn test_class_many_properties() {
    let out = compile_and_run(
        r#"<?php
class Big { public $a; public $b; public $c; public $d; public $e;
    public $f; public $g; public $h; public $i; public $j;
    public function __construct() {
        $this->a = 1; $this->b = 2; $this->c = 3; $this->d = 4; $this->e = 5;
        $this->f = 6; $this->g = 7; $this->h = 8; $this->i = 9; $this->j = 10;
    }
    public function sum() {
        return $this->a + $this->b + $this->c + $this->d + $this->e +
               $this->f + $this->g + $this->h + $this->i + $this->j;
    }
}
$b = new Big();
echo $b->sum();
"#,
    );
    assert_eq!(out, "55");
}

/// Verifies deeply nested function calls that build nested HTML tags via string concatenation,
/// ensuring argument evaluation order, nested calls, and string concat work correctly.
#[test]
fn test_deeply_nested_string_function_calls() {
    let out = compile_and_run(
        r#"<?php
function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
echo wrap(wrap(wrap("hello", "b"), "i"), "p");
"#,
    );
    assert_eq!(out, "<p><i><b>hello</b></i></p>");
}

/// Verifies a recursive function that builds a string via repeated concatenation,
/// ensuring recursion, base-case handling, and string concat work correctly.
#[test]
fn test_recursive_string_building() {
    let out = compile_and_run(
        r#"<?php
function repeat_str($s, $n) {
    if ($n <= 0) { return ""; }
    return $s . repeat_str($s, $n - 1);
}
echo repeat_str("ab", 5);
"#,
    );
    assert_eq!(out, "ababababab");
}

/// Verifies that a closure can capture an object via `use($c)` and that the captured
/// reference remains valid after multiple method calls on the object.
#[test]
fn test_closure_capturing_object() {
    let out = compile_and_run(
        r#"<?php
class Counter { public $n = 0; public function inc() { $this->n = $this->n + 1; } }
$c = new Counter();
$c->inc();
$c->inc();
$fn = function() use ($c) { return $c; };
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that a class property storing a float is read correctly inside a method
/// and used in a floating-point arithmetic expression, producing the correct area result.
#[test]
fn test_class_float_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Circle {
    public $radius;
    public function __construct($r) { $this->radius = $r; }
    public function area() { return 3.14159 * $this->radius * $this->radius; }
}
$c = new Circle(5.0);
echo $c->area();
"#,
    );
    assert_eq!(out, "78.53975");
}

/// Verifies that a method returning a float property emits the value correctly,
/// ensuring float return types and property reads from methods work end-to-end.
#[test]
fn test_class_method_returns_float_property() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public $x;
    public function __construct($v) { $this->x = $v; }
    public function getX() { return $this->x; }
}
$f = new Foo(3.14);
echo $f->getX();
"#,
    );
    assert_eq!(out, "3.14");
}

/// Verifies that a method returning `$this` enables fluent chaining:
/// after `->add()` the object is returned and subsequent calls succeed.
#[test]
fn test_class_method_returns_this() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    public $parts = "";
    public function add($s) { $this->parts = $this->parts . $s; return $this; }
}
$b = new Builder();
$b->add("hello");
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Regression for #597: an ordinary top-level local must not seed a method's local
/// environment. The method can reuse the same name with a different type and return
/// its own value without a false reassignment error.
#[test]
fn test_method_local_is_isolated_from_same_named_top_level_local() {
    let out = compile_and_run(
        r#"<?php
$value = "top-level";
class LocalScope {
    public function value(): int {
        $value = 5;
        return $value;
    }
}
echo (new LocalScope())->value();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies that a private property is inaccessible from outside the class
/// but can be read via a public accessor method, ensuring visibility rules are enforced.
#[test]
fn test_class_private_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private $value;
    public function __construct($value) { $this->value = $value; }
    public function reveal() { return $this->value; }
}
$s = new Secret("ok");
echo $s->reveal();
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies native `method_exists()` and `property_exists()` use AOT class metadata.
#[test]
fn test_class_member_exists_builtin_uses_native_metadata() {
    let out = compile_and_run(
        r#"<?php
class NativeMemberBase {
    public $basePublic = 1;
    private $baseSecret = 2;
    private function baseHidden() { return 3; }
    protected function inheritedProtected() { return 4; }
}
class NativeMemberChild extends NativeMemberBase {
    public $childPublic = 5;
    private $childSecret = 6;
    public function run() { return 7; }
    private function hidden() { return 8; }
}
$object = new NativeMemberChild();
echo method_exists($object, "run") ? "M" : "m";
echo method_exists($object, "baseHidden") ? "B" : "b";
echo method_exists("NativeMemberChild", "baseHidden") ? "x" : "X";
echo property_exists($object, "childPublic") ? "P" : "p";
echo property_exists($object, "childSecret") ? "S" : "s";
echo property_exists($object, "baseSecret") ? "y" : "Y";
echo property_exists("NativeMemberChild", "basePublic") ? "A" : "a";
"#,
    );
    assert_eq!(out, "MBXPSYA");
}

/// Verifies that a `readonly` property can be initialized in the constructor
/// and read via a public accessor method, ensuring readonly semantics are respected.
#[test]
fn test_class_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class User {
    public readonly $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$u = new User(7);
echo $u->id();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies PHP's `==` between two objects: same class plus loosely equal properties.
///
/// `===` (identity) must keep answering instance identity, a property-less class
/// compares equal for two distinct instances, differing property values and
/// differing classes compare unequal, and property comparison is LOOSE
/// (`Box(1) == Box("1")` and `Box(0) == Box(null)` are true).
#[test]
fn test_object_loose_equality_compares_class_then_properties() {
    let out = compile_and_run(
        r#"<?php
class Empty1 {}
class Pt { public int $x = 1; public string $y = "a"; }
class Pt2 { public int $x = 1; public string $y = "a"; }
class Box { public $v; function __construct($v) { $this->v = $v; } }
$e1 = new Empty1(); $e2 = new Empty1();
var_dump($e1 == $e2, $e1 === $e2, $e1 === $e1);
$p1 = new Pt(); $p2 = new Pt();
var_dump($p1 == $p2, $p1 === $p2);
$p2->x = 2;
var_dump($p1 == $p2, $p1 != $p2);
var_dump($p1 == new Pt2());
var_dump(new Box(1) == new Box("1"), new Box(0) == new Box(null), new Box(1) == new Box(2));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(true)\n\
         bool(true)\nbool(false)\n\
         bool(false)\nbool(true)\n\
         bool(false)\n\
         bool(true)\nbool(true)\nbool(false)\n"
    );
}

/// Verifies object `==` recurses through array-valued and object-valued properties,
/// and that enum cases keep PHP's compare-by-identity behavior.
#[test]
fn test_object_loose_equality_recurses_and_handles_enums() {
    let out = compile_and_run(
        r#"<?php
class Bag { public array $items = [1, 2, 3]; }
class Pt { public int $x = 1; }
class Wrap { public ?Pt $inner = null; }
enum Suit { case Hearts; case Spades; }
enum Grade: string { case A = 'a'; case B = 'b'; }
$b1 = new Bag(); $b2 = new Bag();
var_dump($b1 == $b2);
$b2->items = [1, 2, 4];
var_dump($b1 == $b2);
$w1 = new Wrap(); $w2 = new Wrap();
var_dump($w1 == $w2);
$w1->inner = new Pt();
var_dump($w1 == $w2);
$w2->inner = new Pt();
var_dump($w1 == $w2);
var_dump(Suit::Hearts == Suit::Hearts, Suit::Hearts == Suit::Spades, Suit::Hearts === Suit::Hearts);
var_dump(Grade::A == Grade::A, Grade::A == Grade::B);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\n\
         bool(true)\nbool(false)\nbool(true)\n\
         bool(true)\nbool(false)\nbool(true)\n\
         bool(true)\nbool(false)\n"
    );
}

/// Verifies a cyclic object graph does not make `==` recurse until the stack dies.
///
/// PHP raises `Nesting level too deep - recursive dependency?`; elephc's walker caps
/// its depth and reports "not equal" instead (documented in `docs/php/classes.md`).
/// The regression this pins is that the program terminates normally.
#[test]
fn test_object_loose_equality_survives_recursive_dependency() {
    let out = compile_and_run(
        r#"<?php
class Node { public ?Node $self = null; public int $v = 1; }
$a = new Node(); $a->self = $a;
$b = new Node(); $b->self = $b;
var_dump($a == $b);
echo "survived";
"#,
    );
    assert_eq!(out, "bool(false)\nsurvived");
}
