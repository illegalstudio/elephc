//! Purpose:
//! Integration or regression tests for diagnostic coverage of exception, enum, and magic-constant diagnostics, including magic method contracts collect multiple errors, try requires catch or finally, and throw requires object.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Covariant returns see every enum's transitive interfaces regardless of HashMap order.
#[test]
fn test_enum_followup_cross_enum_transitive_return() {
    let source = "<?php interface P {} interface C extends P {} interface Factory { public function create(): P; } enum Value implements C { case A; } enum Maker implements Factory { case A; public function create(): Value { return Value::A; } }";
    for _ in 0..64 { expect_no_error(source); }
}

/// Both instance and static enum implementations may add optional parameters or defaults.
#[test]
fn test_enum_followup_optional_interface_parameters() {
    for declaration in [
        "public function f(int $x, int $y = 0): int { return $x; }",
        "public function f(int $x = 1): int { return $x; }",
        "public static function f(int $x, int $y = 0): int { return $x; }",
        "public static function f(int $x = 1): int { return $x; }",
    ] {
        let modifier = if declaration.contains("static") { "static " } else { "" };
        expect_no_error(&format!("<?php interface I {{ public {modifier}function f(int $x): int; }} enum E implements I {{ case A; {declaration} }}"));
    }
}

/// Optional widening does not permit extra required arguments or changed reference passing.
#[test]
fn test_error_enum_followup_interface_parameter_narrowing() {
    for declaration in [
        "public function f(int $x, int $y): int { return $x; }",
        "public function f(int &$x): int { return $x; }",
    ] {
        expect_error(&format!("<?php interface I {{ public function f(int $x): int; }} enum E implements I {{ case A; {declaration} }}"), "Incompatible parameter shape");
    }
    expect_error("<?php interface I { public function f(int $x = 1): int; } enum E implements I { case A; public function f(int $x): int { return $x; } }", "Incompatible parameter shape");
}

/// An alias of an abstract trait method keeps its parameter and return requirements.
#[test]
fn test_error_enum_review_abstract_alias_contract() {
    expect_error("<?php trait T { abstract public function f(int $x): int; } enum E { use T { f as g; } case A; public function f(int $x): int { return $x; } public function g(string $x): string { return $x; } }", "Cannot narrow parameter");
    expect_error("<?php trait T { abstract public function f(int $x): int; } trait M { use T { f as g; } } enum E { use M; case A; public function f(int $x): int { return $x; } public function g(string $x): string { return $x; } }", "Cannot narrow parameter");
    expect_error("<?php trait T { abstract public function f(int $x): int; } trait B { public function f(string $x): string { return $x; } } enum E { use T, B { B::f insteadof T; } case A; }", "Cannot narrow parameter");
}

/// Enums must implement instance and static interface requirements before EIR lowering.
#[test]
fn test_error_enum_review_missing_interface_contract() {
    for declaration in ["public function f(): int;", "public static function f(): int;"] {
        expect_error(&format!("<?php interface I {{ {declaration} }} enum E implements I {{ case A; }}"), "must implement interface");
    }
    expect_error("<?php interface ParentContract { public function f(): int; } interface I extends ParentContract {} enum E implements I { case A; }", "must implement interface");
}

/// Incorrect parameter, return and visibility contracts are rejected on enum implementations.
#[test]
fn test_error_enum_review_incompatible_interface_contract() {
    expect_error("<?php interface I { public function f(int $x): int; } enum E implements I { case A; public function f(int $x): string { return 'bad'; } }", "incompatible return type");
    expect_error("<?php interface I { public function f(): int; } enum E implements I { case A; protected function f(): int { return 1; } }", "public");
    expect_error("<?php interface I { public function f(int $x): int; } enum E implements I { case A; public function f(string $x): int { return 1; } }", "Cannot narrow interface parameter");
    expect_error("<?php interface I { public static function f(): int; } enum E implements I { case A; public static function f(): string { return 'bad'; } }", "incompatible return type");
    expect_error("<?php interface I { public function &f(): int; } enum E implements I { case A; public function f(): int { return 1; } }", "Cannot remove by-reference return");
}

/// Optional trait parameters cannot become required, and extra required parameters narrow calls.
#[test]
fn test_error_enum_review_required_trait_parameter_shape() {
    expect_error("<?php trait T { abstract public function f(int $x = 1): int; } enum E { case A; use T; public function f(int $x): int { return $x; } }", "Incompatible parameter shape");
    expect_error("<?php trait T { abstract public function f(int $x): int; } enum E { case A; use T; public function f(int $x, int $extra): int { return $x; } }", "Incompatible parameter shape");
}

/// Compiler-added argument collectors do not constrain the PHP-visible optional trait shape.
#[test]
fn test_enum_review_optional_trait_shape_ignores_generated_parameters() {
    expect_no_error("<?php trait T { abstract public function f(int $x): int; } enum E { case A; use T; public function f(int $x = 1, int $extra = 2): int { return $x + $extra; } } debug_print_backtrace(); echo E::A->f();");
}

/// Direct and nested trait constants cannot reuse a pure or backed enum case name.
#[test]
fn test_error_enum_trait_constant_conflicts_with_case() {
    for source in [
        "<?php trait T { const A = 1; } enum E { use T; case A; }",
        "<?php trait T { const A = 1; } enum E: int { case A = 1; use T; }",
        "<?php trait Inner { const A = 1; } trait T { use Inner; } enum E { use T; case A; }",
    ] {
        expect_error(source, "Enum constant E::A conflicts with enum case");
    }
}

/// Enum cases and imported trait constants keep PHP's case-sensitive constant names.
#[test]
fn test_enum_trait_constant_case_sensitive_names() {
    expect_no_error("<?php trait T { const a = 1; } enum E { use T; case A; } echo E::a;");
}

/// Enum declarations cannot leave abstract instance or static methods unimplemented.
#[test]
fn test_error_enum_cannot_declare_abstract_methods() {
    for declaration in ["abstract public function missing();", "abstract public static function missing();"] {
        expect_error(&format!("<?php enum Mode {{ case Active; {declaration} }}"), "Enum method Mode::missing cannot be abstract");
    }
    expect_error("<?php trait T { abstract public function missing(); } enum Mode { use T; case Active; }", "Enum method Mode::missing cannot be abstract");
}

/// Verifies that checking multiple classes with conflicting magic method contracts
/// (private vs public `__toString`) produces at least two distinct errors.
/// Uses `check_source_full` to collect and flatten all diagnostics.
#[test]
fn test_error_magic_method_contracts_collect_multiple_errors() {
    let error = check_source_full(
        "<?php class A { private function __toString() { return \"x\"; } } class B { public static function __toString() { return \"y\"; } }",
    )
    .unwrap_err();
    let all = error.flatten();
    assert!(
        all.len() >= 2,
        "expected multiple magic method contract errors, got {:?}",
        all.iter()
            .map(|error| error.message.clone())
            .collect::<Vec<_>>(),
    );
}

/// Verifies that a `try` block without a `catch` or `finally` clause
/// reports "Expected at least one catch or a finally block after try".
#[test]
fn test_error_try_requires_catch_or_finally() {
    expect_error(
        "<?php try { echo 1; }",
        "Expected at least one catch or a finally block after try",
    );
}

/// Verifies that `throw 123` (non-object operand) reports
/// "throw requires an object value".
#[test]
fn test_error_throw_requires_object() {
    expect_error("<?php throw 123;", "throw requires an object value");
}

/// Verifies that `new Color()` on a backed enum reports
/// "Cannot instantiate enum: Color".
#[test]
fn test_error_enum_cannot_be_instantiated() {
    expect_error(
        "<?php enum Color: int { case Red = 1; } $x = new Color();",
        "Cannot instantiate enum: Color",
    );
}

/// Verifies that a backed enum case without an explicit value
/// (e.g., `case Red;` in `enum Color: int`) reports
/// "Backed enum cases must declare a value".
#[test]
fn test_error_backed_enum_case_requires_value() {
    expect_error(
        "<?php enum Color: int { case Red; }",
        "Backed enum cases must declare a value",
    );
}

/// Verifies that a pure (unbacked) enum case with a backing value
/// (e.g., `case Hearts = 1`) reports
/// "Pure enum cases cannot declare a backing value".
#[test]
fn test_error_pure_enum_case_cannot_have_backing_value() {
    expect_error(
        "<?php enum Suit { case Hearts = 1; }",
        "Pure enum cases cannot declare a backing value",
    );
}

/// Verifies that a backed enum with two cases sharing the same backing value
/// (e.g., `case Red = 1; case Crimson = 1`) reports
/// "Duplicate enum backing value".
#[test]
fn test_error_enum_duplicate_backing_value() {
    expect_error(
        "<?php enum Color: int { case Red = 1; case Crimson = 1; }",
        "Duplicate enum backing value",
    );
}

/// Verifies that calling `Suit::from(1)` on a pure enum reports
/// "Undefined method: Suit::from" (backed enums only get `from`).
#[test]
fn test_error_pure_enum_has_no_from_method() {
    expect_error(
        "<?php enum Suit { case Hearts; } Suit::from(1);",
        "Undefined method: Suit::from",
    );
}

/// Verifies that throwing a class that does not implement `Throwable`
/// (e.g., `class PlainObject {}`) reports
/// "throw requires an object implementing Throwable".
#[test]
fn test_error_throw_requires_throwable() {
    expect_error(
        "<?php class PlainObject {} throw new PlainObject();",
        "throw requires an object implementing Throwable",
    );
}

/// Verifies that a throw expression in a null-coalescing chain
/// (`$value = null ?? throw 123`) with a non-object operand reports
/// "throw requires an object value".
#[test]
fn test_error_throw_expression_requires_object() {
    expect_error(
        "<?php $value = null ?? throw 123;",
        "throw requires an object value",
    );
}

/// Verifies that `clone` rejects scalar operands during type checking.
#[test]
fn test_error_clone_requires_object() {
    expect_error("<?php $copy = clone 123;", "clone requires an object value");
}

/// Verifies that a `static` `__clone` reports
/// "Magic method must be non-static: User::__clone".
#[test]
fn test_error_magic_clone_must_be_non_static() {
    expect_error(
        "<?php class User { public static function __clone() { } }",
        "Magic method must be non-static: User::__clone",
    );
}

/// Verifies that `__clone` with a parameter reports
/// "Magic method must take 0 arguments: User::__clone".
#[test]
fn test_error_magic_clone_must_take_zero_arguments() {
    expect_error(
        "<?php class User { public function __clone($x) { } }",
        "Magic method must take 0 arguments: User::__clone",
    );
}

/// Verifies that `__clone` with a non-void declared return type reports
/// "Magic method must return void: User::__clone".
#[test]
fn test_error_magic_clone_must_return_void() {
    expect_error(
        "<?php class User { public function __clone(): int { return 1; } }",
        "Magic method must return void: User::__clone",
    );
}

/// Verifies that a private `__toString` method reports
/// "Magic method must be public: User::__toString".
#[test]
fn test_error_magic_tostring_must_be_public() {
    expect_error(
        "<?php class User { private function __toString() { return \"x\"; } }",
        "Magic method must be public: User::__toString",
    );
}

/// Verifies that `__toString` with a parameter reports
/// "Magic method must take 0 arguments: User::__toString".
#[test]
fn test_error_magic_tostring_must_take_zero_arguments() {
    expect_error(
        "<?php class User { public function __toString($x) { return \"x\"; } }",
        "Magic method must take 0 arguments: User::__toString",
    );
}

/// Verifies that `__toString` returning an integer reports
/// "Magic method must return string: User::__toString".
#[test]
fn test_error_magic_tostring_must_return_string() {
    expect_error(
        "<?php class User { public function __toString() { return 123; } }",
        "Magic method must return string: User::__toString",
    );
}

/// Verifies that `__get` with no parameters reports
/// "Magic method must take 1 argument: Bag::__get".
#[test]
fn test_error_magic_get_must_take_one_argument() {
    expect_error(
        "<?php class Bag { public function __get() { return 1; } }",
        "Magic method must take 1 argument: Bag::__get",
    );
}

/// Verifies that a private `__set` method reports
/// "Magic method must be public: Bag::__set".
#[test]
fn test_error_magic_set_must_be_public() {
    expect_error(
        "<?php class Bag { private function __set($name, $value) { } }",
        "Magic method must be public: Bag::__set",
    );
}

/// Verifies that a `static` `__destruct` reports
/// "Magic method must be non-static: Conn::__destruct".
#[test]
fn test_error_magic_destruct_must_be_non_static() {
    expect_error(
        "<?php class Conn { public static function __destruct() { } }",
        "Magic method must be non-static: Conn::__destruct",
    );
}

/// Verifies that `__destruct` declared with a parameter reports
/// "Magic method must take 0 arguments: Conn::__destruct".
#[test]
fn test_error_magic_destruct_must_take_zero_arguments() {
    expect_error(
        "<?php class Conn { public function __destruct($x) { } }",
        "Magic method must take 0 arguments: Conn::__destruct",
    );
}

/// Verifies that `__set` with only one parameter reports
/// "Magic method must take 2 arguments: Bag::__set".
#[test]
fn test_error_magic_set_must_take_two_arguments() {
    expect_error(
        "<?php class Bag { public function __set($name) { } }",
        "Magic method must take 2 arguments: Bag::__set",
    );
}

/// Verifies that a `static` `__isset` reports
/// "Magic method must be non-static: Bag::__isset".
#[test]
fn test_error_magic_isset_must_be_non_static() {
    expect_error(
        "<?php class Bag { public static function __isset($name) { return true; } }",
        "Magic method must be non-static: Bag::__isset",
    );
}

/// Verifies that a private `__isset` method reports
/// "Magic method must be public: Bag::__isset".
#[test]
fn test_error_magic_isset_must_be_public() {
    expect_error(
        "<?php class Bag { private function __isset($name) { return true; } }",
        "Magic method must be public: Bag::__isset",
    );
}

/// Verifies that `__isset` with no parameters reports
/// "Magic method must take 1 argument: Bag::__isset".
#[test]
fn test_error_magic_isset_must_take_one_argument() {
    expect_error(
        "<?php class Bag { public function __isset() { return true; } }",
        "Magic method must take 1 argument: Bag::__isset",
    );
}

/// Verifies that `__isset` with a non-bool declared return type reports
/// "Magic method must return bool: Bag::__isset".
#[test]
fn test_error_magic_isset_declared_return_type_must_be_bool() {
    expect_error(
        "<?php class Bag { public function __isset($name): string { return \"yes\"; } }",
        "Magic method must return bool: Bag::__isset",
    );
}

/// Verifies that a `static` `__unset` reports
/// "Magic method must be non-static: Bag::__unset".
#[test]
fn test_error_magic_unset_must_be_non_static() {
    expect_error(
        "<?php class Bag { public static function __unset($name) { } }",
        "Magic method must be non-static: Bag::__unset",
    );
}

/// Verifies that a private `__unset` method reports
/// "Magic method must be public: Bag::__unset".
#[test]
fn test_error_magic_unset_must_be_public() {
    expect_error(
        "<?php class Bag { private function __unset($name) { } }",
        "Magic method must be public: Bag::__unset",
    );
}

/// Verifies that `__unset` with no parameters reports
/// "Magic method must take 1 argument: Bag::__unset".
#[test]
fn test_error_magic_unset_must_take_one_argument() {
    expect_error(
        "<?php class Bag { public function __unset() { } }",
        "Magic method must take 1 argument: Bag::__unset",
    );
}

/// Verifies that `__unset` with a non-void declared return type reports
/// "Magic method must return void: Bag::__unset".
#[test]
fn test_error_magic_unset_declared_return_type_must_be_void() {
    expect_error(
        "<?php class Bag { public function __unset($name): bool { return true; } }",
        "Magic method must return void: Bag::__unset",
    );
}

/// Verifies that unsetting an inaccessible property without `__unset` remains an access error.
#[test]
fn test_error_unset_private_property_without_magic_unset() {
    expect_error(
        "<?php class Bag { private $token = 1; } $bag = new Bag(); unset($bag->token);",
        "Cannot access private property: Bag::token",
    );
}

/// Verifies that `__call` with only one parameter reports
/// "Magic method must take 2 arguments: Proxy::__call".
#[test]
fn test_error_magic_call_must_take_two_arguments() {
    expect_error(
        "<?php class Proxy { public function __call($name) { return 1; } }",
        "Magic method must take 2 arguments: Proxy::__call",
    );
}

/// Verifies that a private `__call` method reports
/// "Magic method must be public: Proxy::__call".
#[test]
fn test_error_magic_call_must_be_public() {
    expect_error(
        "<?php class Proxy { private function __call($name, $args) { return 1; } }",
        "Magic method must be public: Proxy::__call",
    );
}

/// Verifies that non-static `__callStatic` reports
/// "Magic method must be static: Proxy::__callStatic".
#[test]
fn test_error_magic_call_static_must_be_static() {
    expect_error(
        "<?php class Proxy { public function __callStatic($name, $args) { return 1; } }",
        "Magic method must be static: Proxy::__callStatic",
    );
}

/// Verifies that `__callStatic` with only one parameter reports
/// "Magic method must take 2 arguments: Proxy::__callStatic".
#[test]
fn test_error_magic_call_static_must_take_two_arguments() {
    expect_error(
        "<?php class Proxy { public static function __callStatic($name) { return 1; } }",
        "Magic method must take 2 arguments: Proxy::__callStatic",
    );
}

/// Verifies that a private `__callStatic` method reports
/// "Magic method must be public: Proxy::__callStatic".
#[test]
fn test_error_magic_call_static_must_be_public() {
    expect_error(
        "<?php class Proxy { private static function __callStatic($name, $args) { return 1; } }",
        "Magic method must be public: Proxy::__callStatic",
    );
}

/// Verifies that a private `__invoke` method reports
/// "Magic method must be public: Handler::__invoke".
#[test]
fn test_error_magic_invoke_must_be_public() {
    expect_error(
        "<?php class Handler { private function __invoke($value) { return $value; } }",
        "Magic method must be public: Handler::__invoke",
    );
}

/// Verifies that a non-static `__callStatic` reports
/// "Magic method must be static: Api::__callStatic".
#[test]
fn test_error_magic_callstatic_must_be_static() {
    expect_error(
        "<?php class Api { public function __callStatic($name, $args) { return 1; } }",
        "Magic method must be static: Api::__callStatic",
    );
}

/// Verifies that `__callStatic` with only one parameter reports
/// "Magic method must take 2 arguments: Api::__callStatic".
#[test]
fn test_error_magic_callstatic_must_take_two_arguments() {
    expect_error(
        "<?php class Api { public static function __callStatic($name) { return 1; } }",
        "Magic method must take 2 arguments: Api::__callStatic",
    );
}

/// Verifies that a private `__callStatic` method reports
/// "Magic method must be public: Api::__callStatic".
#[test]
fn test_error_magic_callstatic_must_be_public() {
    expect_error(
        "<?php class Api { private static function __callStatic($name, $args) { return 1; } }",
        "Magic method must be public: Api::__callStatic",
    );
}

/// Verifies that `__unset` with two parameters reports
/// "Magic method must take 1 argument: Bag::__unset".
#[test]
fn test_error_magic_unset_must_not_take_two_arguments() {
    expect_error(
        "<?php class Bag { public function __unset($name, $extra) { } }",
        "Magic method must take 1 argument: Bag::__unset",
    );
}

/// Verifies that `catch (MissingException $e)` with an undefined class
/// reports "Undefined class: MissingException".
#[test]
fn test_error_catch_requires_defined_class() {
    expect_error(
        "<?php try { echo 1; } catch (MissingException $e) { echo 2; }",
        "Undefined class: MissingException",
    );
}

/// Verifies that catching a plain class not implementing `Throwable`
/// (e.g., `catch (PlainObject $e)`) reports
/// "Catch type must extend or implement Throwable: PlainObject".
#[test]
fn test_error_catch_requires_throwable_type() {
    expect_error(
        "<?php class PlainObject {} try { throw new Exception(); } catch (PlainObject $e) { echo 2; }",
        "Catch type must extend or implement Throwable: PlainObject",
    );
}

/// Verifies that redeclaring the built-in `Exception` class
/// reports "Cannot redeclare built-in type: Exception".
#[test]
fn test_error_cannot_redeclare_builtin_exception_type() {
    expect_error(
        "<?php class Exception {}",
        "Cannot redeclare built-in type: Exception",
    );
}

/// Verifies that redeclaring the built-in `Error` class
/// reports "Cannot redeclare built-in type: Error".
#[test]
fn test_error_cannot_redeclare_builtin_error_type() {
    expect_error(
        "<?php class Error {}",
        "Cannot redeclare built-in type: Error",
    );
}

/// Verifies that redeclaring the PHP 8.6 builtin `SortDirection` enum reports
/// a built-in type redeclaration diagnostic.
#[test]
fn test_error_cannot_redeclare_builtin_sort_direction_enum() {
    expect_error(
        "<?php enum SortDirection { case Up; }",
        "Cannot redeclare built-in type: SortDirection",
    );
}

/// Verifies that class-like declarations cannot reuse the builtin
/// `SortDirection` enum name.
#[test]
fn test_error_cannot_redeclare_builtin_sort_direction_class() {
    expect_error(
        "<?php class SortDirection {}",
        "Cannot redeclare built-in type: SortDirection",
    );
}

/// Verifies that unknown cases on the builtin `SortDirection` enum report the
/// same enum-case diagnostic as user-declared enums.
#[test]
fn test_error_builtin_sort_direction_unknown_case() {
    expect_error(
        "<?php SortDirection::Sideways;",
        "Undefined enum case: SortDirection::Sideways",
    );
}

/// Verifies that directly instantiating the `Throwable` interface
/// (`$e = new Throwable();`) reports "Cannot instantiate interface: Throwable".
#[test]
fn test_error_cannot_instantiate_throwable_interface() {
    expect_error(
        "<?php $e = new Throwable();",
        "Cannot instantiate interface: Throwable",
    );
}

/// Verifies that a `case` declaration outside of an enum body is rejected.
#[test]
fn test_error_case_outside_enum() {
    expect_error(
        "<?php class C { case Foo; }",
        "'case' is only valid inside an enum",
    );
}

/// Verifies that enum trait use rejects imported properties because PHP enums cannot have them.
#[test]
fn test_error_enum_trait_with_property() {
    expect_error(
        "<?php trait T { public int $value; } enum E { use T; case A; }",
        "Enums cannot use traits with properties",
    );
}

/// A local enum method cannot discard an abstract trait's parameter contract.
#[test]
fn test_error_enum_review_trait_parameter_requirement() {
    expect_error(
        "<?php trait T { abstract public function f(int $n): int; } enum E { case Ready; use T; public function f(string $n): int { return 1; } }",
        "Cannot narrow parameter",
    );
}

/// An abstract requirement survives an intermediate trait's concrete implementation.
#[test]
fn test_error_enum_review_nested_trait_return_requirement() {
    expect_error(
        "<?php trait T { abstract public function f(): int; } trait Middle { use T; public function f(): int { return 1; } } enum E { case Ready; use Middle; public function f(): string { return 'bad'; } }",
        "incompatible return type",
    );
}

/// An implementation cannot discard an abstract trait's by-reference return contract.
#[test]
fn test_error_enum_review_trait_by_reference_return_requirement() {
    expect_error(
        "<?php trait T { abstract public function &f(): int; } enum E { case Ready; use T; public function f(): int { return 1; } }",
        "Cannot remove by-reference return",
    );
}

/// A by-value abstract trait requirement permits an implementation returning by reference.
#[test]
fn test_enum_review_trait_can_add_by_reference_return() {
    expect_no_error("<?php trait T { abstract public function f(): int; } enum E { case Ready; use T; public function &f(): int { static $n = 1; return $n; } }");
}

/// Non-abstract enum methods need bodies even when parsing a semicolon declaration succeeds.
#[test]
fn test_error_enum_review_method_without_body() {
    for declaration in ["public function f();", "public static function f(): int;"] {
        expect_error(
            &format!("<?php enum E {{ case Ready; {declaration} }}"),
            "Non-abstract method must have a body",
        );
    }
}

/// Verifies that an enum method body is type-checked like a class method: a declared return type
/// that does not match the returned value is rejected. Regression test — enum method bodies
/// previously bypassed type checking entirely.
#[test]
fn test_error_enum_method_return_type_mismatch() {
    expect_error(
        "<?php enum E { case A; public function f(): int { return \"nope\"; } }",
        "Method 'E::f' return type expects int, got string",
    );
}

/// Verifies that an undefined variable referenced inside an enum method body is reported.
/// Regression test — enum method bodies previously skipped name/variable checking.
#[test]
fn test_error_enum_method_undefined_variable() {
    expect_error(
        "<?php enum E { case A; public function f(): int { return $missing; } }",
        "Undefined variable: $missing",
    );
}

/// Verifies that the hidden argument collector `func_args` appends to a scope that uses
/// `func_num_args()` does not make a zero-argument magic method look variadic.
///
/// The collector occupies the same AST slot as a source `...$args`, so an arity rule that
/// only asked whether a variadic was present rejected `__destruct`/`__toString` bodies PHP
/// accepts. Only the source-visible signature may decide the contract.
#[test]
fn test_magic_methods_accept_the_generated_func_args_collector() {
    expect_no_error(
        "<?php class Logged { public function __toString(): string { return \"n\" . func_num_args(); } public function __destruct() { echo func_num_args(); } } echo new Logged();",
    );
}

/// Verifies the same rule under the whole-program trigger: any `debug_print_backtrace()` in the
/// program gives EVERY frame a hidden collector, including magic methods that never mention an
/// introspection construct themselves.
#[test]
fn test_magic_methods_accept_the_collector_added_to_every_frame() {
    expect_no_error(
        "<?php class Traced { public function __unset(string $name): void { echo $name; } public function __clone() { echo \"c\"; } } function trace(): void { debug_print_backtrace(); } trace(); $t = new Traced(); echo get_class($t);",
    );
}

/// Verifies that a magic method declaring its OWN variadic is still rejected while the program
/// also carries the generated collector. The fix filters exactly one reserved parameter name,
/// so a PHP-visible `...$extra` must keep failing the contract.
#[test]
fn test_magic_destruct_still_rejects_a_source_variadic() {
    expect_error(
        "<?php class Bad { public function __destruct(...$extra) { echo count($extra); } } function trace(): void { debug_print_backtrace(); } trace(); echo get_class(new Bad());",
        "Magic method must take 0 arguments: Bad::__destruct",
    );
}

/// Verifies the arity half of the same rule: a source parameter beyond the contract is still
/// counted even when the generated collector is present.
#[test]
fn test_magic_unset_still_rejects_an_extra_source_parameter() {
    expect_error(
        "<?php class Bad { public function __unset(string $name, int $extra): void { echo $name, $extra; } } function trace(): void { debug_print_backtrace(); } trace(); echo get_class(new Bad());",
        "Magic method must take 1 argument: Bad::__unset",
    );
}
