//! Purpose:
//! Integration or regression tests for diagnostic coverage of type system, including null coalesce assignment missing rhs, null coalesce assignment type change, and string index requires integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies that `??=` with no right-hand side expression produces an "Unexpected token" error.
/// Input: `$x ??=;` — the semicolon terminates the expression with no RHS.
#[test]
fn test_error_null_coalesce_assignment_missing_rhs() {
    expect_error("<?php $x ??=;", "Unexpected token");
}

/// Verifies that `??=` rejects a type-changing initializer on an existing typed variable.
/// Input: `$x = 5; $x ??= 2.5;` — `$x` is int, RHS is float, which widens and is rejected.
#[test]
fn test_error_null_coalesce_assignment_type_change() {
    expect_error(
        "<?php $x = 5; $x ??= 2.5;",
        "null coalescing assignment for $x must keep int, got float",
    );
}

/// Verifies that a non-integer string subscript is rejected on a string value.
/// Input: `$s = "hello"; echo $s["x"];` — string key "x" is not integer.
#[test]
fn test_error_string_index_requires_integer() {
    expect_error(
        "<?php $s = \"hello\"; echo $s[\"x\"];",
        "String index must be integer",
    );
}

/// Verifies that assigning to a string offset (character replacement) is rejected.
/// Input: `$s = "hello"; $s[0] = "H";` — offset assignment on a string is unsupported.
#[test]
fn test_error_string_offset_assignment_is_not_supported() {
    expect_error(
        "<?php $s = \"hello\"; $s[0] = \"H\";",
        "String offset assignment is not supported",
    );
}

/// Verifies that by-reference foreach over a parameter typed `iterable` is rejected.
/// Input: `function f(iterable $items) { foreach ($items as &$value) {} }`
#[test]
fn test_error_by_reference_foreach_rejects_iterable_type() {
    expect_error(
        "<?php function f(iterable $items) { foreach ($items as &$value) {} }",
        "by-reference foreach over Iterator/IteratorAggregate objects",
    );
}

/// Verifies that by-reference foreach over a parameter typed `Iterator` is rejected.
/// Input: `function f(Iterator $items) { foreach ($items as &$value) {} }`
#[test]
fn test_error_by_reference_foreach_rejects_iterator_object_type() {
    expect_error(
        "<?php function f(Iterator $items) { foreach ($items as &$value) {} }",
        "by-reference foreach over Iterator/IteratorAggregate objects",
    );
}

/// Verifies that by-reference foreach over a concrete class implementing `Iterator` is rejected.
/// Uses a `Counter` class that implements Iterator with an int counter field.
#[test]
fn test_error_by_reference_foreach_rejects_concrete_iterator_object() {
    expect_error(
        r#"<?php
class Counter implements Iterator {
    private int $i = 0;
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new Counter() as &$value) {}
"#,
        "by-reference foreach over Iterator/IteratorAggregate objects",
    );
}

/// Verifies that by-reference foreach over a concrete class implementing `IteratorAggregate` is rejected.
/// Uses a `Counters` class that returns a `Counter` iterator via `getIterator()`.
#[test]
fn test_error_by_reference_foreach_rejects_iterator_aggregate_object() {
    expect_error(
        r#"<?php
class Counter implements Iterator {
    private int $i = 0;
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
class Counters implements IteratorAggregate {
    public function getIterator(): Traversable { return new Counter(); }
}
foreach (new Counters() as &$value) {}
"#,
        "by-reference foreach over Iterator/IteratorAggregate objects",
    );
}

/// Verifies that a union-typed local variable rejects an initializer of an incompatible type.
/// Input: `int|string $value = 1.5;` — float is not int or string.
#[test]
fn test_error_union_typed_local_rejects_invalid_initializer() {
    expect_error("<?php int|string $value = 1.5;", "cannot initialize $value");
}

/// Verifies a boxed `mixed` value cannot enter an object parameter without a runtime tag check.
#[test]
fn test_error_mixed_rejected_at_object_parameter_boundary() {
    expect_error(
        "<?php final class Box {} function take(Box $box): void {} function relay(mixed $value): void { take($value); }",
        "Function 'take' parameter $box expects Object(\"Box\"), got Mixed",
    );
}

/// Verifies a boxed `mixed` value cannot leave a function through an array return boundary.
#[test]
fn test_error_mixed_rejected_at_array_return_boundary() {
    expect_error(
        "<?php function relay(mixed $value): array { return $value; }",
        "Function 'relay' return type expects Array(Mixed), got Mixed",
    );
}

/// Verifies that referencing an undefined variable produces an "Undefined variable" error.
#[test]
fn test_error_undefined_variable() {
    expect_error("<?php echo $x;", "Undefined variable: $x");
}

/// Verifies that a plain self-referential assignment is not mistaken for `+=`.
#[test]
fn test_error_plain_self_read_assignment_remains_undefined() {
    expect_error("<?php $x = $x + 1;", "Undefined variable: $x");
}

/// Verifies that reassigning a typed variable to a different type is rejected.
/// Input: `$x = 42; $x = "hello";` — `$x` is int, reassignment to string fails.
#[test]
fn test_error_type_mismatch_reassign() {
    expect_error("<?php $x = 42; $x = \"hello\";", "cannot reassign $x");
}

/// Verifies that arithmetic on a string operand produces an error.
/// Input: `$x = "hi"; echo $x + 1;` — string is not numeric.
#[test]
fn test_error_arithmetic_on_string() {
    expect_error(
        "<?php $x = \"hi\"; echo $x + 1;",
        "Arithmetic operators require numeric operands",
    );
}

/// Verifies a name beginning with `with` does not imply a late-static fluent return.
#[test]
fn test_error_with_prefix_does_not_refine_declared_ancestor_return() {
    expect_error(
        r#"<?php
interface Account {
    public function withdraw(int $amount): Account;
}
interface Savings extends Account {
    public function interestRate(): int;
}
final class SavingsAccount implements Savings {
    public function withdraw(int $amount): Account { return $this; }
    public function interestRate(): int { return 4; }
}
function rate(Savings $account): int {
    return $account->withdraw(10)->interestRate();
}
echo rate(new SavingsAccount());
"#,
        "Undefined method: Account::interestRate",
    );
}

/// Verifies that binding `static` preserves distinct explicit union members.
///
/// `static|Choice` called on `SpecialChoice` becomes `SpecialChoice|Choice`, so a
/// subclass-only method is not safe on the result even though one branch is late-bound.
#[test]
fn test_error_late_static_union_keeps_explicit_ancestor_member() {
    expect_error(
        r#"<?php
class Choice {
    public function choose(bool $same): static|Choice {
        return $same ? $this : new Choice();
    }
}
class SpecialChoice extends Choice {
    public function special(): string { return "special"; }
}
function render(SpecialChoice $choice): string {
    return $choice->choose(false)->special();
}
"#,
        "Undefined method",
    );
}

/// Verifies an interface `static` contract cannot be implemented as the concrete class name.
#[test]
fn test_error_interface_static_return_requires_late_static_implementation() {
    expect_error(
        r#"<?php
interface CreatesLateBound {
    public function create(): static;
}
class ConcreteCreator implements CreatesLateBound {
    public function create(): ConcreteCreator { return $this; }
}
"#,
        "incompatible return type",
    );
}

/// Verifies overriding `static` with the immediate child name is rejected for future subclasses.
#[test]
fn test_error_static_return_override_cannot_become_concrete_child() {
    expect_error(
        r#"<?php
class LateBoundBase {
    public function copy(): static { return $this; }
}
class ConcreteCopy extends LateBoundBase {
    public function copy(): ConcreteCopy { return $this; }
}
"#,
        "incompatible return type",
    );
}

/// Verifies a child interface must preserve its parent's late-static return contract.
#[test]
fn test_error_interface_redeclaration_cannot_replace_static_with_child_name() {
    expect_error(
        r#"<?php
interface LateBoundContract {
    public function copy(): static;
}
interface ConcreteContract extends LateBoundContract {
    public function copy(): ConcreteContract;
}
"#,
        "compatible late-static return type",
    );
}

/// Verifies that negating a non-numeric string produces an error.
/// Input: `$x = "hi"; echo -$x;`
#[test]
fn test_error_negate_string() {
    expect_error(
        "<?php $x = \"hi\"; echo -$x;",
        "Cannot negate a non-numeric value",
    );
}

/// Verifies that comparison operators on strings produce an error.
/// Input: `$x = "a"; echo $x < 1;` — string vs int comparison is invalid.
#[test]
fn test_error_comparison_on_string() {
    expect_error(
        "<?php $x = \"a\"; echo $x < 1;",
        "Comparison operators require numeric operands",
    );
}

/// Verifies that `xor` with no right-hand side produces an "Unexpected token" error.
#[test]
fn test_error_word_logical_missing_rhs() {
    expect_error("<?php echo true xor;", "Unexpected token: Semicolon");
}

/// Verifies that an assignment expression with a non-lvalue target is rejected.
/// Input: `echo 1 = 2;` — 1 is not a valid assignment target.
#[test]
fn test_error_assignment_expression_rejects_non_lvalue() {
    expect_error("<?php echo 1 = 2;", "Invalid assignment target");
}

/// Verifies that a variable assigned inside a short-circuit `&&` is flagged as possibly undefined
/// when referenced after the `&&` expression that did not execute.
/// Input: `echo false && ($x = 1); echo $x;` — `$x` may not be defined.
#[test]
fn test_error_short_circuit_assignment_effect_is_not_definite() {
    expect_error(
        "<?php echo false && ($x = 1); echo $x;",
        "Undefined variable: $x",
    );
}

/// Verifies that the short ternary (`?:`) with no default expression produces an error.
#[test]
fn test_error_short_ternary_missing_default() {
    expect_error("<?php echo $x ?:;", "Unexpected token: Semicolon");
}

/// Verifies that `break` outside any loop or switch produces an error.
#[test]
fn test_error_break_outside_loop_or_switch() {
    expect_error("<?php break;", "Cannot 'break' 1 levels");
}

/// Verifies that `break N` with N exceeding the available nesting levels produces an error.
#[test]
fn test_error_break_too_many_levels() {
    expect_error("<?php while (1) { break 2; }", "Cannot 'break' 2 levels");
}

/// Verifies that `continue N` with N exceeding available loop nesting produces an error.
#[test]
fn test_error_continue_too_many_levels() {
    expect_error(
        "<?php while (1) { continue 2; }",
        "Cannot 'continue' 2 levels",
    );
}

/// Verifies that `break` inside a `finally` block cannot jump out of the finally.
#[test]
fn test_error_break_cannot_jump_out_of_finally() {
    expect_error(
        "<?php while (1) { try { echo 1; } finally { break; } }",
        "Cannot jump out of a finally block",
    );
}

/// Verifies that `continue` inside a `finally` block cannot jump out of the finally.
#[test]
fn test_error_continue_cannot_jump_out_of_finally() {
    expect_error(
        "<?php while (1) { try { echo 1; } finally { continue; } }",
        "Cannot jump out of a finally block",
    );
}

/// Verifies that a multi-level `break N` inside a `finally` block cannot jump out of the finally.
#[test]
fn test_error_multilevel_break_cannot_jump_out_of_finally() {
    expect_error(
        "<?php while (1) { try { echo 1; } finally { while (1) { break 2; } } }",
        "Cannot jump out of a finally block",
    );
}

/// Verifies that calling an undefined function produces an error.
#[test]
fn test_error_undefined_function() {
    expect_error("<?php nope();", "Undefined function: nope");
}

/// Verifies that passing too many arguments to a user-defined function is rejected.
#[test]
fn test_error_wrong_arg_count() {
    expect_error(
        "<?php function f($a) { return $a; } f(1, 2);",
        "expects 1 arguments, got 2",
    );
}

/// Verifies that increment/decrement on a string is rejected.
#[test]
fn test_error_increment_string() {
    expect_error("<?php $x = \"hi\"; $x++;", "Cannot increment/decrement");
}

/// Verifies the kind predicates `is_array`/`is_object`/`is_scalar` reject a wrong argument
/// count, matching the other single-argument type predicates.
#[test]
fn test_error_is_kind_predicates_arity() {
    expect_error(
        "<?php is_array([1], [2]);",
        "is_array() takes exactly 1 argument",
    );
    expect_error("<?php is_object();", "is_object() takes exactly 1 argument");
    expect_error(
        "<?php is_scalar(1, 2, 3);",
        "is_scalar() takes exactly 1 argument",
    );
}

// --- Error positions ---

/// Verifies that the null coalesce operator widens the inferred return type to float
/// when one branch is int and the other is a float literal.
/// Input: `function fallback_pi($x) { return $x ?? 3.14159; }`
#[test]
fn test_null_coalesce_widens_function_return_type_in_checker() {
    let tokens = tokenize("<?php function fallback_pi($x) { return $x ?? 3.14159; }")
        .expect("tokenize failed");
    let ast = parse(&tokens).expect("parse failed");
    let ast = elephc::optimize::fold_constants(ast);
    let check_result = types::check(&ast).expect("type check failed");

    let sig = check_result
        .functions
        .get("fallback_pi")
        .expect("missing function signature for fallback_pi");
    assert_eq!(sig.return_type, PhpType::Float);

    // Verifies that `array` return hints preserve the element type through property storage
    // and method return inference, using a `Wad` class with `Entry` objects.








    // Verifies that `array` parameter and return hints preserve string element types
    // through a chain of `paint`, `pickSecond`, and `loadNames`.





}

/// Verifies generic array return hint keeps specific method and property types.
#[test]
fn test_generic_array_return_hint_keeps_specific_method_and_property_types() {
    let result = check_source_full(
        r#"<?php
class Entry {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }
}

class Wad {
    public $entries;

    public function __construct() {
        $this->entries = $this->loadEntries();
    }

    public function loadEntries(): array {
        return [new Entry("PLAYPAL"), new Entry("COLORMAP")];
    }

    public function secondName(): string {
        $i = 1;
        return $this->entries[$i]->name;
    }
}
"#,
    )
    .expect("expected source to type-check");

    let wad = result.classes.get("Wad").expect("missing Wad class");
    let entries_ty = wad
        .properties
        .iter()
        .find(|(name, _)| name == "entries")
        .map(|(_, ty)| ty.clone())
        .expect("missing entries property");
    assert_eq!(
        entries_ty,
        PhpType::Array(Box::new(PhpType::Object("Entry".to_string())))
    );

    let load_entries = wad
        .methods
        .get(&elephc::names::php_symbol_key("loadEntries"))
        .expect("missing loadEntries");
    assert_eq!(
        load_entries.return_type,
        PhpType::Array(Box::new(PhpType::Object("Entry".to_string())))
    );
}

/// Verifies generic array param and return hints keep specific string array types.
#[test]
fn test_generic_array_param_and_return_hints_keep_specific_string_array_types() {
    let result = check_source_full(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function pickSecond(array $names): string {
    return paint($names[1]);
}

function loadNames(): array {
    return ["foo", "bar"];
}

echo pickSecond(loadNames());
"#,
    )
    .expect("expected source to type-check");

    let pick_second = result
        .functions
        .get("pickSecond")
        .expect("missing pickSecond signature");
    assert_eq!(
        pick_second.params[0].1,
        PhpType::Array(Box::new(PhpType::Str))
    );

    let load_names = result
        .functions
        .get("loadNames")
        .expect("missing loadNames signature");
    assert_eq!(load_names.return_type, PhpType::Array(Box::new(PhpType::Str)));
}

// --- Include/Require errors ---

/// Verifies that passing more arguments than a function with optional parameters accepts is rejected.
/// Input: `function f($a, $b = 1) { return $a + $b; } f(1, 2, 3);`
#[test]
fn test_error_too_many_args_with_defaults() {
    expect_error(
        "<?php function f($a, $b = 1) { return $a + $b; } f(1, 2, 3);",
        "expects 1 to 2 arguments, got 3",
    );
}

/// Verifies that passing fewer arguments than a function with optional parameters requires is rejected.
/// Input: `function f($a, $b = 1) { return $a + $b; } f();`
#[test]
fn test_error_too_few_args_with_defaults() {
    expect_error(
        "<?php function f($a, $b = 1) { return $a + $b; } f();",
        "expects 1 to 2 arguments, got 0",
    );
}

/// Verifies that a promoted constructor parameter with a type mismatch is rejected.
/// Input: `class Box { public function __construct(public int $value) {} } new Box("bad");`
#[test]
fn test_error_promoted_property_type_mismatch() {
    expect_error(
        r#"<?php
class Box {
    public function __construct(public int $value) {}
}
$box = new Box("bad");
"#,
        "Constructor 'Box::__construct' parameter $value expects Int, got Str",
    );
}

/// Verifies that an unrelated object default is rejected after class relationships are known.
#[test]
fn test_error_promoted_property_rejects_incompatible_object_default() {
    expect_error(
        r#"<?php
class Expected {}
class Unrelated {}
class Box {
    public function __construct(public Expected $value = new Unrelated()) {}
}
"#,
        "Method parameter $value expects Object(\"Expected\"), got Object(\"Unrelated\")",
    );
}

/// Verifies an enum-typed parameter default rejects a missing enum case semantically.
#[test]
fn test_error_enum_case_parameter_default_rejects_missing_case() {
    expect_error(
        r#"<?php
enum A {
    case One;
}
function unused_enum_default(A $a = A::Nope): void {}
"#,
        "Undefined enum case: A::Nope",
    );
}

/// Verifies a scalar class constant cannot default an object-typed parameter.
#[test]
fn test_error_object_parameter_default_rejects_scalar_class_constant() {
    expect_error(
        r#"<?php
class Foo {
    public const BAR = 1;
}
function unused_class_constant_default(Foo $value = Foo::BAR): void {}
"#,
        "Function 'unused_class_constant_default' parameter $value expects Object(\"Foo\"), got Int",
    );
}

/// Verifies plain property enum case defaults remain outside the supported EIR surface.
#[test]
fn test_error_plain_property_enum_case_default_remains_unsupported() {
    expect_error(
        r#"<?php
enum Level {
    case Low;
}
class Config {
    public Level $level = Level::Low;
}
"#,
        "Property Config::$level default expects Object(\"Level\"), got Str",
    );
}

/// Verifies that assigning an incompatible value to a static property is rejected.
/// Input: `class Box { public static int $count = 1; } Box::$count = "x";`
#[test]
fn test_error_static_property_type_mismatch() {
    expect_error(
        "<?php class Box { public static int $count = 1; } Box::$count = \"x\";",
        "Static property Box::$count expects",
    );
}

/// Verifies that a child class static property redeclared with an incompatible type is rejected.
/// Input: `class Base { public static int $count = 1; } class Child extends Base { public static string $count = "x"; }`
#[test]
fn test_error_static_property_redeclaration_type_mismatch() {
    expect_error(
        "<?php class Base { public static int $count = 1; } class Child extends Base { public static string $count = \"x\"; }",
        "Type of Child::$count must be int, not string (as in class Base)",
    );
}

/// Verifies that `date()` with too many arguments is rejected.
#[test]
fn test_error_date_too_many_args() {
    expect_error(r#"<?php date("Y", 0, 0);"#, "date() takes 1 or 2 arguments");
}

/// Verifies that `json_encode()` flags argument must be int (not string).
#[test]
fn test_error_json_encode_flag_must_be_int() {
    expect_error(
        r#"<?php json_encode("a", "b");"#,
        "json_encode() flags and depth must be integers",
    );
}

/// Verifies that `json_encode()` depth argument must be int (not string).
#[test]
fn test_error_json_encode_depth_must_be_int() {
    expect_error(
        r#"<?php json_encode("a", 0, "deep");"#,
        "json_encode() flags and depth must be integers",
    );
}

/// Verifies that `json_encode()` with too many arguments is rejected.
#[test]
fn test_error_json_encode_too_many_args() {
    expect_error(
        "<?php json_encode(1, 2, 3, 4);",
        "json_encode() takes 1 to 3 arguments",
    );
}

/// Verifies that `json_decode()` with too many arguments is rejected.
#[test]
fn test_error_json_decode_too_many_args() {
    expect_error(
        r#"<?php json_decode("1", true, 1, 0, 99);"#,
        "json_decode() takes 1 to 4 arguments",
    );
}

/// Verifies that `json_decode()` requires a string-compatible first argument (array is rejected).
#[test]
fn test_error_json_decode_json_arg_must_be_string_compatible() {
    expect_error(
        r#"<?php json_decode([]);"#,
        "json_decode() json argument must be string-compatible",
    );
}

/// Verifies that `json_decode()` associative argument must be bool-compatible or null (array is rejected).
#[test]
fn test_error_json_decode_associative_must_be_bool_compatible() {
    expect_error(
        r#"<?php json_decode("{}", []);"#,
        "json_decode() associative argument must be bool-compatible or null",
    );
}

/// Verifies that `json_decode()` depth argument must be int (not string).
#[test]
fn test_error_json_decode_depth_must_be_int() {
    expect_error(
        r#"<?php json_decode("{}", false, "deep");"#,
        "json_decode() depth and flags must be integers",
    );
}

/// Verifies that `json_decode()` flags argument must be int (not string).
#[test]
fn test_error_json_decode_flags_must_be_int() {
    expect_error(
        r#"<?php json_decode("{}", false, 512, "flags");"#,
        "json_decode() depth and flags must be integers",
    );
}

/// Verifies that `json_validate()` with too many arguments is rejected.
#[test]
fn test_error_json_validate_too_many_args() {
    expect_error(
        r#"<?php json_validate("1", 1, 0, 99);"#,
        "json_validate() takes 1 to 3 arguments",
    );
}

/// Verifies that `json_validate()` requires a string-compatible first argument (array is rejected).
#[test]
fn test_error_json_validate_json_arg_must_be_string_compatible() {
    expect_error(
        r#"<?php json_validate([]);"#,
        "json_validate() json argument must be string-compatible",
    );
}

/// Verifies that `json_validate()` depth argument must be int (not string).
#[test]
fn test_error_json_validate_flag_must_be_int() {
    expect_error(
        r#"<?php json_validate("1", "deep");"#,
        "json_validate() depth and flags must be integers",
    );
}

/// Verifies that `json_validate()` rejects `JSON_THROW_ON_ERROR` in flags.
#[test]
fn test_error_json_validate_rejects_throw_on_error_flag() {
    expect_error(
        r#"<?php json_validate("1", 512, JSON_THROW_ON_ERROR);"#,
        "json_validate() flags must be 0 or JSON_INVALID_UTF8_IGNORE",
    );
}

/// Verifies that `json_validate()` rejects combined flags mixing invalid values.
#[test]
fn test_error_json_validate_rejects_combined_invalid_flags() {
    expect_error(
        r#"<?php json_validate("1", 512, JSON_INVALID_UTF8_IGNORE | JSON_THROW_ON_ERROR);"#,
        "json_validate() flags must be 0 or JSON_INVALID_UTF8_IGNORE",
    );
}

/// Verifies that `sin()` with more than 1 argument is rejected.
#[test]
fn test_error_sin_too_many_args() {
    expect_error("<?php sin(1, 2);", "sin() takes exactly 1 argument");
}

/// Verifies that `log()` with more than 2 arguments is rejected.
#[test]
fn test_error_log_too_many_args() {
    expect_error("<?php log(1, 2, 3);", "log() takes 1 or 2 arguments");
}

/// Verifies that a closure `use()` clause referencing an undefined variable is rejected.
#[test]
fn test_error_closure_use_undefined_variable() {
    expect_error(
        r#"<?php
$fn = function() use ($undefined) { echo $undefined; };
"#,
        "Undefined variable in use(): $undefined",
    );
}

// --- Pointer error tests ---

/// Verifies that loose pointer comparison (`==` or `!=`) is rejected; only `===`/`!==` are allowed.
/// Input: `$p = ptr($x); $q = ptr($x); echo $p == $q;`
#[test]
fn test_error_pointer_loose_comparison_is_rejected() {
    expect_error(
        "<?php $x = 1; $p = ptr($x); $q = ptr($x); echo $p == $q;",
        "Loose pointer comparison is not supported; use === or !==",
    );
}

// --- FFI error tests ---

/// Verifies that using `$this` inside a static closure via a short ternary expression is rejected.
/// Input: `class C { public int $count = 5; public function bad() { $f = static fn($x) => $x ?: $this->count; } }`
#[test]
fn test_error_static_closure_uses_this_through_short_ternary() {
    expect_error(
        "<?php class C { public int $count = 5; public function bad() { $f = static fn($x) => $x ?: $this->count; return $f; } }",
        "Cannot use $this inside a static closure",
    );
}

/// Verifies that combining the nullable shorthand `?T` with a pipe union is rejected, and
/// that the diagnostic points the user at the now-supported `T|null` spelling.
#[test]
fn test_error_nullable_shorthand_with_union() {
    expect_error(
        "<?php function f(): ?int|string { return 1; }",
        "Nullable shorthand cannot be combined directly with union types; write T|null",
    );
}

/// Verifies that a union type with a trailing pipe and no following member is rejected with
/// the type-expression diagnostic, confirming `null`/`false`/`true` did not loosen the
/// requirement that every pipe be followed by a real type.
#[test]
fn test_error_union_trailing_pipe() {
    expect_error(
        "<?php function f(): int| { return 1; }",
        "Expected type expression",
    );
}

/// Verifies that the relative class type `self` is rejected when used as a type outside of any
/// class body (a free function), where it has no enclosing class to resolve to.
#[test]
fn test_error_self_type_outside_class() {
    expect_error(
        "<?php function f(): self { return 1; }",
        "Cannot use 'self' as a type outside of a class",
    );
}

/// Verifies that `static` is likewise rejected as a free-function parameter type.
#[test]
fn test_error_static_type_outside_class() {
    expect_error(
        "<?php function f(static $x): int { return 1; }",
        "Cannot use 'static' as a type outside of a class",
    );
}

/// Verifies that variable variables (`$$name`) are rejected with an explanatory message, since
/// elephc allocates locals to fixed compile-time slots with no runtime variable-name table.
#[test]
fn test_error_variable_variables_unsupported() {
    expect_error(
        "<?php $x = \"y\"; $$x = 1;",
        "Variable variables (`$$name`) are not supported",
    );
}

/// Verifies that the nullable shorthand cannot be combined with an intersection type (`?A&B`),
/// which is a syntax error in PHP. Previously this silently parsed and dropped a member.
#[test]
fn test_error_nullable_intersection_type_rejected() {
    assert!(
        check_source("<?php interface A {} interface B {} function f(?A&B $x): int { return 1; }")
            .is_err(),
        "?A&B should be rejected, not silently accepted",
    );
}

/// `Exception::__construct` third parameter must be `?Throwable`, matching PHP.
#[test]
fn test_error_exception_previous_rejects_non_throwable() {
    expect_error(
        "<?php throw new Exception('x', 0, previous: 123);",
        "previous",
    );
}

/// Regression for issue #587: a `match` merging two indexed arrays with different
/// element types (`[1, 2]` vs `["a", "b"]`) must type as `array<mixed>`, so passing
/// the result to a by-ref `array` parameter type-checks instead of failing with
/// "expects Array(Mixed), got Mixed". Checker-only: heterogeneous merge runtime
/// lowering is tracked by issue #549 / PR #583.
#[test]
fn test_heterogeneous_match_array_merge_accepts_by_ref_array_param() {
    expect_no_error(
        "<?php $r = match($argc) { 1 => [1, 2], default => [\"a\", \"b\"] }; \
         function add(array &$a): void { $a[] = 5; } add($r);",
    );
}

/// Regression for issue #587: a heterogeneous `match` array merge must satisfy the
/// `array` argument of `array_sum()` and `in_array()`, which previously rejected the
/// `mixed`-typed result.
#[test]
fn test_heterogeneous_match_array_merge_accepts_array_builtins() {
    expect_no_error(
        "<?php $r = match($argc) { 1 => [1, 2], default => [\"a\", \"b\"] }; \
         echo array_sum($r); echo in_array(2, $r);",
    );
}

/// Regression for issue #587: spreading a heterogeneous `match` array merge
/// (`[...$r]`) must type-check. This also clears the misleading follow-on
/// "Undefined variable: $s" that appeared because the spread's failure left the
/// assignment target untyped.
#[test]
fn test_heterogeneous_match_array_merge_accepts_spread() {
    expect_no_error(
        "<?php $r = match($argc) { 1 => [1, 2], default => [\"a\", \"b\"] }; \
         $s = [...$r]; echo count($s);",
    );
}

/// Regression for issue #587: the same elementwise widening must apply to a
/// ternary merge, not just `match`, since both share the merge join.
#[test]
fn test_heterogeneous_ternary_array_merge_accepts_array_use() {
    expect_no_error("<?php $r = $argc > 1 ? [1, 2] : [\"a\", \"b\"]; echo array_sum($r);");
}

/// Regression for issue #587: an associative merge whose value types differ
/// (`["k" => 1]` vs `["k" => "v"]`) must widen elementwise to `array<string, mixed>`
/// and stay an array, not collapse to bare `mixed`.
#[test]
fn test_heterogeneous_match_assoc_merge_stays_array() {
    expect_no_error(
        "<?php $r = match($argc) { 1 => [\"k\" => 1], default => [\"k\" => \"v\"] }; \
         echo array_sum($r);",
    );
}

/// Guards issue #587's fix against over-widening: a merge of non-array scalar arms
/// (`1` vs `"a"`) must still type as `mixed`, so an array-only use like `array_sum()`
/// stays rejected.
#[test]
fn test_scalar_match_merge_stays_mixed_and_rejects_array_use() {
    expect_error(
        "<?php $r = match($argc) { 1 => 1, default => \"a\" }; echo array_sum($r);",
        "array_sum() argument must be array",
    );
}
