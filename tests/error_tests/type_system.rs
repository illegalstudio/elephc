use super::*;

#[test]
fn test_error_null_coalesce_assignment_missing_rhs() {
    expect_error("<?php $x ??=;", "Unexpected token");
}

#[test]
fn test_error_null_coalesce_assignment_type_change() {
    expect_error(
        "<?php $x = 5; $x ??= 2.5;",
        "null coalescing assignment for $x must keep int, got float",
    );
}

#[test]
fn test_error_string_index_requires_integer() {
    expect_error(
        "<?php $s = \"hello\"; echo $s[\"x\"];",
        "String index must be integer",
    );
}

#[test]
fn test_error_string_offset_assignment_is_not_supported() {
    expect_error(
        "<?php $s = \"hello\"; $s[0] = \"H\";",
        "String offset assignment is not supported",
    );
}

#[test]
fn test_error_union_typed_local_rejects_invalid_initializer() {
    expect_error("<?php int|string $value = 1.5;", "cannot initialize $value");
}

#[test]
fn test_error_undefined_variable() {
    expect_error("<?php echo $x;", "Undefined variable: $x");
}

#[test]
fn test_error_type_mismatch_reassign() {
    expect_error("<?php $x = 42; $x = \"hello\";", "cannot reassign $x");
}

#[test]
fn test_error_arithmetic_on_string() {
    expect_error(
        "<?php $x = \"hi\"; echo $x + 1;",
        "Arithmetic operators require numeric operands",
    );
}

#[test]
fn test_error_negate_string() {
    expect_error(
        "<?php $x = \"hi\"; echo -$x;",
        "Cannot negate a non-numeric value",
    );
}

#[test]
fn test_error_comparison_on_string() {
    expect_error(
        "<?php $x = \"a\"; echo $x < 1;",
        "Comparison operators require numeric operands",
    );
}

#[test]
fn test_error_word_logical_missing_rhs() {
    expect_error("<?php echo true xor;", "Unexpected token: Semicolon");
}

#[test]
fn test_error_assignment_expression_rejects_non_lvalue() {
    expect_error("<?php echo 1 = 2;", "Invalid assignment target");
}

#[test]
fn test_error_short_circuit_assignment_effect_is_not_definite() {
    expect_error(
        "<?php echo false && ($x = 1); echo $x;",
        "Undefined variable: $x",
    );
}

#[test]
fn test_error_short_ternary_missing_default() {
    expect_error("<?php echo $x ?:;", "Unexpected token: Semicolon");
}

#[test]
fn test_error_break_outside_loop_or_switch() {
    expect_error("<?php break;", "Cannot 'break' 1 levels");
}

#[test]
fn test_error_break_too_many_levels() {
    expect_error("<?php while (1) { break 2; }", "Cannot 'break' 2 levels");
}

#[test]
fn test_error_continue_too_many_levels() {
    expect_error(
        "<?php while (1) { continue 2; }",
        "Cannot 'continue' 2 levels",
    );
}

#[test]
fn test_error_break_cannot_jump_out_of_finally() {
    expect_error(
        "<?php while (1) { try { echo 1; } finally { break; } }",
        "Cannot jump out of a finally block",
    );
}

#[test]
fn test_error_continue_cannot_jump_out_of_finally() {
    expect_error(
        "<?php while (1) { try { echo 1; } finally { continue; } }",
        "Cannot jump out of a finally block",
    );
}

#[test]
fn test_error_multilevel_break_cannot_jump_out_of_finally() {
    expect_error(
        "<?php while (1) { try { echo 1; } finally { while (1) { break 2; } } }",
        "Cannot jump out of a finally block",
    );
}

#[test]
fn test_error_undefined_function() {
    expect_error("<?php nope();", "Undefined function: nope");
}

#[test]
fn test_error_wrong_arg_count() {
    expect_error(
        "<?php function f($a) { return $a; } f(1, 2);",
        "expects 1 arguments, got 2",
    );
}

#[test]
fn test_error_increment_string() {
    expect_error("<?php $x = \"hi\"; $x++;", "Cannot increment/decrement");
}

// --- Error positions ---

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
}

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

    let load_entries = wad.methods.get("loadEntries").expect("missing loadEntries");
    assert_eq!(
        load_entries.return_type,
        PhpType::Array(Box::new(PhpType::Object("Entry".to_string())))
    );
}

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

#[test]
fn test_error_too_many_args_with_defaults() {
    expect_error(
        "<?php function f($a, $b = 1) { return $a + $b; } f(1, 2, 3);",
        "expects 1 to 2 arguments, got 3",
    );
}

#[test]
fn test_error_too_few_args_with_defaults() {
    expect_error(
        "<?php function f($a, $b = 1) { return $a + $b; } f();",
        "expects 1 to 2 arguments, got 0",
    );
}

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

#[test]
fn test_error_static_property_type_mismatch() {
    expect_error(
        "<?php class Box { public static int $count = 1; } Box::$count = \"x\";",
        "Static property Box::$count expects",
    );
}

#[test]
fn test_error_static_property_redeclaration_type_mismatch() {
    expect_error(
        "<?php class Base { public static int $count = 1; } class Child extends Base { public static string $count = \"x\"; }",
        "Type of Child::$count must be int (as in class Base)",
    );
}

#[test]
fn test_error_date_too_many_args() {
    expect_error(r#"<?php date("Y", 0, 0);"#, "date() takes 1 or 2 arguments");
}

#[test]
fn test_error_json_encode_too_many_args() {
    expect_error(
        r#"<?php json_encode("a", "b");"#,
        "json_encode() takes exactly 1 argument",
    );
}

#[test]
fn test_error_sin_too_many_args() {
    expect_error("<?php sin(1, 2);", "sin() takes exactly 1 argument");
}

#[test]
fn test_error_log_too_many_args() {
    expect_error("<?php log(1, 2, 3);", "log() takes 1 or 2 arguments");
}

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

#[test]
fn test_error_pointer_loose_comparison_is_rejected() {
    expect_error(
        "<?php $x = 1; $p = ptr($x); $q = ptr($x); echo $p == $q;",
        "Loose pointer comparison is not supported; use === or !==",
    );
}

// --- FFI error tests ---

#[test]
fn test_error_static_closure_uses_this_through_short_ternary() {
    expect_error(
        "<?php class C { public int $count = 5; public function bad() { $f = static fn($x) => $x ?: $this->count; return $f; } }",
        "Cannot use $this inside a static closure",
    );
}
