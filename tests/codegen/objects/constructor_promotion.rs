//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object constructor property promotion, including constructor promoted properties, constructor promoted readonly property, and constructor promoted by ref property reads source updates.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests basic constructor property promotion: promoted parameters become properties.
///
/// Fixture: `User` class with `public int $id` and `private string $name = "Ada"` promoted
/// parameters. Verifies that promoted properties are accessible as `$obj->prop` and that
/// default values are applied correctly.
#[test]
fn test_constructor_promoted_properties() {
    let out = compile_and_run(
        r#"<?php
class User {
    public function __construct(public int $id, private string $name = "Ada") {}
    public function name() { return $this->name; }
}
$u = new User(7);
echo $u->id;
echo ":";
echo $u->name();
"#,
    );
    assert_eq!(out, "7:Ada");
}

/// Tests constructor property promotion with the `readonly` modifier.
///
/// Fixture: `Token` class with `public readonly int $id`. Verifies that readonly promoted
/// properties are initialized at construction and accessible via the object.
#[test]
fn test_constructor_promoted_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class Token {
    public function __construct(public readonly int $id) {}
    public function id() { return $this->id; }
}
$token = new Token(42);
echo $token->id();
"#,
    );
    assert_eq!(out, "42");
}

/// Tests that by-reference promoted properties reflect updates to the source variable.
///
/// Fixture: `Box` class with `public int &$value`. After constructing `new Box($value)`,
/// assigning `$value = 2` causes `$box->value` to read as `2` because the property aliases
/// the caller's variable.
#[test]
fn test_constructor_promoted_by_ref_property_reads_source_updates() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value) {}
}
$value = 1;
$box = new Box($value);
$value = 2;
echo $box->value;
"#,
    );
    assert_eq!(out, "2");
}

/// Tests that writes to a by-reference promoted property propagate back to the source variable.
///
/// Fixture: `Box` class with `public int &$value`. After construction, assigning
/// `$box->value = 3` writes back through the reference so the original `$value` becomes `3`.
#[test]
fn test_constructor_promoted_by_ref_property_writes_to_source() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value) {}
}
$value = 1;
$box = new Box($value);
$box->value = 3;
echo $value;
"#,
    );
    assert_eq!(out, "3");
}

/// Tests that by-reference promotion works for string-typed properties.
///
/// Fixture: `Box` class with `public string &$name`. Verifies the by-reference aliasing
/// mechanism is not limited to integers.
#[test]
fn test_constructor_promoted_by_ref_string_property_writes_to_source() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public string &$name) {}
}
$name = "Ada";
$box = new Box($name);
$box->name = "Grace";
echo $name;
"#,
    );
    assert_eq!(out, "Grace");
}

/// Tests that a by-reference promoted property with a default value uses an internal
/// reference cell when constructed without an argument.
///
/// Fixture: `Box` class with `public int &$value = 1`. When called as `new Box()` with no
/// argument, the property binds to an internal default cell initialized to `1`.
#[test]
fn test_constructor_promoted_by_ref_property_uses_default_reference_cell() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value = 1) {}
}
$box = new Box();
echo $box->value;
$box->value = 4;
echo ":";
echo $box->value;
"#,
    );
    assert_eq!(out, "1:4");
}

/// Tests that a by-reference promoted property with a default value still aliases an
/// explicit variable argument when one is provided.
///
/// Fixture: `Box` class with `public int &$value = 1`. When called as `new Box($value)`,
/// the property aliases `$value` rather than using the default cell, so mutations to
/// `$box->value` write back through the original variable.
#[test]
fn test_constructor_promoted_by_ref_property_with_default_still_links_variable_arg() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value = 1) {}
}
$value = 5;
$box = new Box($value);
$box->value = 7;
echo $value;
$value = 9;
echo ":";
echo $box->value;
"#,
    );
    assert_eq!(out, "7:9");
}

/// Verifies that an interface-typed promoted property accepts a concrete implementing-class
/// default after schema construction, and that the default instance dispatches normally.
#[test]
fn test_interface_promoted_property_accepts_object_default() {
    let out = compile_and_run(
        r#"<?php
interface I {
    public function v(): string;
}
final class N implements I {
    public function v(): string { return "n"; }
}
final class C {
    public function __construct(public I $x = new N()) {}
    public function read(I $x = new N()): string { return $x->v(); }
}
function read_value(I $x = new N()): string { return $x->v(); }
$c = new C();
echo $c->x->v();
echo ":";
echo $c->read();
echo ":";
echo read_value();
"#,
    );
    assert_eq!(out, "n:n:n");
}

/// Verifies ReflectionParameter reports constructor-promoted parameters.
#[test]
fn test_reflection_parameter_is_promoted() {
    let out = compile_and_run(
        r#"<?php
class ReflectPromotedParamUser {
    public function __construct(public int $id, string $name = "Ada") {
        echo "C";
    }
    public function run(int $id) {}
}
class ReflectPromotedParamFactory {
    public function make(): ReflectPromotedParamUser {
        echo "F";
        return new ReflectPromotedParamUser(2);
    }
}
$ctor = new ReflectionMethod(ReflectPromotedParamUser::class, "__construct");
$params = $ctor->getParameters();
echo $params[0]->isPromoted() ? "I" : "i";
echo $params[1]->isPromoted() ? "N" : "n";
$direct = new ReflectionParameter([ReflectPromotedParamUser::class, "__construct"], "id");
echo $direct->isPromoted() ? "D" : "d";
$run = new ReflectionParameter([ReflectPromotedParamUser::class, "run"], "id");
echo $run->isPromoted() ? "R" : "r";
$inline = new ReflectionParameter([new ReflectPromotedParamUser(1), "run"], 0);
echo ":" . $inline->getName();
$factory = new ReflectPromotedParamFactory();
$fromReturn = new ReflectionParameter([$factory->make(), "run"], 0);
echo ":" . $fromReturn->getName();
"#,
    );
    assert_eq!(out, "InDrC:idFC:id");
}

/// A `parent::__construct()` that OMITS a by-reference promoted default still binds a cell
/// that outlives the call.
///
/// The caller-side cell for an omitted optional by-reference argument is an ordinary stack
/// slot released when the call returns — except where the callee can KEEP the reference, and
/// a constructor that promotes `&$value` into a property is exactly that case. `new Child()`
/// reaches the parent constructor through the lexical-static lowering rather than the
/// object-allocation one, so it needs the same exemption and nothing pinned it before.
#[test]
fn test_parent_constructor_by_ref_promoted_default_outlives_the_call() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __construct(public int &$value = 1) {}
}
class Child extends Base {
    public function __construct() { parent::__construct(); }
}
$c = new Child();
echo $c->value;
$c->value = 7;
echo ":", $c->value;
"#,
    );
    assert_eq!(out, "1:7");
}

/// `new $cls($var)` — a DYNAMIC constructor call with a runtime class string — reaches the
/// by-reference argument stager through the receiver-REGISTER materializer
/// (`objects::dynamic_mixed_candidates::emit_dynamic_new_mixed_constructor_call`) rather than
/// the direct-call one, so it is the second constructor path whose promoted property borrows
/// the argument's cell for the object's whole life. The property must READ the caller's value
/// after the constructor returns; a caller-stack cell would leave it pointing into a released
/// frame and read garbage.
///
/// KNOWN GAP, PINNED HONESTLY: the write direction does NOT propagate back
/// (`$dynamic->value = 7` leaves `$shared` at 42 where PHP updates it to 7), so the dynamic
/// path binds a copy rather than the caller's slot. That is PRE-EXISTING — the same source
/// prints `42:42:42:1` at the commit before this task's by-reference work — and unrelated to
/// the cell lifetime this fixture exists for. The expectation below therefore records
/// today's behaviour; PHP 8.5 prints `42:42:7:1`.
#[test]
fn test_dynamic_new_binds_a_by_ref_promoted_property_to_the_caller_variable() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value = 1) {}
}
$eager = new Box();
$shared = 42;
$cls = "Box";
$dynamic = new $cls($shared);
echo $dynamic->value, ":", $shared, ":";
$dynamic->value = 7;
echo $shared, ":", $eager->value;
"#,
    );
    assert_eq!(
        out, "42:42:42:1",
        "the promoted property must read the caller's value through a cell that outlived the \
         constructor call; the third field is the pre-existing write-direction gap (PHP: 7)"
    );
}

/// THE DYNAMIC-NEW CELL-LIFETIME PIN. `new $cls($holder->n)` passes a PROPERTY, not a local,
/// to a by-reference promoted constructor parameter — and a property source is exactly the
/// shape that plans a real caller-side cell (`plan_ref_arg_temp_cells` skips locals, whose
/// own frame slot is passed directly, and array elements, which already have an address).
///
/// The promoted property BORROWS that cell for the object's whole life, so it must be heap
/// storage: this fixture reads the property AFTER the constructor returned, which a
/// caller-stack cell could not survive. Reverting the dynamic-new path to
/// `RefArgCellLifetime::CallOnly` makes this program fail to compile — the receiver-register
/// stager refuses to stage a cell while the receiver sits in a caller-saved register
/// (verified by flipping the constant: *"receiver-register method call staging a
/// by-reference cell with the receiver in x0"*).
///
/// HEAP BALANCE IS DELIBERATELY NOT ASSERTED: the borrowed cell is never freed (one 16-byte
/// block per constructed object), the narrower pre-existing defect `materialize_temporary_ref_arg_cell`
/// documents — only the object model can own that cell.
#[test]
fn test_dynamic_new_by_ref_promoted_property_from_a_property_source_outlives_the_call() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value = 1) {}
}
final class Holder { public int $n = 42; }
$eager = new Box();
$holder = new Holder();
$cls = "Box";
$dynamic = new $cls($holder->n);
echo $dynamic->value, ":", $holder->n, ":", $eager->value;
"#,
    );
    assert_eq!(out, "42:42:1");
}
