//! Purpose:
//! End-to-end regressions for issue #1118: the `ReflectionUnionType` OBJECT must render its
//! members the way PHP does, which is not the way they were declared.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_union_type` through Rust's test harness.
//!
//! Key details:
//! - PHP renders a union from its internal type mask, which has a fixed order, so `int|string`
//!   prints as `string|int`. #1080 gave `ReflectionMethod::__toString()` that order through
//!   `reflection_union_member_rank`; the type object returned by `getType()` kept the declared
//!   one, so the same program disagreed with itself.
//! - Two members were worse than mis-ordered. `false` fell through the `PhpType` mapper's
//!   catch-all to `None`, which made the whole union unrepresentable and rendered it as the
//!   EMPTY string; a bare `object` hint resolves to `PhpType::Object` with no name, and copying
//!   that name printed nothing, leaving `array||string`.
//! - Not fixed here, and unchanged by this work: `true` prints as `bool` and `iterable` is not
//!   expanded to `Traversable|array`. Both are member NAMING, both predate this issue, and both
//!   affect the dump and the type object alike — so the two surfaces still agree with each
//!   other, which is what #1118 was about.

use crate::support::compile_and_run;

/// Verifies the six rows the issue was filed with.
#[test]
fn test_union_type_object_renders_php_member_order() {
    let out = compile_and_run(
        r#"<?php
class C {}
interface I {}

class U {
    public function a(int|string $v) {}
    public function d(bool|array|string $v) {}
    public function f(C|int|string $v) {}
    public function g(int|C|I $v) {}
    public function h(false|int $v) {}
    public function i(array|object|string $v) {}
}

echo (string) (new ReflectionMethod('U', 'a'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('U', 'd'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('U', 'f'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('U', 'g'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('U', 'h'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('U', 'i'))->getParameters()[0]->getType(), "|";
"#,
    );

    assert_eq!(
        out,
        "string|int|array|string|bool|C|string|int|C|I|int|int|false|object|array|string|"
    );
}

/// Verifies the type object and the parameter dump agree, which is the disagreement the issue
/// reported rather than the ordering itself.
#[test]
fn test_union_type_object_agrees_with_the_parameter_dump() {
    let out = compile_and_run(
        r#"<?php
class A2 {}

class V {
    public function m(A2|int|string $v) {}
}

$p = (new ReflectionMethod('V', 'm'))->getParameters()[0];
echo (string) $p, "|";
echo (string) $p->getType(), "|";
"#,
    );

    assert_eq!(out, "Parameter #0 [ <required> A2|string|int $v ]|A2|string|int|");
}

/// Verifies the order holds on the other surfaces a union can reach: a return type, a typed
/// property, and a union that also allows null.
#[test]
fn test_union_member_order_holds_for_returns_properties_and_nullables() {
    let out = compile_and_run(
        r#"<?php
class C3 {}

class W {
    public int|string $prop = 1;
    public C3|false|array $mixedProp = false;

    public function ret(): int|string { return 1; }
    public function retObj(): object|array|string { return []; }
    public function nullable(int|string|null $v) {}
    public function twoClasses(C3|float|int $v) {}
    public function callableUnion(callable|string $v) {}
}

echo (string) (new ReflectionMethod('W', 'ret'))->getReturnType(), "|";
echo (string) (new ReflectionMethod('W', 'retObj'))->getReturnType(), "|";
echo (string) (new ReflectionMethod('W', 'nullable'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('W', 'twoClasses'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionMethod('W', 'callableUnion'))->getParameters()[0]->getType(), "|";
echo (string) (new ReflectionProperty('W', 'prop'))->getType(), "|";
echo (string) (new ReflectionProperty('W', 'mixedProp'))->getType(), "|";
"#,
    );

    assert_eq!(
        out,
        "string|int|object|array|string|string|int|null|C3|int|float|callable|string|string|int|C3|array|false|"
    );
}

/// Verifies a union that reduces to a single member after `null` is removed still answers as a
/// nullable NAMED type rather than a one-member union, which the ordering change must not
/// disturb.
#[test]
fn test_a_single_member_union_stays_a_nullable_named_type() {
    let out = compile_and_run(
        r#"<?php
class X {
    public function single(int|null $v) {}
}

$t = (new ReflectionMethod('X', 'single'))->getParameters()[0]->getType();
echo (string) $t, "|";
echo $t->allowsNull() ? "yes" : "no", "|";
echo $t->getName(), "|";
"#,
    );

    assert_eq!(out, "?int|yes|int|");
}
