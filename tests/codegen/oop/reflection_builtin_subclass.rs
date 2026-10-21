//! Purpose:
//! Regressions for reflecting user subclasses of PHP's builtin Reflection classes and for
//! compiler-generated members that PHP's Reflection API never lists.
//!
//! Called from:
//! - `tests/codegen/oop.rs` in the codegen integration test suite.
//!
//! Key details:
//! - `new ReflectionClass(<name>)` materializes its whole metadata graph eagerly, and every
//!   ReflectionMethod embeds a declaring-class ReflectionClass. Listing the private `__*`
//!   backing slots of the builtin Reflection classes in each of those nested reflectors puts
//!   `new ReflectionClass('Mine')` over `class Mine extends ReflectionClass {}` close to the
//!   default 8 MiB heap, and `getProperties()` over it exhausts it.
//! - Expected outputs were measured on PHP 8.5.

use crate::support::compile_and_run;

/// Control (passes before this fix too): reflecting a user subclass of ReflectionClass must fit
/// the default heap. It guards the budget the hidden backing slots free once Reflection classes
/// gain public properties such as `name`.
#[test]
fn reflecting_a_reflection_class_subclass_fits_the_default_heap() {
    let out = compile_and_run(
        r#"<?php
class Mine extends ReflectionClass {}
echo (new ReflectionClass('Mine'))->getName(), "\n";
"#,
    );
    assert_eq!(out, "Mine\n");
}

/// The private `__*` storage slots elephc declares on builtin Reflection classes are not PHP
/// properties.
#[test]
fn reflection_class_backing_slots_are_not_reflected() {
    let out = compile_and_run(
        r#"<?php
class Mine extends ReflectionClass {}
$mine = new ReflectionClass('Mine');
$declaring = $mine->getMethod('getName')->getDeclaringClass();
echo $declaring->getName(), "\n";
var_dump($declaring->getDefaultProperties());
var_dump($declaring->hasProperty('__name'));
"#,
    );
    assert_eq!(out, "ReflectionClass\narray(0) {\n}\nbool(false)\n");
}

/// A user property hook compiles to `__propget_*` / `__propset_*` methods that PHP's
/// `getMethods()` and `hasMethod()` never report.
#[test]
fn property_hook_accessors_are_not_reflected_as_methods() {
    let out = compile_and_run(
        r#"<?php
class Hooked {
    public string $label { get => "x"; set { } }
    public function run(): void {}
}
$names = [];
foreach ((new ReflectionClass('Hooked'))->getMethods() as $method) {
    $names[] = $method->getName();
}
echo implode(",", $names), "\n";
var_dump((new ReflectionClass('Hooked'))->hasMethod('__propget_label'));
var_dump((new ReflectionClass('Hooked'))->hasMethod('__propset_label'));
"#,
    );
    assert_eq!(out, "run\nbool(false)\nbool(false)\n");
}

/// `getProperties()` on a receiver known at lowering time is expanded statically; it must apply
/// the same listing rule as the runtime metadata, so the builtin Reflection `__*` storage slots
/// never surface there either.
#[test]
fn static_get_properties_hides_reflection_backing_slots() {
    let out = compile_and_run(
        r#"<?php
class Mine extends ReflectionClass {}
$rc = new ReflectionClass('Mine');
foreach ($rc->getProperties() as $p) {
    if (str_starts_with($p->getName(), "__")) { echo "slot: ", $p->getName(), "\n"; }
}
echo count($rc->getProperties(ReflectionProperty::IS_PRIVATE)), "\n";
"#,
    );
    assert_eq!(out, "0\n");
}

/// PHP's `getProperties()` never lists a parent's private properties (instance or static), on
/// the statically expanded path too.
#[test]
fn static_get_properties_skips_inherited_private_properties() {
    let out = compile_and_run(
        r#"<?php
class P { private $__y; private $z; private static $__x; public $pub; }
class C extends P { private $own; }
$rc = new ReflectionClass('C');
$names = [];
foreach ($rc->getProperties() as $p) { $names[] = $p->getName(); }
sort($names);
echo implode(",", $names), "\n";
echo count($rc->getProperties(ReflectionProperty::IS_PRIVATE)), "\n";
echo $rc->getProperties(ReflectionProperty::IS_PRIVATE)[0]->getName(), "\n";
"#,
    );
    assert_eq!(out, "own,pub\n1\nown\n");
}
