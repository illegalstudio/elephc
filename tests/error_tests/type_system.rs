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

/// Verifies that referencing an undefined variable produces an "Undefined variable" error.
#[test]
fn test_error_undefined_variable() {
    expect_error("<?php echo $x;", "Undefined variable: $x");
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
