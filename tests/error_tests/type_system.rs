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

/// Verifies that reassigning an inferred local to an incompatible type is diagnosed.
/// Input: `$x = 42; $x = "hello";` — a depth-0, statement-form, unaliased retype, so the
/// permissive default warns and re-binds while `--strict-locals` keeps the hard error.
#[test]
fn test_error_type_mismatch_reassign() {
    expect_warning(
        "<?php $x = 42; $x = \"hello\"; echo $x;",
        "changes type from int to string",
    );
    expect_error_strict("<?php $x = 42; $x = \"hello\";", "cannot reassign $x");
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

/// Verifies the two `string` storage shapes that cannot take the boxed `mixed` contract
/// PHP's string increment needs: a by-reference parameter aliases a caller slot whose
/// declared `string` type must not change, and a `static` local's initializer writes its
/// symbol with the declared `string` representation. Both must be source-level errors.
#[test]
fn test_error_increment_string_in_unboxable_storage() {
    expect_error(
        "<?php function f(string &$r): void { $r++; } $v = \"az\"; f($v);",
        "it is a by-reference parameter",
    );
    expect_error(
        "<?php function g(): string { static $s = \"aa\"; $s++; return $s; } g();",
        "it is a static local",
    );
}

/// Verifies that increment/decrement is still rejected on the local types PHP has no
/// increment rule for, now that `string` locals have one (`"az"++` is `"ba"`).
#[test]
fn test_error_increment_unsupported_type() {
    expect_error(
        "<?php $x = [1, 2]; $x++;",
        "Cannot increment/decrement $x of type",
    );
    expect_error(
        "<?php $x = [1, 2]; --$x;",
        "Cannot increment/decrement $x of type",
    );
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

/// Verifies `get_object_vars()` rejects scalar inputs instead of treating their
/// storage representation as an object property table.
#[test]
fn test_error_get_object_vars_requires_object() {
    expect_error(
        "<?php get_object_vars(42);",
        "get_object_vars() argument must be an object",
    );
}

// --- Error positions ---

/// Verifies that `??` merges two DIFFERENT arm types to `mixed` rather than letting one arm
/// absorb the other.
///
/// This test previously asserted `Float`, on the theory that `??` widens like an arithmetic
/// operator. It does not: `??` is `isset($a) ? $a : $b` and performs no coercion at all, so
/// `fallback_pi("hi")` must return the string `"hi"`. Under the old `Float` inference the
/// value branch was lowered as a float coercion and the caller silently received `float(0)`
/// for a string argument, `float(2)` for `2` and `float(1)` for `true` (reference PHP 8.5.6:
/// `string(2) "hi"`, `int(2)`, `bool(true)`). `Mixed` is the only merge that keeps every arm
/// intact; `null_coalesce_merge_type` in `src/types/checker/inference/syntactic.rs` computes
/// it, and it agrees with the IR-level `wider_type_for_merge` that already emitted a Mixed
/// merge slot for this shape.
/// Input: `function fallback_pi($x) { return $x ?? 3.14159; }`
#[test]
fn test_null_coalesce_merges_mismatched_arms_to_mixed_in_checker() {
    let tokens = tokenize("<?php function fallback_pi($x) { return $x ?? 3.14159; }")
        .expect("tokenize failed");
    let ast = parse(&tokens).expect("parse failed");
    let ast = elephc::optimize::fold_constants(ast);
    let check_result = types::check(&ast).expect("type check failed");

    let sig = check_result
        .functions
        .get("fallback_pi")
        .expect("missing function signature for fallback_pi");
    assert_eq!(sig.return_type, PhpType::Mixed);

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
/// "expects Array(Mixed), got Mixed".
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

/// Regression for issue #587: `??` must join array element types just like
/// `match`/ternary after removing null from the value side, instead of retaining
/// the left branch's `array<int>` type.
#[test]
fn test_heterogeneous_null_coalesce_array_merge_widens_element_type() {
    let tokens = tokenize(
        "<?php function maybe(int $n) { return $n === 1 ? [1, 2] : null; } \
         $r = maybe($argc) ?? [\"a\", \"b\"];",
    )
    .expect("tokenize failed");
    let ast = parse(&tokens).expect("parse failed");
    let ast = elephc::optimize::fold_constants(ast);
    let result = types::check(&ast).expect("expected source to type-check");

    assert_eq!(
        result.global_env.get("r"),
        Some(&PhpType::Array(Box::new(PhpType::Mixed)))
    );
}

/// An empty branch contributes no element values, so `[]` merged with
/// `array<int>` retains `array<int>` instead of widening unnecessarily.
#[test]
fn test_empty_match_array_branch_keeps_populated_element_type() {
    let tokens = tokenize(
        "<?php $r = match($argc) { 1 => [], default => [1, 2] };",
    )
    .expect("tokenize failed");
    let ast = parse(&tokens).expect("parse failed");
    let ast = elephc::optimize::fold_constants(ast);
    let result = types::check(&ast).expect("expected source to type-check");

    assert_eq!(
        result.global_env.get("r"),
        Some(&PhpType::Array(Box::new(PhpType::Int)))
    );
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

/// Verifies the `Undefined variable` diagnostic still fires for an ordinary read, so the null-probe
/// tolerance is scoped to `isset`/`empty`/`unset`/`??` and nothing else.
#[test]
fn test_undefined_variable_read_is_still_rejected() {
    expect_error("<?php echo $neverDefined;", "Undefined variable: $neverDefined");
}

/// Verifies only the probe's chain SPINE is tolerated: PHP warns about `$b` in `isset($a[$b])`
/// but not about `$a`, so the index subexpression keeps the diagnostic.
#[test]
fn test_null_probe_index_subexpression_still_requires_a_defined_variable() {
    expect_error("<?php var_dump(isset($a[$b]));", "Undefined variable: $b");
}

/// Verifies `isset()`, `empty()`, `unset()` and `??` accept a never-declared variable, which is
/// exactly what those constructs exist for. Runtime answers are pinned by codegen tests.
#[test]
fn test_null_probes_accept_a_never_declared_variable() {
    expect_no_error("<?php var_dump(isset($neverA));");
    expect_no_error("<?php var_dump(empty($neverB));");
    expect_no_error(r#"<?php var_dump($neverC ?? "d");"#);
    expect_no_error("<?php unset($neverD); echo 'ok';");
    expect_no_error("<?php var_dump(isset($neverE['k']));");
}

/// Verifies a probed name that is ALSO assigned in the same scope keeps the diagnostic.
///
/// The tolerance is only sound while the variable stays `null` for the whole scope: main's local
/// types come from the final global environment, so an assigned name gets that assigned type on a
/// slot the probe would read before any store. Accepting it would miscompile, so the deferred
/// check restores the original error.
#[test]
fn test_null_probe_on_a_later_assigned_variable_is_still_rejected() {
    expect_error(
        "<?php if (!isset($cfg)) { $cfg = 3; } var_dump($cfg);",
        "Undefined variable: $cfg",
    );
}

/// Verifies a lossy float constant at an `int` parameter is rejected instead of silently
/// truncated. PHP passes `5` after emitting `Deprecated: Implicit conversion from float 5.5 to
/// int loses precision`; elephc has no runtime deprecation channel, so it refuses the program
/// rather than dropping the notice.
#[test]
fn test_error_int_parameter_rejects_lossy_float_constant() {
    expect_error(
        "<?php function ti(int $i) { return $i; } echo ti(5.5);",
        "PHP emits `Deprecated: Implicit conversion from float 5.5 to int loses precision`",
    );
}

/// Verifies a non-numeric string constant at an `int` parameter is rejected with PHP's
/// failure mode named. PHP throws `TypeError` when the call runs.
#[test]
fn test_error_int_parameter_rejects_non_numeric_string_constant() {
    expect_error(
        "<?php function ti(int $i) { return $i; } echo ti(\"abc\");",
        "PHP throws `TypeError` for the non-numeric string \"abc\" at an `int` parameter",
    );
}

/// Verifies a leading-numeric string constant is rejected too: PHP's `(int)` cast would give
/// `42`, but parameter binding throws `TypeError` because the string is not fully numeric.
#[test]
fn test_error_int_parameter_rejects_leading_numeric_string_constant() {
    expect_error(
        "<?php function ti(int $i) { return $i; } echo ti(\"42abc\");",
        "PHP throws `TypeError` for the non-numeric string \"42abc\" at an `int` parameter",
    );
}

/// Verifies a runtime float at an `int` parameter is rejected, because deciding between PHP's
/// silent conversion, its deprecation notice and its `TypeError` needs a runtime check elephc
/// cannot perform at a parameter boundary.
#[test]
fn test_error_int_parameter_rejects_runtime_float() {
    expect_error(
        "<?php function ti(int $i) { return $i; } $f = 5.5 * $argc; echo ti($f);",
        "add an explicit cast at the call site",
    );
}

/// Verifies a non-numeric string constant at a `float` parameter is rejected the same way.
#[test]
fn test_error_float_parameter_rejects_non_numeric_string_constant() {
    expect_error(
        "<?php function tf(float $f) { return $f; } echo tf(\"abc\");",
        "PHP throws `TypeError` for the non-numeric string \"abc\" at a `float` parameter",
    );
}

/// Verifies an out-of-range float constant at an `int` parameter reports PHP's `TypeError`
/// rather than wrapping around.
#[test]
fn test_error_int_parameter_rejects_out_of_range_float_constant() {
    expect_error(
        "<?php function ti(int $i) { return $i; } echo ti(1e20);",
        "PHP throws `TypeError` for the float",
    );
}

/// Verifies a pass-by-reference parameter stays on the strict path. PHP converts the caller's
/// variable in place and writes the converted value back; elephc's binding would pass a
/// converted temporary and silently drop the callee's writes, so the call is rejected instead.
#[test]
fn test_error_by_ref_parameter_is_not_coerced() {
    expect_error(
        "<?php function f(string &$s) { $s = $s . \"!\"; } $n = 42; f($n);",
        "Function 'f' parameter $s expects Str, got Int",
    );
}

/// Verifies `declare(strict_types=1)` rejects the `bool`→`int` binding PHP's coercive mode
/// performs silently, and that the diagnostic names the `TypeError` PHP would throw.
///
/// This is the audit repro: before the directive was honoured, elephc compiled `ti(true)` to
/// `int(1)` while PHP 8.4.20 fatals with
/// `TypeError: ti(): Argument #1 ($i) must be of type int, true given`.
#[test]
fn test_error_strict_types_rejects_bool_into_int_parameter() {
    expect_error(
        "<?php declare(strict_types=1); function ti(int $i) { return $i; } echo ti(true);",
        "must be of type int, bool given",
    );
}

/// Verifies the strict diagnostic identifies the directive as the reason and suggests the cast
/// that makes the call legal, rather than reading as a plain type mismatch.
#[test]
fn test_error_strict_types_diagnostic_names_the_directive() {
    expect_error(
        "<?php declare(strict_types=1); function ti(int $i) { return $i; } echo ti(true);",
        "`declare(strict_types=1)` is active in this file",
    );
}

/// Verifies every scalar that binds to a `string` parameter in coercive mode is rejected under
/// the directive: `int`, `float` and `bool` sources all throw `TypeError` in PHP 8.4.20.
#[test]
fn test_error_strict_types_rejects_scalars_into_string_parameter() {
    for (argument, php_type) in [("42", "int"), ("4.5", "float"), ("true", "bool")] {
        expect_error(
            &format!(
                "<?php declare(strict_types=1); function ts(string $s) {{ return $s; }} echo ts({});",
                argument
            ),
            &format!("must be of type string, {} given", php_type),
        );
    }
}

/// Verifies every scalar that binds to a `bool` parameter in coercive mode is rejected under the
/// directive.
#[test]
fn test_error_strict_types_rejects_scalars_into_bool_parameter() {
    for (argument, php_type) in [("1", "int"), ("1.5", "float"), ("\"a\"", "string")] {
        expect_error(
            &format!(
                "<?php declare(strict_types=1); function tb(bool $b) {{ return $b; }} echo tb({});",
                argument
            ),
            &format!("must be of type bool, {} given", php_type),
        );
    }
}

/// Verifies the constant `float`/numeric-string arguments coercive mode folds into an `int`
/// parameter are rejected under the directive instead.
#[test]
fn test_error_strict_types_rejects_constants_into_int_parameter() {
    expect_error(
        "<?php declare(strict_types=1); function ti(int $i) { return $i; } echo ti(5.0);",
        "must be of type int, float given",
    );
    expect_error(
        "<?php declare(strict_types=1); function ti(int $i) { return $i; } echo ti(\"42\");",
        "must be of type int, string given",
    );
}

/// Verifies a numeric string is rejected at a `float` parameter under the directive, even though
/// coercive mode binds it as a constant.
#[test]
fn test_error_strict_types_rejects_numeric_string_into_float_parameter() {
    expect_error(
        "<?php declare(strict_types=1); function tf(float $f) { return $f; } echo tf(\"1.5\");",
        "must be of type float, string given",
    );
}

/// Verifies the directive reaches method calls, not just plain functions.
#[test]
fn test_error_strict_types_rejects_method_argument() {
    expect_error(
        "<?php declare(strict_types=1); class C { public function m(int $i) { return $i; } } $c = new C(); echo $c->m(true);",
        "Method C::m parameter $i expects Int, got Bool",
    );
}

/// Verifies the directive reaches a closure invoked through a variable, which is validated on a
/// different checker path from a named function call.
#[test]
fn test_error_strict_types_rejects_closure_argument() {
    expect_error(
        "<?php declare(strict_types=1); $f = function (int $i) { return $i; }; echo $f(true);",
        "must be of type int, bool given",
    );
}

/// Verifies the directive reaches a declared variadic element type, which PHP checks exactly
/// like a regular declared parameter.
#[test]
fn test_error_strict_types_rejects_variadic_element() {
    expect_error(
        "<?php declare(strict_types=1); function f(int ...$xs) { return count($xs); } echo f(true);",
        "variadic parameter $xs expects Int, got Bool",
    );
}

/// Verifies `call_user_func` stays on the strict path. Unlike `array_map`, it forwards the
/// caller's frame, so PHP 8.4.20 throws `TypeError` for `call_user_func('g', true)` in a
/// strict file.
#[test]
fn test_error_strict_types_reaches_call_user_func() {
    expect_error(
        "<?php declare(strict_types=1); function g(int $i) { return $i; } echo call_user_func('g', true);",
        "must be of type int, bool given",
    );
}

/// Verifies a coercive file is unaffected: the same `bool`→`int` call still binds, so the
/// directive genuinely narrows only the files that declare it.
#[test]
fn test_strict_types_absent_keeps_coercive_binding() {
    expect_no_error("<?php function ti(int $i) { return $i; } echo ti(true);");
}

/// `unset` at top level kills the binding: a later incompatible assignment binds fresh.
#[test]
fn test_unset_then_retype_is_accepted() {
    expect_no_error("<?php $a = 1; unset($a); $a = \"ciao\"; echo $a;");
}

/// `unset` at top level kills the binding: a later read is an undefined variable.
#[test]
fn test_read_after_unset_is_undefined() {
    expect_error("<?php $a = 1; unset($a); echo $a;", "Undefined variable");
}

/// Multi-arg unset kills every plain-variable binding.
#[test]
fn test_multi_arg_unset_kills_all_bindings() {
    expect_no_error("<?php $a = 1; $b = 2; unset($a, $b); $a = \"x\"; $b = \"y\"; echo $a . $b;");
}

/// A conditional unset does NOT kill the binding (sound: the branch may not run).
/// (A later incompatible reassignment of the still-bound name is Task 3's business:
/// it becomes a depth-0 retype warning — do not assert an error for it here.)
#[test]
fn test_conditional_unset_keeps_binding() {
    expect_no_error("<?php $a = 1; if ($argc > 1) { unset($a); } echo $a;");
}

/// A binding created inside a branch is not killable at depth 0 (may be uninitialized).
#[test]
fn test_branch_created_binding_not_killable() {
    expect_error("<?php if ($argc > 1) { $a = 1; } unset($a); $a = \"x\"; echo $a;", "cannot reassign");
}

/// Reference-aliased locals are never killable.
#[test]
fn test_ref_aliased_local_not_killable() {
    expect_error("<?php $a = 1; $r =& $a; unset($a); $a = \"x\";", "cannot reassign");
}

/// Static locals are never killable.
#[test]
fn test_static_local_not_killable() {
    expect_error("<?php function f() { static $a = 1; unset($a); $a = \"x\"; } f();", "cannot reassign");
}

/// Global-bound locals are never killable.
#[test]
fn test_global_local_not_killable() {
    expect_error("<?php $g = 1; function f() { global $g; unset($g); $g = \"x\"; } f();", "cannot reassign");
}

/// A name ANY function-like body in the program declares `global` is never killable, wherever the
/// `unset` is written.
///
/// `Checker::active_globals` is per-body: at top level it is empty, so the top-level `unset($a)`
/// below was accepted as a kill even though `w()`'s `global $a` binds the very same program-global
/// storage. Measured on HEAD, the program was rejected with `Undefined variable: $a` (it compiled
/// before this feature existed); PHP 8.4 prints `5`. The lowering already refuses to abandon the
/// slot — it consults the same program-wide `global` set — so the two halves now share one
/// collector and cannot drift.
#[test]
fn test_program_wide_global_name_not_killable() {
    expect_no_error("<?php function w() { global $a; $a = 5; } $a = 1; unset($a); w(); echo $a;");
    let result =
        check_source_full("<?php function w() { global $a; $a = 5; } $a = 1; unset($a); w(); echo $a;")
            .expect("a program-wide global name must still type-check after an unset");
    assert!(
        result.local_bind_kill_sites.is_empty(),
        "a name some body declares `global` must record no kill site: {:?}",
        result.local_bind_kill_sites
    );
}

/// Control for the test above: the SAME shape with no `global` declaration anywhere still kills,
/// so the veto is about the `global` and not about a checker that stopped killing at top level.
#[test]
fn test_unset_of_a_non_global_name_still_kills() {
    let result = check_source_full("<?php function w() { $a = 5; } $a = 1; unset($a); w(); echo \"ok\";")
        .expect("an ordinary local kill must type-check");
    assert_eq!(
        result.local_bind_kill_sites.len(),
        1,
        "a name no body declares `global` must still record its kill site: {:?}",
        result.local_bind_kill_sites
    );
}

/// The RETYPE is deliberately NOT vetoed by the same rule, because it does not need to be: it
/// leaves the name BOUND, and lowering already refuses to abandon a top-level slot that program
/// storage backs (`uses_global_storage`), so the site degrades to the pre-feature widening path and
/// prints PHP's answer. Measured: this compiles today, warns once, and prints `5` — exactly PHP.
/// Extending the veto to it would turn a working program into a compile error.
#[test]
fn test_program_wide_global_name_stays_retypable() {
    expect_no_error("<?php function w() { global $a; $a = 5; } $a = \"x\"; $a = 2; w(); echo $a;");
    expect_warning(
        "<?php function w() { global $a; $a = 5; } $a = \"x\"; $a = 2; w(); echo $a;",
        "changes type from string to int",
    );
}

/// The veto is scoped to the TOP-LEVEL body, mirroring lowering's `in_main && all_global_var_names`
/// gate. A same-named local inside a FUNCTION body is that frame's own storage — nothing reaches it
/// by the `global` alias — so it stays killable even when another body declares the name `global`.
#[test]
fn test_a_function_local_sharing_a_global_name_stays_killable() {
    let result = check_source_full(
        "<?php function w() { global $a; $a = 5; } function f() { $a = 1; unset($a); $a = \"s\"; echo $a; } f();",
    )
    .expect("a function-local kill must type-check");
    assert_eq!(
        result.local_bind_kill_sites.len(),
        1,
        "a function's own local must stay killable: {:?}",
        result.local_bind_kill_sites
    );
}

/// By-ref closure captures are never killable.
#[test]
fn test_by_ref_capture_not_killable() {
    expect_error("<?php $a = 1; $f = function() use (&$a) { return $a; }; unset($a); $a = \"x\";", "cannot reassign");
}

/// A local passed to a by-ref parameter is aliased from that point on.
#[test]
fn test_by_ref_call_arg_not_killable() {
    expect_error("<?php function f(&$x) { $x = 2; } $a = 1; f($a); unset($a); $a = \"s\";", "cannot reassign");
}

/// A BY-REF PARAMETER itself (`active_ref_params`, not an aliased caller-side local) is excluded
/// from the kill: `unset($x)` on it is a checker no-op, exactly like the pre-feature behavior — a
/// later read sees the still-bound param and a later incompatible assignment is the old hard
/// error, not a fresh kill-then-rebind.
#[test]
fn test_by_ref_param_unset_is_not_a_kill() {
    expect_no_error("<?php function f(&$x) { unset($x); echo $x; } $a = 1; f($a);");
    expect_error(
        "<?php function f(&$x) { unset($x); $x = \"s\"; } $a = 1; f($a);",
        "cannot reassign $x from int to string",
    );
}

/// A BY-REF PARAMETER is also excluded from the straight-line retype: reassigning it to an
/// incompatible type stays the old hard error in permissive mode, whether the param carries an
/// explicit type hint or only the type the call site infers.
#[test]
fn test_by_ref_param_retype_not_permitted() {
    expect_error(
        "<?php function f(int &$x) { $x = \"s\"; } $a = 1; f($a);",
        "cannot reassign $x from int to string",
    );
    expect_error(
        "<?php function f(&$x) { $x = \"s\"; } $a = 1; f($a);",
        "cannot reassign $x from int to string",
    );
}

/// The `$r` side of `$r =& $a` is just as excluded as the `$a` side: a direct retype of the alias
/// TARGET, with no preceding `unset`, is the hard error too.
#[test]
fn test_ref_alias_target_retype_not_permitted() {
    expect_error("<?php $a = 1; $r = &$a; $r = \"s\";", "cannot reassign $r from int to string");
}

/// A local handed to a callable whose signature the checker cannot resolve is aliased.
///
/// The plan's eligibility rule disqualifies a name "passed as an argument to a by-ref parameter
/// anywhere in the body", and mandates conservatism "when the callee cannot be resolved
/// statically". `$cb` here is a `callable` parameter with no signature attached, so nothing says
/// whether its first parameter is by-reference — and if it is, the kill would abandon a slot the
/// callee still holds a reference into. The branch-divergent pre-scan already disqualifies every
/// `ClosureCall`/`ExprCall` argument for the same reason.
#[test]
fn test_unresolved_callable_arg_not_killable() {
    expect_error(
        "<?php function g(callable $cb) { $a = 1; $cb($a); unset($a); $a = \"s\"; echo $a; }",
        "cannot reassign",
    );
}

/// The same rule for a variable function (`$f = \"sort\"; $f($a);`), whose callee is a string
/// resolved at runtime: `sort()` binds its argument by reference, and no signature reaches the
/// call site. Both the `unset` kill and the straight-line retype must step aside.
#[test]
fn test_string_variable_callee_arg_not_killable() {
    expect_error(
        "<?php $f = \"sort\"; $a = 1; $f($a); unset($a); $a = \"s\"; echo $a;",
        "cannot reassign",
    );
    expect_error(
        "<?php $f = \"sort\"; $a = 1; $f($a); $a = \"s\"; echo $a;",
        "cannot reassign",
    );
}

/// Sibling unknown-callee shapes reach the same rule: a dynamic class static call
/// (`$c::m($a)`, which desugars to `call_user_func([$c, "m"], $a)`), a dynamic constructor
/// (`new $c($a)`), and a method call on a `mixed` receiver dispatched over runtime candidates.
#[test]
fn test_unknown_callee_siblings_not_killable() {
    expect_error(
        "<?php class C { static function m(&$x) { $x = 2; } } function g() { $a = 1; $c = \"C\"; $c::m($a); unset($a); $a = \"s\"; echo $a; }",
        "cannot reassign",
    );
    expect_error(
        "<?php function g(string $c) { $a = 1; $x = new $c($a); unset($a); $a = \"s\"; echo $a, $x; }",
        "cannot reassign",
    );
    expect_error(
        "<?php class C { function m(&$x) { $x = 2; } } function g($o) { $a = 1; $o->m($a); unset($a); $a = \"s\"; echo $a; }",
        "cannot reassign",
    );
}

/// The conservatism is per-ARGUMENT, not per-body: an unresolvable call that never mentions `$a`
/// leaves `$a` killable, and a KNOWN by-value signature leaves both the kill and the retype
/// available. Without these controls the rule above could be satisfied by disqualifying
/// everything.
#[test]
fn test_unknown_callee_does_not_over_reach() {
    expect_no_error(
        "<?php $f = \"sort\"; $b = [3, 1]; $a = 1; $f($b); unset($a); $a = \"s\"; echo $a; echo count($b);",
    );
    expect_no_error("<?php function h($x) { return $x; } $a = 1; h($a); unset($a); $a = \"s\"; echo $a;");
    expect_no_error("<?php function h($x) { return $x; } $a = 1; h($a); $a = \"s\"; echo $a;");
}

/// The PHP 8.5 pipe is a call: `$a |> $cb` hands `$a` to `$cb` as its single argument, so an
/// UNRESOLVABLE pipe target aliases it exactly like an unresolvable ordinary callee.
///
/// `$cb` is a `callable` parameter with no signature attached, so `infer_pipe_type` falls through
/// to its syntactic return-type guess with no `ref_params` to consult — the same position the
/// sixteen sites above are in. The branch-divergent pre-scan disqualifies the piped value for this
/// same target (`mixed_storage_scan`'s `Pipe` arm resolves nothing here, so it falls to
/// `disqualify_root`), so recording the alias here is what makes the two sides agree. The surface
/// is narrow — the RFC gives the pipe no by-ref
/// parameters and the known-signature path rejects one outright — but the conservatism must not
/// depend on which call syntax reached the callee.
#[test]
fn test_unresolved_pipe_target_arg_not_killable() {
    expect_error(
        "<?php function g(callable $cb) { $a = 1; $r = $a |> $cb; unset($a); $a = \"s\"; echo $a, $r; }",
        "cannot reassign",
    );
    expect_error(
        "<?php function g(callable $cb) { $a = 1; $r = $a |> $cb; $a = \"s\"; echo $a, $r; }",
        "cannot reassign",
    );
}

/// The pipe controls: a KNOWN pipe target leaves both shapes available, and an unresolvable pipe
/// over a DIFFERENT name leaves `$a` alone.
///
/// A resolved pipe signature is by-value by construction — `check_pipe_known_callable_call`
/// rejects any `ref_params` entry with "Pipe operator does not support by-reference parameters"
/// before it checks anything else — so the known path needs no aliasing of its own and must keep
/// the kill and the retype it grants today. The MARKING side reaches the same conclusion for the
/// same target (see `test_known_by_value_pipe_target_leaves_the_piped_value_markable`), so a known
/// by-value pipe costs a local nothing on either side.
#[test]
fn test_pipe_conservatism_does_not_over_reach() {
    expect_no_error("<?php $a = 1; $r = $a |> strval(...); unset($a); $a = \"s\"; echo $a, $r;");
    expect_no_error("<?php $a = 1; $r = $a |> strval(...); $a = \"s\"; echo $a, $r;");
    expect_no_error(
        "<?php function g(callable $cb) { $a = 1; $b = 2; $r = $b |> $cb; unset($a); $a = \"s\"; echo $a, $r; }",
    );
}

/// The same rule on the MARKING side: a pipe whose target is a known BY-VALUE function leaves the
/// piped value an ordinary read, so a branch-divergent local keeps the boxed-mixed mark that the
/// identical call written WITHOUT the pipe gets.
///
/// The pre-scan used to disqualify every piped value unconditionally, which cost this program its
/// mark purely because of the call syntax: `echo $a |> strval(...);` failed with
/// `cannot reassign $a from int to string` while `echo strval($a);` compiled. The `Pipe` arm now
/// consults the same `callee_may_bind_arguments_by_ref` the ordinary call arm consults, so the two
/// spellings agree.
#[test]
fn test_known_by_value_pipe_target_leaves_the_piped_value_markable() {
    expect_no_error(
        "<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a |> strval(...);",
    );
    // The symmetry control: the same operation without the pipe, which has always marked.
    expect_no_error("<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo strval($a);");
}

/// The conservative half of the `Pipe` arm, unchanged: a target the scan cannot resolve to a name
/// with a signature still disqualifies the piped value, and so does a resolvable BY-REFERENCE one.
///
/// A `callable` parameter carries no signature here, so an unknown callee could bind the piped
/// value by reference for the rest of the body — the divergent assignment stays the pre-feature
/// hard error. The by-reference first-class callable is the arm that proves the new gate is the
/// signature and not the syntax: same shape, same `strval(...)` spelling, different `ref_params`,
/// and the checker rejects the pipe itself on top.
#[test]
fn test_unresolved_or_by_ref_pipe_target_still_disqualifies_the_piped_value() {
    expect_error(
        "<?php function g(callable $cb, int $n) { if ($n > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a |> $cb; }",
        "cannot reassign $a from int to string",
    );
    expect_error(
        "<?php function f(&$x) { $x = 1; return 1; } if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a |> f(...);",
        "cannot reassign $a from int to string",
    );
}

/// An `unset` inside a `try`/`catch`/`finally` sits at conditional depth ≥ 1, so it never kills.
///
/// `try` is a conditional group like an `if`: the block may exit through an exception before the
/// `unset` runs, so the store that would replace the binding is not guaranteed. The kill therefore
/// records nothing and the `unset` stays the pre-feature typing no-op — which is only visible
/// through the decision map, because a depth-0 reassignment BELOW the `try` is picked up by the
/// straight-line retype instead and warns rather than failing.
#[test]
fn test_unset_inside_try_catch_finally_records_no_kill() {
    for source in [
        "<?php $a = 1; try { unset($a); } catch (Throwable $e) {} $a = \"s\"; echo $a;",
        "<?php $a = 1; try { throw new Exception(\"x\"); } catch (Throwable $e) { unset($a); } $a = \"s\"; echo $a;",
        "<?php $a = 1; try { echo \"t\"; } finally { unset($a); } $a = \"s\"; echo $a;",
    ] {
        let result = check_source_full(source).expect("fixture should type-check");
        assert!(
            result.local_bind_kill_sites.is_empty(),
            "an unset at conditional depth must record no kill site ({source}): {:?}",
            result.local_bind_kill_sites
        );
    }
}

/// The same non-eligibility as an ERROR, in the shapes where the straight-line retype cannot
/// rescue the program: a re-binding assignment that is itself inside a branch, and an
/// expression-form one. Both are the hard `cannot reassign` a kill would have avoided.
#[test]
fn test_unset_inside_try_leaves_the_pre_feature_error() {
    expect_error(
        "<?php $a = 1; try { unset($a); } finally { echo \"f\"; } if ($argc > 1) { $a = \"s\"; } echo $a;",
        "cannot reassign $a from int to string",
    );
    expect_error(
        "<?php $a = 1; try { unset($a); } finally { echo \"f\"; } $b = ($a = \"s\"); echo $b;",
        "cannot reassign $a from int to string",
    );
    // And under --strict-locals the depth-0 rescue is gone too, in both modes' shared shape.
    expect_error_strict(
        "<?php $a = 1; try { unset($a); } finally { echo \"f\"; } $a = \"s\"; echo $a;",
        "cannot reassign $a from int to string",
    );
}

/// A retype assignment whose RHS can THROW is no exception to the depth-0 gate.
///
/// The straight-line retype requires conditional depth 0 (`merge_local_assignment_type`'s sibling
/// of `Checker::local_binding_is_killable`), and `try` raises depth for its whole body exactly
/// like an `if` with no `else`. Probed via `--check`: `$s = "heap" . $argc; try { $s =
/// mightThrow($argc); } catch (...) {}`, where `mightThrow` returns `int`, stays the ordinary
/// depth-gated hard error — whether the RHS can throw changes nothing the checker looks at. This
/// LITERAL shape never reaches lowering, so it is pinned here rather than as a codegen e2e
/// fixture.
///
/// This is not the last word on the underlying ownership risk, though: moving the depth-0 retype
/// into a CALLEE and the `try` into the CALLER reaches a shape that DOES compile, because the
/// retype is no longer nested inside the `try` at all — only the CALL that can throw is. See
/// `codegen::locals_retype::test_retype_whose_throwing_rhs_unwinds_out_of_the_callee_frame` for
/// the e2e fixture that pins the unwind-across-a-pending-release case this rejection alone would
/// otherwise leave uncovered.
#[test]
fn test_retype_inside_try_with_throwing_rhs_stays_the_depth_gated_error() {
    expect_error(
        "<?php function mightThrow(int $n): int { if ($n === 1) { throw new Exception(\"boom\"); } return $n; } $s = \"heap\" . $argc; try { $s = mightThrow($argc); } catch (Exception $e) {} echo $s;",
        "cannot reassign $s from string to int",
    );
}

/// A NAMED by-reference argument (`f(x: $a)`) aliases `$a` exactly like the positional form.
///
/// `Checker::record_reference_alias_root` unwraps `ExprKind::NamedArg` on its way to the local, so
/// the name is excluded from the kill and the `unset` degrades to the pre-feature typing no-op.
/// The positional twin is `test_by_ref_call_arg_not_killable`; without the unwrap this shape would
/// silently keep its eligibility while the callee holds a reference to the slot.
#[test]
fn test_named_by_ref_call_arg_not_killable() {
    expect_error(
        "<?php function f(&$x) { $x = 2; } $a = 1; f(x: $a); unset($a); $a = \"s\"; echo $a;",
        "cannot reassign $a from int to string",
    );
    let result = check_source_full(
        "<?php function f(&$x) { $x = 2; } $a = 1; f(x: $a); echo $a;",
    )
    .expect("the named by-ref call alone must type-check");
    assert!(
        result.local_bind_kill_sites.is_empty(),
        "no kill is recorded for a named by-ref argument: {:?}",
        result.local_bind_kill_sites
    );
}

/// Declared-typed locals are a contract: never killable, in both modes.
#[test]
fn test_typed_local_not_killable() {
    expect_error("<?php int $a = 1; unset($a); $a = \"x\";", "cannot reassign");
}

/// Parameters with a declared type hint are a contract: never killable.
/// (An untyped parameter stays killable — pin that too.)
#[test]
fn test_typed_param_not_killable() {
    expect_error("<?php function f(int $a) { unset($a); $a = \"x\"; } f(1);", "cannot reassign");
    expect_no_error("<?php function f($a) { unset($a); $a = \"x\"; echo $a; } f(1);");
}

/// Class properties never reach the local retype paths: pin the declared-property error.
#[test]
fn test_typed_property_stays_strict() {
    expect_error(
        "<?php class C { public string $s = \"a\"; } $c = new C(); $c->s = 5;",
        "Property C::$s expects Str, got Int",
    );
}

/// A local passed to a METHOD's by-ref parameter is aliased too — that path validates its
/// arguments from a `FunctionSig`, not from the `FnDecl` the plain-function test exercises.
#[test]
fn test_method_by_ref_call_arg_not_killable() {
    expect_error(
        "<?php class C { function m(&$x) { $x = 2; } } $c = new C(); $a = 1; $c->m($a); unset($a); $a = \"s\";",
        "cannot reassign",
    );
}

/// A local passed to a BUILTIN's by-ref parameter (`sort`, `preg_match`, …) is aliased: the
/// builtin reaches the local through its storage.
#[test]
fn test_builtin_by_ref_call_arg_not_killable() {
    expect_error(
        "<?php $a = [3, 1]; sort($a); unset($a); $a = \"s\";",
        "cannot reassign",
    );
}

/// A `foreach` value target is bound inside the loop, which may never run, so it is not
/// killable at depth 0 afterwards — even though nothing ever assigned it via `check_assign`.
#[test]
fn test_foreach_bound_local_not_killable() {
    expect_error(
        "<?php foreach ([1, 2] as $v) {} unset($v); $v = \"x\"; echo $v;",
        "cannot reassign",
    );
}

/// Same rule for a `list()` target bound inside a branch: the binding depth is recorded for
/// every shape a conditional statement can introduce, not only for plain assignments.
#[test]
fn test_branch_created_list_unpack_target_not_killable() {
    expect_error(
        "<?php if ($argc > 0) { [$p, $q] = [1, 2]; } unset($p); $p = \"x\"; echo $p;",
        "cannot reassign",
    );
}

/// A local passed to a by-REFERENCE variadic (`&...$xs`) is aliased just like one bound to a
/// regular by-ref parameter: the callee can write back through the collected slot.
#[test]
fn test_by_ref_variadic_call_arg_not_killable() {
    expect_error(
        "<?php function f(&...$xs) { $xs[0] = 9; } $a = 1; f($a); unset($a); $a = \"s\";",
        "cannot reassign",
    );
}

/// A by-VALUE variadic collects copies, so its arguments stay kill-eligible.
#[test]
fn test_by_value_variadic_call_arg_stays_killable() {
    expect_no_error(
        "<?php function f(...$xs) { return count($xs); } $a = 1; f($a); unset($a); $a = \"s\"; echo $a;",
    );
}

/// `foreach ($arr as &$v)` takes references into `$arr`'s elements, so `$arr` is aliased and
/// its binding can no longer be killed.
#[test]
fn test_by_ref_foreach_iterable_not_killable() {
    expect_error(
        "<?php $arr = [1, 2, 3]; foreach ($arr as &$v) { } unset($arr); $arr = \"gone\";",
        "cannot reassign",
    );
}

/// A by-VALUE `foreach` iterates copies, so the iterable stays kill-eligible.
#[test]
fn test_by_value_foreach_iterable_stays_killable() {
    expect_no_error(
        "<?php $arr = [1, 2, 3]; foreach ($arr as $v) { } unset($arr); $arr = \"gone\"; echo $arr;",
    );
}

/// The by-ref foreach VALUE variable is reference-aliased too, so a name already bound at
/// depth 0 before the loop cannot be killed by a later `unset`.
///
/// `foreach ($arr as &$v)` binds `$v` to each element's storage; lowering ref-binds `$v`'s slot
/// (`mark_ref_bound_local`) and then refuses to abandon it, so a kill the checker approved would
/// leave the checker believing the binding ended while the slot still aliases `$arr`'s element.
/// The pre-loop binding is what makes the conditional-depth rule miss this: `$v` is at depth 0
/// from the assignment ABOVE the loop, not from the loop.
#[test]
fn test_by_ref_foreach_value_var_not_killable() {
    expect_error(
        "<?php $v = 0; $arr = [1, 2, 3]; foreach ($arr as &$v) { } unset($v); $v = \"s\"; echo $v;",
        "cannot reassign $v from int to string",
    );
}

/// The by-VALUE twin: `foreach ($arr as $v)` copies each element into `$v`, no alias is created,
/// so a pre-bound `$v` stays kill-eligible and the fresh binding is accepted.
#[test]
fn test_by_value_foreach_value_var_stays_killable() {
    expect_no_error(
        "<?php $v = 0; $arr = [1, 2, 3]; foreach ($arr as $v) { } unset($v); $v = \"s\"; echo $v;",
    );
}

/// Permissive default: an incompatible depth-0 reassignment warns and re-binds.
#[test]
fn test_implicit_retype_warns_by_default() {
    expect_warning("<?php $a = 0; $a = \"ciao\"; echo $a;", "changes type from int to string");
}

/// --strict-locals restores the hard error.
#[test]
fn test_implicit_retype_errors_under_strict_locals() {
    expect_error_strict("<?php $a = 0; $a = \"ciao\"; echo $a;", "cannot reassign $a");
}

/// The `unset()` kill is MODE-INDEPENDENT: `--strict-locals` only tightens the two permissive
/// retype shapes, and dropping a binding never went through either of them.
///
/// The kill's acceptance was only ever pinned in permissive mode, where it says nothing about the
/// flag. Here the rebind after the kill is a FRESH binding, not a retype, so the strict checker
/// has nothing to reject.
#[test]
fn test_unset_then_incompatible_assign_is_accepted_under_strict_locals() {
    expect_no_error_strict("<?php $a = 1; unset($a); $a = \"ciao\"; echo $a;");
}

/// The other half of the same contract: the kill really did end the binding under
/// `--strict-locals` too, so a read with no intervening assignment is the ordinary undefined-name
/// diagnostic rather than a silently surviving `int`.
#[test]
fn test_read_after_unset_still_errors_under_strict_locals() {
    expect_error_strict("<?php $a = 1; unset($a); echo $a;", "Undefined variable");
}

/// A compatible reassignment stays silent.
#[test]
fn test_compatible_reassign_has_no_warning() {
    expect_no_warning("<?php $a = 1; $a = 2;", "changes type");
}

/// unset-then-assign is a fresh binding, not a retype: no warning.
#[test]
fn test_unset_then_assign_has_no_warning() {
    expect_no_warning("<?php $a = 0; unset($a); $a = \"ciao\";", "changes type");
}

/// A conditional incompatible reassignment is not kill/rebind-eligible. The `$a++`
/// write also blocks Task 6's mixed-storage marking, so this fixture stays an error
/// through the whole plan (a plain conditional retype becomes legal in Task 6).
#[test]
fn test_conditional_retype_still_errors() {
    expect_error("<?php $a = 0; if ($argc > 1) { $a = \"x\"; } $a++;", "cannot reassign");
}

/// Interplay: a conditional unset leaves the binding alive, so a later depth-0
/// incompatible reassignment is an ordinary retype (warning), and it is sound:
/// the fresh slot is written on both paths.
#[test]
fn test_retype_after_conditional_unset_warns() {
    expect_warning("<?php $a = 1; if ($argc > 1) { unset($a); } $a = \"x\"; echo $a;", "changes type");
}

/// A ref-aliased local stays an error even in permissive mode.
#[test]
fn test_ref_aliased_retype_still_errors() {
    expect_error("<?php $a = 0; $r =& $a; $a = \"x\";", "cannot reassign");
}

/// A name used as the VALUE variable of a by-reference `foreach` is reference-aliased, so a
/// later incompatible assignment is the hard error in permissive mode too.
///
/// PHP keeps writing through the reference after the loop ends — this fixture makes `$arr[2]`
/// become `"changed"` — so the retype is not a retype at all: it is a store into the last
/// element's storage. Lowering ref-binds `$v`'s slot, refuses to re-bind it for the retype, and
/// degrades to `store_ref_cell_slot` at the CELL's `int` type, which has no `Str` -> `Int`
/// coercion; the permissive re-bind therefore produced a program that miscompiles rather than
/// one that runs. This is the pin for that regression.
#[test]
fn test_by_ref_foreach_value_var_retype_still_errors() {
    expect_error(
        "<?php $v = 0; $arr = [1, 2, 3]; foreach ($arr as &$v) { } $v = \"changed\"; echo $arr[2];",
        "cannot reassign $v from int to string",
    );
}

/// Control for the pin above: with a by-VALUE `foreach` nothing is aliased, so a pre-bound `$v`
/// keeps the ordinary permissive retype — a warning in default mode, the hard error under
/// `--strict-locals`. The by-ref fix must not cost the by-value shape its coverage.
#[test]
fn test_by_value_foreach_value_var_retype_still_warns() {
    expect_warning(
        "<?php $v = 0; $arr = [1, 2, 3]; foreach ($arr as $v) { } $v = \"s\"; echo $v;",
        "$v changes type from int to string",
    );
    expect_error_strict(
        "<?php $v = 0; $arr = [1, 2, 3]; foreach ($arr as $v) { } $v = \"s\"; echo $v;",
        "cannot reassign $v from int to string",
    );
}

/// Control: a by-ref `foreach` value variable the LOOP itself binds was already excluded before
/// this fix — it is bound at conditional depth 1, so it never had a depth-0 binding to re-type —
/// and the permanent alias marking must leave that answer unchanged in both modes.
#[test]
fn test_loop_bound_by_ref_foreach_value_var_retype_still_errors() {
    expect_error(
        "<?php $arr = [1, 2, 3]; foreach ($arr as &$v) { } $v = \"s\"; echo $v;",
        "cannot reassign $v from int to string",
    );
    expect_error_strict(
        "<?php $arr = [1, 2, 3]; foreach ($arr as &$v) { } $v = \"s\"; echo $v;",
        "cannot reassign $v from int to string",
    );
}

/// The mixed-storage half of the same exclusion: a `foreach` value variable — by reference or by
/// value — is disqualified outright by `mixed_storage_scan::collect_stmt`, so a branch-divergent
/// pair of assignments after the loop can never box a slot lowering has ref-bound. Pinned in both
/// modes because the marking is what would otherwise make permissive mode diverge from strict.
#[test]
fn test_by_ref_foreach_value_var_is_never_mixed_marked() {
    let src = "<?php $v = 0; $arr = [1, 2, 3]; foreach ($arr as &$v) { } \
               if ($argc > 1) { $v = 1; } else { $v = \"s\"; } echo $v;";
    expect_error(src, "cannot reassign $v from int to string");
    expect_error_strict(src, "cannot reassign $v from int to string");
}

/// A kill site a SUPERSEDED checker pass recorded must not survive into `CheckResult`.
///
/// The checker walks the top level twice (`check_types_impl`: an initial pass, then a final one
/// after method bodies stabilize). Here the first pass cannot yet know that `$g` holds a closure
/// with a BY-REFERENCE parameter — `make()`'s return type is only inferred by
/// `type_check_methods_until_stable`, which runs between the two passes — so it records no
/// reference alias for `$a`, judges `unset($a)` killable, and records a kill site. The final pass
/// does know, refuses the kill, and leaves `$a` bound (which is why `$a = 5` merges silently
/// instead of erroring). Only the final pass's decision may reach EIR lowering: acting on the
/// stale one would abandon the frame slot the closure still holds a reference to.
#[test]
fn test_superseded_pass_kill_site_does_not_reach_the_result() {
    let result = check_source_full(
        "<?php class C { public function make() { return function (&$x) { $x = 2; }; } } \
         $o = new C(); $a = 1; $g = $o->make(); $g($a); unset($a); $a = 5; echo $a;",
    )
    .expect("fixture should type-check");
    assert!(
        result.local_bind_kill_sites.is_empty(),
        "a superseded pass's kill site survived into CheckResult: {:?}",
        result.local_bind_kill_sites
    );
}

/// The same program with the late-discovered alias REMOVED still records its kill site, so the
/// test above is pinning cross-pass staleness rather than a checker that stopped killing.
#[test]
fn test_final_pass_kill_site_still_reaches_the_result() {
    let result = check_source_full(
        "<?php class C { public function make() { return function ($x) { return $x; }; } } \
         $o = new C(); $a = 1; $g = $o->make(); $g($a); unset($a); $a = 5; echo $a;",
    )
    .expect("fixture should type-check");
    assert_eq!(
        result.local_bind_kill_sites.len(),
        1,
        "the by-VALUE closure parameter leaves $a killable, so its kill site must be recorded"
    );
}

/// A retype at a span that names NO node must never be recorded, however legal the re-bind is.
///
/// `Span::dummy()` is what every compiler-generated AST node carries — the synthetic class
/// builders, the PDO/mysqli/curl preludes, the parser's own desugarings — so it is not an
/// identity, it is a shared bucket (`Span::identifies_a_node`). EIR lowering consults
/// `local_retype_sites` at EVERY `StmtKind::Assign`, so one entry filed under `dummy()` would
/// re-bind the local at every dummy-span assignment in the program at once. The re-bind itself
/// still happens — the WARNING below is what proves the checker did not simply fall back to the
/// hard error — only the span lowering acts on is withheld.
#[test]
fn test_retype_at_a_dummy_span_is_not_recorded() {
    use elephc::parser::ast::{Expr, Stmt};

    let program: elephc::parser::ast::Program = vec![
        Stmt::assign("a", Expr::int_lit(1)),
        Stmt::assign("a", Expr::string_lit("s")),
        Stmt::echo(Expr::var("a")),
    ];
    let result = elephc::types::check(&program).expect("permissive retype should type-check");
    assert!(
        result.local_retype_sites.is_empty(),
        "a retype was recorded at a span that names no node: {:?}",
        result.local_retype_sites
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("changes type")),
        "the permissive re-bind itself must still happen: {:?}",
        result.warnings
    );
}

/// The same re-bind written in real source DOES record its span, so the test above is pinning the
/// dummy-span guard rather than a checker that stopped recording retypes.
#[test]
fn test_retype_in_real_source_is_recorded() {
    let result = check_source_full("<?php $a = 1; $a = \"s\"; echo $a;")
        .expect("permissive retype should type-check");
    assert_eq!(
        result.local_retype_sites.len(),
        1,
        "a depth-0 incompatible reassignment in real source must record exactly one retype site"
    );
    assert!(
        result
            .local_retype_sites
            .iter()
            .all(|(span, names)| span.identifies_a_node()
                && names.len() == 1
                && names.contains("a")),
        "recorded retype sites must name a node AND the local they re-bind: {:?}",
        result.local_retype_sites
    );
}

/// A warning that belongs to a local-binding decision is retracted when a later walk removes the
/// decision, so the diagnostic no longer depends on the ORDER of a function's call sites.
///
/// `f`'s body is walked once per call-site specialization. With `f(1)` first, `$a` is `int` and
/// `$a = "s" . $p` retypes it — warning recorded. The `f("x")` walk then widens the parameter to
/// `mixed`, the assignment merges, and the retype decision is REMOVED — but its warning used to
/// stay. Reversing the two calls produced no warning at all, for the identical program.
#[test]
fn test_retype_warning_does_not_depend_on_call_order() {
    let call_int_first = check_source_full(
        "<?php function f($p) { $a = $p; $a = \"s\" . $p; echo $a; } f(1); f(\"x\");",
    )
    .expect("both specializations must type-check");
    let call_string_first = check_source_full(
        "<?php function f($p) { $a = $p; $a = \"s\" . $p; echo $a; } f(\"x\"); f(1);",
    )
    .expect("both specializations must type-check");
    let retype_warnings = |result: &elephc::types::CheckResult| {
        result
            .warnings
            .iter()
            .filter(|warning| warning.message.contains("changes type"))
            .count()
    };
    assert_eq!(
        retype_warnings(&call_int_first),
        retype_warnings(&call_string_first),
        "the retype warning must not depend on which call site is checked first: {:?} vs {:?}",
        call_int_first.warnings,
        call_string_first.warnings
    );
    assert_eq!(
        retype_warnings(&call_int_first),
        0,
        "the surviving decision is no retype, so nothing must warn about one: {:?}",
        call_int_first.warnings
    );
    assert!(
        call_int_first.local_retype_sites.is_empty(),
        "the last walk removed the retype decision: {:?}",
        call_int_first.local_retype_sites
    );
}

/// Control for the test above: a retype the LAST walk still makes keeps its warning.
#[test]
fn test_a_surviving_retype_still_warns() {
    expect_warning(
        "<?php $a = \"old\" . $argc; $a = 7; echo $a;",
        "changes type from string to int",
    );
}

/// Superglobals are seeded into every environment with no binding depth: they are not bindings
/// the body created, so `unset` must not kill them — EIR lowering would otherwise abandon (and
/// re-mint as a frame slot) storage that lives in an `_eir_global_*` symbol.
///
/// Read through `$_GET` rather than `$_SERVER`, which used to be the subject. The
/// observable here is a RETYPE being refused, which needs the name to still carry
/// an array type; `$_SERVER` is now seeded from `getenv()` and so is `Mixed`, and
/// assigning an int to it is accepted — as PHP accepts it too, `$_SERVER = 5;`
/// being legal there. The guard is about storage surviving `unset`, not about
/// that diagnostic, so it moves to a name where the diagnostic can still see it.
#[test]
fn test_seeded_superglobal_not_killable() {
    expect_error("<?php unset($_GET); $_GET = 5;", "cannot reassign");
}

/// Same rule for the top-level-seeded `$argv`/`$argc`. Measured before the fix: the kill was
/// recorded, lowering abandoned the slot holding the runtime-built argv array, and the program
/// leaked it (`HEAP DEBUG: live_blocks=2`).
#[test]
fn test_seeded_argv_not_killable() {
    expect_error("<?php unset($argv); $argv = 5;", "cannot reassign");
}

/// A by-VALUE closure capture is seeded from the enclosing frame, so it is not killable inside
/// the closure body either.
#[test]
fn test_by_value_capture_not_killable_inside_closure() {
    expect_error(
        "<?php $a = 1; $f = function () use ($a) { unset($a); $a = \"x\"; return $a; }; echo $f();",
        "cannot reassign",
    );
}

/// Branch-divergent assignment is accepted via whole-frame Mixed storage.
#[test]
fn test_branch_divergent_assignment_is_accepted() {
    expect_no_error("<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a;");
}

/// Permissive default: the marking warns once, naming the boxed compilation.
#[test]
fn test_branch_divergent_assignment_warns() {
    expect_warning("<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a;", "boxed mixed storage");
}

/// --strict-locals disables the pre-scan: the divergent assignment errors as today.
#[test]
fn test_branch_divergent_assignment_errors_under_strict() {
    expect_error_strict("<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a;", "cannot reassign");
}

/// Single-branch retype of an outer binding is also handled by marking.
#[test]
fn test_single_branch_retype_of_outer_binding_is_accepted() {
    expect_no_error("<?php $a = 0; if ($argc > 1) { $a = \"x\"; } echo $a;");
}

/// Heterogeneous loop-carried local is handled by marking.
#[test]
fn test_loop_carried_heterogeneous_local_is_accepted() {
    expect_no_error("<?php $a = 0; for ($i = 0; $i < $argc; $i++) { $a = \"s\"; } echo $a;");
}

/// The MARK wins over flow narrowing: a marked local is `Mixed` at EVERY assignment, including one
/// inside a branch that a type guard narrowed.
///
/// The invariant the marking rests on is "a marked name is `Mixed`, and `Mixed` absorbs every later
/// assignment, so the retype hook and the `cannot reassign` error can never fire for it". A guard
/// falsified it: `control_flow` inserts the narrowed type into the shared environment for the
/// guarded body, which pulls the marked name back out of `Mixed`, and the mark was only consulted
/// on the FRESH-INSERT path. Measured before this fix, the fixture below was a hard
/// `cannot reassign $a from int to string` in BOTH modes — permissive included.
#[test]
fn test_marking_survives_a_type_guard_on_the_marked_name() {
    expect_no_error(
        "<?php if ($argc > 1) { $a = 1; } else { $a = \"s\"; } if (is_int($a)) { $a = \"z\"; } echo $a;",
    );
    expect_warning(
        "<?php if ($argc > 1) { $a = 1; } else { $a = \"s\"; } if (is_int($a)) { $a = \"z\"; } echo $a;",
        "boxed mixed storage",
    );
}

/// Control for the test above: `--strict-locals` marks nothing, so the DIVERGENT assignment is the
/// error it always was. The guard changes nothing about that half.
#[test]
fn test_marking_survives_a_type_guard_control_under_strict() {
    expect_error_strict(
        "<?php if ($argc > 1) { $a = 1; } else { $a = \"s\"; } if (is_int($a)) { $a = \"z\"; } echo $a;",
        "cannot reassign $a from int to string",
    );
}

/// A name whose ONLY conflicting assignment sits inside a branch guarded by a type test on that
/// same name is not marked: the checker narrows the name to the guard's target for that branch and
/// restores the pre-`if` binding afterwards, so it never rejects the assignment at all.
///
/// Before this, the scan predicted a rejection the checker never makes: the fixture warned "compile
/// with --strict-locals to make this an error" while `--strict-locals` compiled it CLEAN — false
/// advice — and boxed the frame slot for nothing, on top of blocking constant propagation for the
/// name program-wide.
#[test]
fn test_guard_narrowed_only_conflict_is_not_marked() {
    expect_no_warning(
        "<?php $a = 1; if (is_string($a)) { $a = \"x\"; } echo $a;",
        "boxed mixed storage",
    );
    expect_no_error("<?php $a = 1; if (is_string($a)) { $a = \"x\"; } echo $a;");
    let result = check_source_full("<?php $a = 1; if (is_string($a)) { $a = \"x\"; } echo $a;")
        .expect("a guard-narrowed assignment must type-check");
    assert!(
        result.mixed_storage_store_sites.is_empty(),
        "a name the checker never rejects must not be boxed: {:?}",
        result.mixed_storage_store_sites
    );
}

/// Control for the test above: the same fixture compiles CLEAN under `--strict-locals`, which is
/// exactly why warning about it was wrong.
#[test]
fn test_guard_narrowed_only_conflict_compiles_under_strict() {
    expect_no_error_strict("<?php $a = 1; if (is_string($a)) { $a = \"x\"; } echo $a;");
}

/// The disqualifier is scoped to the guard's own subject: a conflicting assignment guarded by a
/// type test on a DIFFERENT name is ordinary evidence and still marks.
#[test]
fn test_a_guard_on_another_name_does_not_block_marking() {
    expect_warning(
        "<?php $b = \"q\"; $a = 1; if (is_string($b)) { $a = \"x\"; } echo $a;",
        "boxed mixed storage",
    );
}

/// A NEGATED type test does not narrow its subject TO the target in the guarded branch — it
/// narrows to the complement, which for a concrete type is the type itself — so the checker really
/// does reject the assignment and the mark is the right answer.
#[test]
fn test_a_negated_guard_does_not_block_marking() {
    expect_warning(
        "<?php $a = 1; if (!is_string($a)) { $a = \"x\"; } echo $a;",
        "boxed mixed storage",
    );
}

/// A guard on the right NAME but the wrong TYPE is still evidence: the checker narrows `$a` to
/// `float` inside the branch, `float` and `string` do not merge, and the assignment really is
/// rejected — so the name needs its mark, and with it the body compiles and prints PHP's `1`.
///
/// Dropping the acceptance half of the disqualifier turned this into a hard
/// `cannot reassign $a from float to string`, which is what the branch-divergent marking exists to
/// avoid.
#[test]
fn test_a_guard_whose_target_rejects_the_value_still_marks() {
    expect_warning(
        "<?php $a = 1; if (is_float($a)) { $a = \"s\"; } echo $a;",
        "boxed mixed storage",
    );
    expect_no_error("<?php $a = 1; if (is_float($a)) { $a = \"s\"; } echo $a;");
}

/// The INNERMOST guard on a name governs the branch it opens, because the checker's narrowings
/// COMPOSE: `narrow_to` narrows from whatever the environment holds NOW, so the inner
/// `is_float($a)` re-narrows the `string` the outer `is_string($a)` had just installed.
///
/// Testing acceptance against ANY guard on the stack let the outer `string` frame answer for an
/// assignment the inner `float` frame governs, so the site was skipped as "the checker accepts
/// this" when the checker rejects it — no mark, and a hard `cannot reassign $a from float to
/// string` in permissive mode on code PHP runs (it prints `1`).
#[test]
fn test_the_innermost_guard_governs_a_nested_guarded_assignment() {
    expect_no_error(
        "<?php $a = 1; if (is_string($a)) { if (is_float($a)) { $a = \"x\"; } } echo $a;",
    );
    expect_warning(
        "<?php $a = 1; if (is_string($a)) { if (is_float($a)) { $a = \"x\"; } } echo $a;",
        "boxed mixed storage",
    );
}

/// A guarded region is transparent only when EVERY assignment it governs merges, replayed in
/// order from the guard's target — because the checker carries an in-branch assignment forward to
/// the next statement of the same branch and only restores the pre-branch binding when the branch
/// ENDS.
///
/// Judging each assignment against the guard target on its own said "accepted" for `$a = "x"` and
/// left the replay's binding at `int`, so the following `$a = 2` merged cleanly with it and no
/// conflict was seen. The checker meanwhile had `$a` at `string` and rejected `$a = 2`, giving a
/// hard `cannot reassign $a from string to int` where PHP prints `1`.
#[test]
fn test_a_guarded_region_that_rejects_later_is_evidence() {
    expect_no_error("<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = 2; } echo $a;");
    expect_warning(
        "<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = 2; } echo $a;",
        "boxed mixed storage",
    );
}

/// The same rule across the two arms of a nested NON-guard branch: the checker shares one mutable
/// environment across `if`/`else`, so the `else` arm sees what the `then` arm assigned.
#[test]
fn test_a_guarded_region_rejecting_across_inner_branches_is_evidence() {
    expect_no_error(
        "<?php $a = 1; if (is_string($a)) { if ($argc > 1) { $a = \"x\"; } else { $a = 2; } } echo $a;",
    );
    expect_warning(
        "<?php $a = 1; if (is_string($a)) { if ($argc > 1) { $a = \"x\"; } else { $a = 2; } } echo $a;",
        "boxed mixed storage",
    );
}

/// Controls for the three above: `--strict-locals` marks nothing, so each stays the error it was
/// before this feature existed.
#[test]
fn test_guarded_region_shapes_still_error_under_strict() {
    for source in [
        "<?php $a = 1; if (is_string($a)) { if (is_float($a)) { $a = \"x\"; } } echo $a;",
        "<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = 2; } echo $a;",
        "<?php $a = 1; if (is_string($a)) { if ($argc > 1) { $a = \"x\"; } else { $a = 2; } } echo $a;",
    ] {
        expect_error_strict(source, "cannot reassign $a");
    }
}

/// A region whose assignments ALL merge from the guard's target stays transparent, however many
/// there are: the checker accepts every one of them and restores the pre-branch binding at the
/// end, so there is nothing to warn about and nothing to box.
#[test]
fn test_a_wholly_accepted_guarded_region_is_still_not_marked() {
    expect_no_warning(
        "<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = \"y\"; } echo $a;",
        "boxed mixed storage",
    );
    expect_no_error_strict("<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = \"y\"; } echo $a;");
}

/// Regions are grouped by the guard frame that GOVERNS each assignment, not by source adjacency:
/// the outer `is_string` region governs `$a = "p"` and `$a = "q"` (both accepted, transparent)
/// while the nested `is_float` region governs `$a = "x"` on its own (rejected, evidence).
#[test]
fn test_a_nested_region_is_judged_apart_from_the_one_around_it() {
    expect_no_error(
        "<?php $a = 1; if (is_string($a)) { $a = \"p\"; if (is_float($a)) { $a = \"x\"; } $a = \"q\"; } echo $a;",
    );
    expect_warning(
        "<?php $a = 1; if (is_string($a)) { $a = \"p\"; if (is_float($a)) { $a = \"x\"; } $a = \"q\"; } echo $a;",
        "boxed mixed storage",
    );
}

/// Declared-typed locals are never marked (contract wins in both modes).
#[test]
fn test_typed_local_never_mixed() {
    expect_error("<?php int $a = 0; if ($argc > 1) { $a = \"x\"; }", "cannot reassign");
}

/// Ref-aliased locals are never marked.
#[test]
fn test_ref_aliased_never_mixed() {
    expect_error("<?php $a = 0; $r =& $a; if ($argc > 1) { $a = 1; } else { $a = \"x\"; }", "cannot reassign");
}

/// A non-Assign write (++) blocks marking: the divergent assignment stays an error.
#[test]
fn test_incdec_write_blocks_marking() {
    expect_error("<?php $a = 0; if ($argc > 1) { $a = \"x\"; } $a++;", "cannot reassign");
}

/// unset anywhere in the body blocks marking: the name stays unmarked, so the
/// else-branch assignment errors exactly as today (before the unset is even reached).
#[test]
fn test_unset_blocks_marking() {
    expect_error("<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } unset($a); echo $a;", "cannot reassign");
}

/// The marking records EVERY store site of the marked name, each naming a node and the local it
/// boxes, and warns exactly once however many times the checker walks the body.
#[test]
fn test_mixed_storage_store_sites_are_recorded_once() {
    let result = check_source_full("<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a;")
        .expect("a branch-divergent assignment must type-check in permissive mode");
    assert_eq!(
        result.mixed_storage_store_sites.len(),
        2,
        "both branch assignments must be recorded: {:?}",
        result.mixed_storage_store_sites
    );
    assert!(
        result
            .mixed_storage_store_sites
            .iter()
            .all(|(span, names)| {
                span.identifies_a_node() && names.len() == 1 && names.contains("a")
            }),
        "recorded store sites must name a node AND the local they box: {:?}",
        result.mixed_storage_store_sites
    );
    let mixed_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|warning| warning.message.contains("boxed mixed storage"))
        .collect();
    assert_eq!(
        mixed_warnings.len(),
        1,
        "the checker walks the top level twice; the marking must warn once: {:?}",
        result.warnings
    );
    assert_eq!(
        mixed_warnings[0].message,
        "$a is assigned incompatible types (int and string); it is compiled as boxed mixed \
         storage (compile with --strict-locals to make this an error)"
    );
    // The LATER span of the first failing pair: the `else` branch's assignment, at column 41.
    assert_eq!(
        (mixed_warnings[0].span.line, mixed_warnings[0].span.col),
        (1, 41),
        "the warning must land on the later assignment of the first failing pair"
    );
}

/// A marking whose store sites name NO node must not happen at all.
///
/// `Span::dummy()` is what every compiler-generated AST node carries, so a store site filed under
/// it would box locals at every other dummy-span assignment in the program — and, worse, the
/// checker would type this local `Mixed` while lowering never saw the site that boxes its slot.
/// Refusing to mark keeps the two halves in lock-step: the body reports today's error instead.
#[test]
fn test_mixed_storage_marking_at_dummy_spans_is_refused() {
    use elephc::parser::ast::{Expr, Stmt, StmtKind};
    use elephc::span::Span;

    let program: elephc::parser::ast::Program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("argc"),
            then_body: vec![Stmt::assign("a", Expr::int_lit(0))],
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::assign("a", Expr::string_lit("ciao"))]),
        },
        Span::dummy(),
    )];
    let error = elephc::types::check(&program)
        .err()
        .expect("a divergent assignment at dummy spans must stay an error");
    assert!(
        error.message.contains("cannot reassign"),
        "expected today's diagnostic, got: {}",
        error.message
    );
}

/// The scan only trusts value types it can infer EXACTLY (literals and scalar casts).
///
/// `infer_expr_type_syntactic` answers `Int` for everything it does not recognise, a plain
/// `$a = $s` included, so trusting it here would box a local in a program that type-checks
/// perfectly well today.
#[test]
fn test_inexactly_typed_value_blocks_marking() {
    expect_no_warning(
        "<?php $s = \"hello\"; $a = $s; if ($argc > 1) { $a = \"x\"; } echo $a;",
        "boxed mixed storage",
    );
}

/// The scan runs at every per-body entry point, not only at top level.
#[test]
fn test_branch_divergent_assignment_in_a_function_body_is_accepted() {
    expect_no_error(
        "<?php function f(int $n) { if ($n > 1) { $a = 0; } else { $a = \"s\"; } return $a; } echo f(2);",
    );
}

/// Method bodies get the same treatment.
#[test]
fn test_branch_divergent_assignment_in_a_method_body_is_accepted() {
    expect_no_error(
        "<?php class C { public function m(int $n) { if ($n > 1) { $a = 0; } else { $a = \"s\"; } return $a; } } $c = new C(); echo $c->m(2);",
    );
}

/// Closure bodies too.
#[test]
fn test_branch_divergent_assignment_in_a_closure_body_is_accepted() {
    expect_no_error(
        "<?php $f = function (int $n) { if ($n > 1) { $a = 0; } else { $a = \"s\"; } return $a; }; echo $f(2);",
    );
}

/// A conflict the depth-0 retype path already resolved must not box the local.
///
/// `$a = "s"; $a = 0;` re-binds `$a` to a fresh `int` slot (Task 3), so the later `$a = 1` inside
/// the branch merges cleanly and the program compiles today with only the "changes type" warning.
/// Comparing the assignments pairwise instead of replaying them would see (`string`, `int`) and
/// box a local for a conflict that no longer exists.
#[test]
fn test_conflict_already_resolved_by_the_retype_path_is_not_marked() {
    expect_no_warning(
        "<?php $a = \"s\"; $a = 0; if ($argc > 1) { $a = 1; } echo $a;",
        "boxed mixed storage",
    );
    expect_warning(
        "<?php $a = \"s\"; $a = 0; if ($argc > 1) { $a = 1; } echo $a;",
        "changes type from string to int",
    );
}

/// A PARAMETER is never marked: it is already bound on entry, so the fresh-insert hook that gives
/// a marked name boxed storage structurally cannot fire for it.
///
/// `$x` here is an untyped by-value parameter whose two call sites make it `mixed` already, so the
/// body compiles on the PRE-EXISTING mixed-parameter path. Marking it would attribute that storage
/// to this feature and file store sites for a binding the marking never created — spurious keys
/// that block DCE tail-sinking and enter the binding-decision ambiguity tally.
#[test]
fn test_parameter_is_never_marked() {
    expect_no_warning(
        "<?php function h($x, int $n): void { if ($n > 1) { $x = 0; } else { $x = \"s\"; } var_dump($x); } h($argc > 1 ? 42 : \"hello\", 2);",
        "boxed mixed storage",
    );
}

/// A name the body's INCOMING environment seeds, backed by storage the body does not own, is never
/// marked either.
///
/// `$argc`/`$argv` and the extern C globals are seeded into the top-level environment by
/// `seed_global_env`; the request superglobals are seeded into every scope. None of them is a slot
/// this frame created. The marking used to reach them anyway: measured, both fixtures below WARNED
/// and then type-checked, where they are the pre-existing hard error — and a marked name binds
/// `Mixed` at every assignment, so the mark would have boxed program-wide storage the rest of the
/// compiler reaches at its declared type.
///
/// The rejection IS the proof that nothing was marked: a marked name binds `Mixed`, which absorbs
/// every later assignment, so a marked fixture type-checks instead of erroring.
#[test]
fn test_seeded_program_storage_is_never_marked() {
    expect_error(
        "<?php $argv = 1; if ($argc > 1) { $argv = \"s\"; } var_dump($argv);",
        "cannot reassign $argv",
    );
    expect_error(
        "<?php $_SESSION = 1; if ($argc > 1) { $_SESSION = \"s\"; } var_dump($_SESSION);",
        "cannot reassign $_SESSION",
    );
}

/// Control for the test above: a superglobal NAME used as an ordinary local inside a FUNCTION body
/// is still the shared storage (every scope seeds the superglobals), so it stays excluded there
/// too, while a plain local in the same body marks normally.
#[test]
fn test_a_plain_local_beside_a_seeded_name_still_marks() {
    expect_warning(
        "<?php function h(int $n): void { if ($n > 1) { $x = 0; } else { $x = \"s\"; } var_dump($x); } h(2);",
        "boxed mixed storage",
    );
}

/// A by-VALUE closure capture is genuinely pre-bound own storage: it arrives as a hidden parameter
/// into a slot the CLOSURE's frame owns, so it stays markable, and the boxed store type is what
/// releases the previous occupant of the capture slot.
/// `codegen::locals_retype::test_marked_local_captured_by_value_and_overwritten_in_a_closure` is
/// the leak half of this claim (48 bytes without the mark).
///
/// The mark is SILENT here, and the reason is a REPLAY, not the mere fact of being pre-bound. The
/// warning ends "compile with --strict-locals to make this an error", so it may only be withheld
/// when strict really would NOT error. That is answered by replaying the body's assignments to the
/// name from its INCOMING type: here the enclosing `$m` is a ternary-merged union, `int` and
/// `string` both merge into it, strict compiles clean — the advice would be false, so nothing is
/// said. `test_a_capture_whose_incoming_type_rejects_warns` is the other half: an `int`-typed
/// capture reassigned to `string` DOES make strict error, and there the warning is true and is
/// emitted. The mark and the store sites are unaffected either way.
#[test]
fn test_a_by_value_capture_is_marked_silently() {
    let source = "<?php\n$m = $argc > 1 ? 1 : \"z\";\n$f = function (int $n) use ($m) { if ($n > 1) { $m = 0; } else { $m = \"s\"; } return $m; };\nvar_dump($f($argc));";
    expect_no_warning(source, "boxed mixed storage");
    expect_no_error_strict(source);
    let result = check_source_full(source).expect("a marked capture must type-check");
    assert!(
        !result.mixed_storage_store_sites.is_empty(),
        "the silent mark must still record its store sites: {:?}",
        result.mixed_storage_store_sites
    );
    assert!(
        result.mixed_storage_local_names().contains("m"),
        "the silent mark must still box the capture: {:?}",
        result.mixed_storage_store_sites
    );
}

/// A capture whose INCOMING type rejects the body's assignments WARNS: `--strict-locals` really
/// does error on it, so the advice is true and withholding it would hide a real difference.
///
/// The enclosing `$m` is `int` here, not a union, so replaying `$m = 1; … $m = "s";` from `int`
/// fails — exactly what strict reports as `cannot reassign $m from int to string`. Silencing every
/// pre-bound name regardless of that replay muted this one: measured, the body compiled with ZERO
/// diagnostics in permissive mode while strict rejected it.
#[test]
fn test_a_capture_whose_incoming_type_rejects_warns() {
    let source = "<?php\n$m = 1;\n$f = function (int $n) use ($m) { $m = 1; if ($n > 1) { $m = \"s\"; } return $m; };\nvar_dump($f(2));";
    expect_no_error(source);
    expect_warning(source, "boxed mixed storage");
    expect_error_strict(source, "cannot reassign $m");
}

/// A by-REFERENCE capture is never marked. The mark gives the local one boxed `Mixed` slot, and a
/// `use (&$m)` capture's slot IS the caller's storage — so the boxed pointer would be written
/// straight through the alias into the enclosing frame.
///
/// Measured before the exclusion: this body was marked, and the caller's `var_dump($m)` printed
/// `int(4378264920)` — a raw heap pointer — where PHP prints `string(1) "s"`. The design has said
/// from the start that a reference-aliased name is never markable; the scan simply was not
/// consulting the set that knows. `active_ref_params` carries the closure's by-ref captures and is
/// installed before the scan runs, so it is the set that cannot drift from
/// `local_binding_is_killable`'s own view.
///
/// Reverting to "not marked" restores the pre-feature behaviour, which is the hard error.
#[test]
fn test_a_by_ref_capture_is_never_marked() {
    let source = "<?php\n$m = 1;\n$f = function (int $n) use (&$m) { $m = 1; if ($n > 1) { $m = \"s\"; } return $m; };\nvar_dump($f(2));\nvar_dump($m);";
    expect_error(source, "cannot reassign $m");
    expect_error_strict(source, "cannot reassign $m");
}

/// Control for the test above: the same body with a by-VALUE capture is markable, and warns because
/// its `int` incoming type really does make `--strict-locals` reject it.
#[test]
fn test_a_by_value_capture_of_the_same_shape_is_still_marked() {
    let source = "<?php\n$m = 1;\n$f = function (int $n) use ($m) { $m = 1; if ($n > 1) { $m = \"s\"; } return $m; };\nvar_dump($f(2));";
    expect_no_error(source);
    expect_warning(source, "boxed mixed storage");
}

/// The top level installs `active_ref_params` the way every other body does: saved, emptied,
/// restored. `enter_local_binding_scope` does not touch that set, and `check_top_level_program`
/// runs TWICE, so without the reset pass 2 would start with pass 1's `=&` targets already in it —
/// and the pre-scan reads the set now (a reference-aliased name is never markable), which is what
/// turned a tidiness point into a structural one.
///
/// This pin is a TRIPWIRE rather than a reproduction: the leak is provably unobservable today. Every
/// write to `active_ref_params` outside a body scope comes from `check_ref_assign`, and every one of
/// them inserts the `=&` TARGET — a name the scan's `StmtKind::RefAssign` arm disqualifies outright,
/// in both passes, whatever the set says. It would start mattering the moment either half changed,
/// which is exactly when a silent cross-pass difference would be hardest to find.
#[test]
fn test_a_top_level_reference_assignment_does_not_disturb_marking() {
    let source = "<?php $x = 1; $r =& $x; $a = 0; if ($argc > 1) { $a = \"s\"; } echo $a, $r;";
    expect_no_error(source);
    expect_warning(source, "boxed mixed storage");
    let result = check_source_full(source).expect("the fixture must type-check");
    assert!(
        result.mixed_storage_local_names().contains("a"),
        "the unrelated local must still be marked: {:?}",
        result.mixed_storage_store_sites
    );
    assert!(
        !result.mixed_storage_local_names().contains("r"),
        "a reference-assignment target must never be marked: {:?}",
        result.mixed_storage_store_sites
    );
}

/// `is_array($a)` narrows too, so a conflicting store inside its branch is not evidence.
///
/// The recogniser skipped it because `GuardTarget::AnyArray` has no single `PhpType`, on the
/// reasoning that missing a guard "costs only a spurious warning". It is the same truthfulness
/// violation this feature removed everywhere else: measured, `$a = 1; if (is_array($a)) { $a = "x"; }`
/// warned "compile with --strict-locals to make this an error" while `--strict-locals` compiled it
/// CLEAN — the only such hit in a ~120-fixture sweep.
///
/// The target is modelled as `Mixed`, which is what `GuardTarget::AnyArray::fallback_type` yields
/// whenever the guarded name is not already array-typed. It cannot be one for a name this scan
/// marks: `has_exact_syntactic_type` refuses array literals and `(array)` casts outright, so a
/// markable name's replay never holds an array. Over-recognising for an array-typed pre-bound name
/// would only suppress a mark, which reverts that body to the pre-feature error rather than to a
/// wrong answer.
#[test]
fn test_an_is_array_guard_does_not_produce_false_advice() {
    let source = "<?php $a = 1; if (is_array($a)) { $a = \"x\"; } echo $a;";
    expect_no_warning(source, "boxed mixed storage");
    expect_no_error(source);
    expect_no_error_strict(source);
}

/// Controls: the guards whose warning is TRUE keep it. `is_object` is not a narrowing predicate the
/// checker supports at all, `isset` is self-negating (its branch sees the COMPLEMENT), and
/// `is_callable` narrows to a target that rejects a `string` — all three really do make
/// `--strict-locals` report `cannot reassign`, so the advice is accurate and stays.
#[test]
fn test_guards_whose_advice_is_true_still_warn() {
    for source in [
        "<?php $a = 1; if (is_object($a)) { $a = \"x\"; } echo $a;",
        "<?php $a = 1; if (isset($a)) { $a = \"x\"; } echo $a;",
        "<?php $a = 1; if (is_callable($a)) { $a = \"x\"; } echo $a;",
    ] {
        expect_warning(source, "boxed mixed storage");
        expect_error_strict(source, "cannot reassign $a");
    }
}

/// A guard region containing a pre-bound name's FIRST in-body assignment IS transparent: the guard
/// narrows a capture, which is bound on entry, so `guard_narrowing` really does fire there.
///
/// `guard_region_is_transparent` refuses a region holding assignment index 0 because a guard cannot
/// narrow a name with no binding yet — true for a name the body creates, false for one it inherits.
/// Sharing that verdict with the seeded replay made this body WARN while `--strict-locals` compiles
/// it clean: the exact false advice R19b exists to remove, arrived at from the other direction.
#[test]
fn test_a_guard_over_a_pre_bound_names_first_assignment_is_transparent() {
    let source = "<?php\n$m = 1;\n$f = function (int $n) use ($m) { if (is_string($m)) { $m = \"x\"; } if ($n > 1) { $m = 2; } return $m; };\nvar_dump($f(2));";
    expect_no_warning(source, "boxed mixed storage");
    expect_no_error(source);
    expect_no_error_strict(source);
    // SILENT, not unmarked. The seeded replay clears the body — which is why no warning is
    // emitted — but the unseeded replay still finds its own conflict, so the capture keeps its
    // boxed slot. Pinned because "no warning" and "no mark" are different answers and the
    // difference is invisible in a diagnostic-only assertion.
    let result = check_source_full(source).expect("the fixture must type-check");
    assert!(
        result.mixed_storage_local_names().contains("m"),
        "a silently marked capture must still be boxed: {:?}",
        result.mixed_storage_store_sites
    );
}

/// A pre-bound name cannot be killed or re-typed, so its evidence replay must start from the type
/// it ARRIVES with and must not take the depth-0 retype arm.
///
/// `$m = 1; $m = "s";` at depth 0 looks re-typable to an unseeded replay — two depth-0 assignments
/// are exactly what `local_binding_is_killable` accepts — but a capture has no binding depth this
/// body recorded, so the checker refuses and reports `cannot reassign`. The name went unmarked and
/// the body hard-errored in PERMISSIVE mode on code PHP runs (`string(1) "s"`).
#[test]
fn test_a_pre_bound_name_is_not_assumed_retype_eligible() {
    let source = "<?php\n$m = 1;\n$f = function (int $n) use ($m) { $m = 1; $m = \"s\"; return $m; };\nvar_dump($f(2));";
    expect_no_error(source);
    expect_warning(source, "boxed mixed storage");
    expect_error_strict(source, "cannot reassign $m");
}

/// The same seeding fixes a replay that started from the wrong type entirely: `$m = null;` makes an
/// unseeded replay begin at `Void`, which absorbs the later `string`, while the checker begins at
/// the capture's `int` and rejects. Another permissive hard error on code PHP runs.
#[test]
fn test_a_pre_bound_names_replay_starts_from_its_incoming_type() {
    let source = "<?php\n$m = 1;\n$f = function (int $n) use ($m) { $m = null; if ($n > 1) { $m = \"s\"; } return $m; };\nvar_dump($f(2));";
    expect_no_error(source);
    expect_warning(source, "boxed mixed storage");
    expect_error_strict(source, "cannot reassign $m");
}

/// A FRESH closure local is never silenced, however the enclosing scope happens to name its own
/// variables. The gate keys on the closure's by-value CAPTURE list, not on the environment the
/// closure body starts from — that environment is a clone of the whole enclosing scope.
///
/// Measured before the gate keyed on captures: `$m` here was silenced purely because the top level
/// also binds a `$m`, so the body compiled with no diagnostic at all while `--strict-locals`
/// rejected it. Renaming the closure's local to `$q` — which collides with nothing — warned
/// normally, which is what isolated the cause to the name collision.
#[test]
fn test_a_fresh_closure_local_colliding_with_an_outer_name_still_warns() {
    let colliding = "<?php\n$m = 1;\n$f = function (int $n) { $m = 1; if ($n > 1) { $m = \"s\"; } return $m; };\nvar_dump($f(2));\necho $m;";
    let distinct = "<?php\n$m = 1;\n$f = function (int $n) { $q = 1; if ($n > 1) { $q = \"s\"; } return $q; };\nvar_dump($f(2));\necho $m;";
    expect_warning(colliding, "boxed mixed storage");
    expect_warning(distinct, "boxed mixed storage");
    expect_error_strict(colliding, "cannot reassign $m");
    expect_error_strict(distinct, "cannot reassign $q");
}

/// Control for the test above: an ordinary LOCAL of the same closure — one the body itself
/// creates — is marked out loud, because the boxed slot really is a new cost the marking chose.
#[test]
fn test_a_closure_local_is_still_marked_out_loud() {
    expect_warning(
        "<?php\n$f = function (int $n) { if ($n > 1) { $q = 0; } else { $q = \"s\"; } return $q; };\nvar_dump($f($argc));",
        "boxed mixed storage",
    );
}

/// A by-reference builtin argument passed as a SPREAD still aliases the local behind it.
///
/// `check_builtin` skipped the whole by-reference arm for a spread — the lvalue-shape diagnostic
/// underneath has nothing to say about one — and skipped the alias recording with it, so `$args`
/// looked kill/retype eligible after `sort(...$args)` even though `sort` holds a reference into it.
/// The recording now happens before that bail-out, and `record_reference_alias_root` sees through
/// the `Spread` wrapper (as `mixed_storage_scan::disqualify_root` already did).
#[test]
fn test_spread_by_ref_builtin_argument_is_ref_aliased() {
    expect_error(
        "<?php $args = [[3, 1, 2]]; sort(...$args); $args = \"s\"; echo $args;",
        "cannot reassign",
    );
}

/// Control for the test above: the same local with no by-reference builtin in sight is still
/// re-bindable, so the alias is about `sort` and not about spreads generally.
#[test]
fn test_spread_argument_of_a_by_value_builtin_stays_rebindable() {
    expect_no_error("<?php $args = [[3, 1, 2]]; var_dump(...$args); $args = \"s\"; echo $args;");
}

/// Control for the test above: the same shape written as a LOCAL still marks and warns, so the
/// exclusion above is about parameters rather than a scan that stopped working inside functions.
#[test]
fn test_same_shape_local_still_marks_inside_a_function() {
    expect_warning(
        "<?php function h(int $n): void { if ($n > 1) { $x = 0; } else { $x = \"s\"; } var_dump($x); } h(2);",
        "boxed mixed storage",
    );
}

/// A CONCATENATION is exact evidence: `.` yields `string` for every operand pair, in the
/// syntactic scan (`infer_expr_type_syntactic`) and in the typed walk (`Checker::binary_op_type`)
/// alike, so a conflict the scan reads off one is a conflict the checker really rejects.
///
/// Without this the loop fixture `$a = 0; for (…) { $a = "s" . $i; }` was DISQUALIFIED as an
/// inexact value and stayed a hard "cannot reassign $a from int to string" error, which is the
/// one failure mode a lowering test cannot tell apart from a genuine pass.
#[test]
fn test_string_concatenation_is_exact_marking_evidence() {
    expect_warning(
        "<?php $a = 0; for ($i = 0; $i < $argc; $i++) { $a = \"s\" . $i; } echo $a;",
        "boxed mixed storage",
    );
    expect_no_error("<?php $a = 0; for ($i = 0; $i < $argc; $i++) { $a = \"s\" . $i; } echo $a;");
}

/// The concatenation's OPERANDS need no exactness of their own: `.` casts both sides to string,
/// so the result type is a property of the operator, not of what it is applied to.
#[test]
fn test_concatenation_of_inexactly_typed_operands_is_still_exact() {
    expect_warning(
        "<?php $o = [1, 2]; $a = 0; if ($argc > 1) { $a = count($o) . \"x\"; } echo $a;",
        "boxed mixed storage",
    );
}

/// A concatenation still only counts as evidence, never as a licence: a write the scan cannot
/// model reaching the same name disqualifies it exactly as before.
#[test]
fn test_concatenation_evidence_does_not_survive_a_disqualifying_write() {
    expect_error(
        "<?php $a = 0; if ($argc > 1) { $a = \"s\" . $argc; } $a++;",
        "cannot reassign",
    );
}

/// Marking verification for every end-to-end fixture in `codegen::locals_retype` that claims the
/// mixed-storage path.
///
/// A lowering fixture that quietly fell out of the marking would still compile and print the
/// right answer for the branch the harness happens to take (`argc == 1`), so "it passes" proves
/// nothing on its own. Each source below is the VERBATIM fixture text; the assertion is that the
/// checker really marked it.
#[test]
fn test_every_lowering_fixture_takes_the_mixed_storage_path() {
    for source in [
        "<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a;",
        "<?php $a = 41; if ($argc > 0) { $a = \"ciao\"; } echo $a;",
        "<?php $a = 41; if ($argc > 5) { $a = \"ciao\"; } echo $a;",
        "<?php $a = 0; for ($i = 0; $i < $argc; $i++) { $a = \"s\" . $i; } echo $a;",
        "<?php if ($argc > 1) { $a = 42; } else { $a = \"hello\"; } echo strlen($a);",
        "<?php\nif ($argc > 1) { $a = 42; } else { $a = \"hello\"; }\necho strlen($a), \"|\", strtoupper($a), \"|\", gettype($a), \"|\";\nvar_dump(is_string($a));",
        "<?php $a = 123456789; for ($i = 1; $i < $argc; $i++) { $a = \"s\"; } var_dump($a);",
        "<?php\nfunction q() { global $a; $a = 42; }\nif ($argc > 1) { $a = 0; } else { $a = \"hello\"; }\necho $a, \"|\";\nq();\necho $a, \"|\";\nvar_dump($a);",
        "<?php\nclass W { public function w() { global $a; $a = 42; } }\nif ($argc > 1) { $a = 0; } else { $a = \"hello\"; }\necho $a, \"|\";\n(new W())->w();\necho $a, \"|\";\nvar_dump($a);",
        "<?php $a = 0; for ($i = 0; $i < $argc + 3; $i++) { $a = \"s\" . $i; } echo $a;",
        "<?php if ($argc > 1) { $a = 42; } else { $a = \"hello\" . $argc; } echo $a;",
        "<?php\nif ($argc > 1) { $m = 1; } else { $m = \"z\"; }\n$f = function (int $n) use ($m) {\n    if ($n > 1) { $m = 0; } else { $m = \"s\"; }\n    return $m;\n};\nvar_dump($f($argc));\n$g = function () use ($m) { return $m; };\nvar_dump($g());",
        "<?php\nfunction q() { global $a; var_dump($a); }\nif ($argc > 1) { $a = 0; } else { $a = \"hello\"; }\nq();\n$a = 42;\nq();",
        "<?php\n$w = function () { global $a; $a = 42; };\nif ($argc > 1) { $a = 0; } else { $a = \"hello\"; }\necho $a, \"|\";\n$w();\necho $a, \"|\";",
        "<?php\nif ($argc > 1) { $a = 42; } else { $a = \"hello\"; }\n$a = 99;\necho strlen($a);",
        "<?php\nif ($argc > 1) { $a = 42; } else { $a = \"hello\"; }\n$a = 99;\necho str_repeat($a, 2), \"|\", strlen($a), \"|\", strtoupper($a), \"|\", gettype($a), \"|\";\nvar_dump($a);",
        "<?php\n$a = 0;\nif ($argc > 1) { $a = \"s\" . $argc; }\n$a = 5;\necho strlen($a), \"|\", strtoupper($a), \"|\";\nvar_dump($a);",
        "<?php\nfunction f(int $n) {\n    if ($n > 1) { $a = 42; } else { $a = \"hello\"; }\n    $a = 7;\n    return strlen($a) . \"|\" . strtoupper($a) . \"|\" . gettype($a);\n}\necho f($argc), \"\\n\";\nif ($argc > 1) { $c = 1; } else { $c = \"x\"; }\n$c = 3;\nswitch ($c) { case 3: echo \"three|\"; break; default: echo \"other|\"; }\necho ($c == 3 ? \"eq\" : \"ne\"), \"|\", $c + 1, \"|\";\nvar_dump($c);",
        "<?php $a = 0; switch ($argc) { case 1: echo \"one|\"; default: $a = \"ciao\" . $argc; } echo $a, \"|\"; var_dump($a);",
    ] {
        let result = check_source_full(source)
            .unwrap_or_else(|error| panic!("fixture must type-check: {}\n{}", error.message, source));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.message.contains("boxed mixed storage")),
            "fixture never reached the mixed-storage path: {}",
            source
        );
        assert!(
            !result.mixed_storage_store_sites.is_empty()
                && result
                    .mixed_storage_store_sites
                    .keys()
                    .all(|span| span.identifies_a_node()),
            "fixture must record store sites that name a node: {:?} for {}",
            result.mixed_storage_store_sites,
            source
        );
    }
}

/// Marking verification for 28 of the 38 total `changes type from`-emitting end-to-end fixtures in
/// `codegen::locals_retype` that claim the straight-line RETYPE path (shape 2: an incompatible
/// depth-0 reassignment) — the mirror of
/// `test_every_lowering_fixture_takes_the_mixed_storage_path` above, for `local_retype_sites`
/// instead of `mixed_storage_store_sites`.
///
/// A lowering fixture that quietly fell out of the retype marking would still compile and print
/// the right answer for the branch the harness happens to take, so "it passes" proves nothing
/// about the DECISION on its own. Each source below is the VERBATIM fixture text, probed with
/// `--check` before inclusion; the assertion is that the checker really recorded a retype site
/// for it (and, since every one of these warns, that the warning fired too).
///
/// The remaining 10 of the 38 are deliberately NOT here:
/// - 9 are multi-file `require`-based fixtures (e.g.
///   `test_two_different_names_retyped_at_one_position_both_take_effect`,
///   `test_require_once_scopes_the_depth_rule_to_top_level_statements`), which
///   `check_source_full` cannot exercise: it parses one in-memory string with no include
///   resolution, so a fixture whose retype lives in a second `require`d file has no single-string
///   form to probe here.
/// - One superficially similar fixture is excluded on its merits, not on a harness limit:
///   `codegen::locals_retype::test_string_incdec_local_retyped_to_int_still_increments_as_int`'s
///   `$s = "a" . $argc; $s++; echo $s; $s = 5; $s++; echo $s;` was probed and neither warns nor
///   records a retype site (even under `--strict-locals`, where it still compiles clean) — the
///   `++` target marking (`CheckResult::string_incdec_locals`) makes the later `int` store a
///   plain compatible merge rather than an incompatible retype, so it would itself be a vacuous
///   entry here.
#[test]
fn test_every_lowering_fixture_takes_the_retype_path() {
    for source in [
        "<?php $a = $argc; $a = \"ciao\"; echo $a;",
        "<?php $a = \"ciao\" . $argc; $a = 7; echo $a;",
        "<?php $a = $argc; $a = \"n=\" . $a; echo $a;",
        "<?php $a = \"x\"; for ($i = 0; $i < $argc; $i++) { $a .= \"y\"; } $a = 7; echo $a;",
        "<?php $a = $argc; $f = function() use ($a) { return $a; }; $a = \"x\"; echo $f() . $a;",
        "<?php $a = 3; $a = \"ciao\"; echo $a;",
        "<?php $a = \"s\" . $argc; if ($argc > 1) { unset($a); } $a = 7; echo $a;",
        "<?php $x = $argc; $x .= \"a\"; echo $x;",
        "<?php $a = [1, $argc]; $a = \"str\" . $argc; echo $a;",
        "<?php\nclass Box {\n    public int $v;\n    public function __construct(int $v) { $this->v = $v; }\n    public function __destruct() { echo \"bye|\"; }\n}\n$o = new Box($argc);\necho $o->v, \"|\";\n$o = \"gone\" . $argc;\necho $o;",
        "<?php\nclass Box {\n    public int $v;\n    public function __construct(int $v) { $this->v = $v; }\n    public function __destruct() { echo \"bye|\"; }\n}\n$x = $argc;\necho $x, \"|\";\n$x = new Box($argc);\necho $x->v;",
        "<?php\nfunction probe(int $n): string {\n    $a = $n;\n    $a = \"ciao\" . $n;\n    return $a;\n}\necho probe($argc);",
        "<?php\nclass Box {\n    public int $v;\n    public function __construct(int $v) { $this->v = $v; }\n    public function __destruct() { echo \"bye|\"; }\n}\nfunction probe(int $n): string {\n    $o = new Box($n);\n    $arr = [1, $n];\n    echo $o->v, \"|\", $arr[1], \"|\";\n    $o = \"s\" . $n;\n    $arr = \"t\" . $n;\n    return $o . $arr;\n}\necho probe($argc);",
        "<?php\nfunction probe($a, int $n): string {\n    $a = \"grown\" . $n;\n    return $a;\n}\necho probe([1, 2], $argc);",
        "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } echo $q; $q = 1; $q = \"s\"; echo \"|\", $q;",
        "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } echo $q; $q = 1; echo \"|\", $q;",
        "<?php function w() { global $a; $a = 5; } $a = \"x\"; $a = 2; w(); echo $a;",
        "<?php\n$a = [1, $argc];\n$b = $argc;\n$b = $argc > 0 ? \"yes\" : \"no\";\n$a[0] = \"s\";\necho $b, \"|\", $a[0], \"|\", $a[1];",
        "<?php\nfunction probe(int $n): string {\n    $q = \"a\" . $n;\n    if ($n > 5) { echo \"x\"; }\n    $r = $q;\n    $q = 1;\n    return $r . \"|\" . $q;\n}\necho probe($argc);",
        "<?php $a = $argc; $a = \"ciao\" . $argc; echo strlen($a), \"|\", $a;",
        "<?php $a = \"n\" . $argc; $a = strlen($a); echo $a;",
        "<?php $a = \"s\" . $argc; $a = [$a]; echo $a[0];",
        "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } else { echo \"y\"; } echo $q; $q = 1; echo \"|\", $q;",
        "<?php $q = \"a\" . $argc; switch ($argc) { case 9: echo \"x\"; break; default: echo \"y\"; } echo $q; $q = 1; echo \"|\", $q;",
        "<?php $q = \"a\" . $argc; try { echo \"t\"; } catch (Exception $e) { echo \"c\"; } echo $q; $q = 1; echo \"|\", $q;",
        "<?php $n = $argc; if ($argc > 5) { echo \"x\"; } echo $n; $n = \"s\" . $argc; echo \"|\", $n;",
        "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } $q = 1; echo \"|\", $q;",
        "<?php $v = $argc; $arr = [1, 2, 3]; foreach ($arr as $v) { } $v = \"ciao\" . $argc; echo $v;",
    ] {
        let result = check_source_full(source)
            .unwrap_or_else(|error| panic!("fixture must type-check: {}\n{}", error.message, source));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.message.contains("changes type from")),
            "fixture never reached the retype path: {}",
            source
        );
        assert!(
            !result.local_retype_sites.is_empty()
                && result
                    .local_retype_sites
                    .keys()
                    .all(|span| span.identifies_a_node()),
            "fixture must record retype sites that name a node: {:?} for {}",
            result.local_retype_sites,
            source
        );
    }
}

/// The one `codegen::locals_retype` fixture whose ONLY marked name is a by-value capture is
/// verified separately: its mark is SILENT, so it records store sites without warning.
///
/// The outer `$m` here is assigned once (through a ternary) and is not marked at all, so the
/// closure's captured `$m` is the whole of the fixture's mixed-storage claim — and a capture is
/// bound on entry, which is exactly the case the silent-mark rule covers. Verifying it by store
/// sites rather than by warning is what keeps
/// `codegen::locals_retype::test_marked_local_captured_by_value_and_overwritten_in_a_closure`
/// honest about still taking the boxed path.
#[test]
fn test_the_capture_only_lowering_fixture_is_marked_silently() {
    let source = "<?php\n$m = $argc > 1 ? 1 : \"z\";\n$f = function (int $n) use ($m) { if ($n > 1) { $m = 0; } else { $m = \"s\"; } return $m; };\nvar_dump($f($argc));\n$g = function () use ($m) { return $m; };\nvar_dump($g());";
    let result = check_source_full(source).expect("fixture must type-check");
    assert!(
        result.mixed_storage_local_names().contains("m"),
        "the capture must still be boxed: {:?}",
        result.mixed_storage_store_sites
    );
    assert!(
        result
            .mixed_storage_store_sites
            .keys()
            .all(|span| span.identifies_a_node()),
        "recorded store sites must name a node: {:?}",
        result.mixed_storage_store_sites
    );
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("boxed mixed storage")),
        "a mark on a pre-bound name must be silent: {:?}",
        result.warnings
    );
}

/// A body that calls `eval()` anywhere keeps every local binding: the `unset` kill becomes a
/// typing no-op, so no kill site is recorded for lowering to act on.
///
/// The eval scope addresses caller locals BY NAME, while the kill drops the name's frame slot.
/// Measured before this gate: `$a = 1; unset($a); eval('$a = 5;'); echo $a;` printed NOTHING
/// where PHP prints `5`. The flag has to be BODY-scoped rather than point-in-time — the `eval`
/// here sits BELOW the `unset`, so nothing has raised an eval barrier yet when the kill is judged.
#[test]
fn test_unset_in_an_eval_body_records_no_kill_site() {
    let result = check_source_full("<?php $a = 1; unset($a); eval('$a = 5;'); echo $a;")
        .expect("an eval body must still type-check");
    assert!(
        result.local_bind_kill_sites.is_empty(),
        "an eval-calling body must record no kill site: {:?}",
        result.local_bind_kill_sites
    );
}

/// An `unset` that is NOT a whole statement records nothing and changes nothing.
///
/// PHP's grammar makes `unset(...)` a statement and rejects this program outright; elephc's parser
/// accepts it as an expression, so the kill arm was reachable from a ternary operand. The operand
/// may never run, and the kill's effects live on the CHECKER (kill site, binding depth, per-name
/// metadata), so they escaped the discarded branch environment: measured as a recorded kill for the
/// never-taken arm, after which the program printed NOTHING for a local that is still live.
#[test]
fn test_unset_in_a_ternary_arm_records_no_kill() {
    let result = check_source_full(
        "<?php $a = \"x\" . $argc; $c = $argc > 0 ? 1 : unset($a); echo $a, $c;",
    )
    .expect("an expression-position unset must still type-check");
    assert!(
        result.local_bind_kill_sites.is_empty(),
        "an unset outside statement position must record no kill site: {:?}",
        result.local_bind_kill_sites
    );
}

/// The same shape must not silently drop a per-name diagnostic either.
///
/// The kill clears the callable/reflection metadata for the name. Fired from a discarded ternary
/// arm, that clear survived the discard and `$f("s")` lost the signature that reports the bad
/// argument — the program then compiled clean.
#[test]
fn test_unset_in_a_ternary_arm_keeps_the_callable_signature() {
    expect_error(
        "<?php $f = function (int $x) { return $x + 1; }; $c = $argc > 0 ? 1 : unset($f); echo $f(\"s\"), $c;",
        "callable $f parameter $x expects Int, got Str",
    );
}

/// Control for the two tests above: a statement-position `unset` still kills, so the gate is about
/// POSITION and not about a kill that stopped working.
#[test]
fn test_statement_position_unset_still_kills() {
    let result = check_source_full("<?php $a = \"x\" . $argc; unset($a); $a = 5; echo $a;")
        .expect("a statement-position kill must type-check");
    assert_eq!(
        result.local_bind_kill_sites.len(),
        1,
        "a statement-position unset must still record its kill site: {:?}",
        result.local_bind_kill_sites
    );
}

/// Control for the test above: the identical program WITHOUT the `eval` still records its kill,
/// so the gate is about eval rather than a checker that stopped killing.
#[test]
fn test_unset_without_eval_still_records_a_kill_site() {
    let result = check_source_full("<?php $a = 1; unset($a); $a = 5; echo $a;")
        .expect("a plain kill-then-rebind must type-check");
    assert_eq!(
        result.local_bind_kill_sites.len(),
        1,
        "the same shape without eval must still record its kill site: {:?}",
        result.local_bind_kill_sites
    );
}

/// The binding the kill no longer ends is still there, so a later INCOMPATIBLE assignment is the
/// pre-feature hard error rather than a fresh binding.
///
/// Measured before the gate: this compiled with no diagnostic at all and printed NOTHING where
/// PHP prints `7`.
#[test]
fn test_kill_then_rebind_in_an_eval_body_is_an_error() {
    expect_error(
        "<?php $a = \"old\" . $argc; unset($a); $a = 7; eval('echo $a;');",
        "cannot reassign $a",
    );
}

/// The straight-line RETYPE is gated by the same rule, and for the same reason: it too abandons
/// the name's slot and mints a fresh one.
///
/// This shape has no `unset` in it at all, and it was the second silent miscompile found while
/// fixing the first: measured before the gate, `$a = "old"; $a = 7; eval('echo $a;');` compiled
/// with only the retype warning and printed NOTHING where PHP prints `7`.
#[test]
fn test_retype_in_an_eval_body_is_an_error() {
    expect_error(
        "<?php $a = \"old\"; $a = 7; eval('echo $a;');",
        "cannot reassign $a",
    );
    expect_error(
        "<?php $a = \"old\" . $argc; $a = 7; eval('echo $a;');",
        "cannot reassign $a",
    );
}

/// An `eval` in a CLOSURE body poisons that closure's scope only. The closure gets its own body
/// walk, so the enclosing body's flag is untouched and its kill still happens.
///
/// Sound for the reason every other per-body fact is: the fragment addresses the CLOSURE's scope
/// by name, and the one capture form that could reach an enclosing local — `use (&$x)` — already
/// makes that local reference-aliased and therefore neither killable nor re-bindable out here.
#[test]
fn test_eval_inside_a_closure_does_not_gate_the_enclosing_body() {
    let result = check_source_full(
        "<?php $f = function () { eval('$z = 1;'); }; $a = 1; unset($a); $a = \"s\"; echo $a; $f();",
    )
    .expect("the enclosing body must still type-check");
    assert_eq!(
        result.local_bind_kill_sites.len(),
        1,
        "an eval confined to a closure body must leave the enclosing kill in place: {:?}",
        result.local_bind_kill_sites
    );
}

/// The branch-divergent (`Mixed`-storage) shape is deliberately NOT gated by the eval flag: it
/// never ends a binding, it gives the local one boxed `Mixed` slot for the whole frame — which is
/// exactly the representation the eval scope wants.
#[test]
fn test_branch_divergent_marking_survives_an_eval_body() {
    let result = check_source_full(
        "<?php if ($argc > 1) { $b = 1; } else { $b = \"z\"; } eval('echo $b; $b = \"w\";'); echo \"|\", $b;",
    )
    .expect("a marked local in an eval body must still type-check");
    assert_eq!(
        result.mixed_storage_store_sites.len(),
        2,
        "both branch assignments must still be recorded in an eval body: {:?}",
        result.mixed_storage_store_sites
    );
}
