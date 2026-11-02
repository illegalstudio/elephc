//! Purpose:
//! Integration or regression tests for diagnostic coverage of extensions, including packed class rejects non pod field, buffer new rejects non pod element type, and buffer new rejects union element type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies that a packed class with a non-POD field (string) is rejected with a specific error message.
#[test]
fn test_error_packed_class_rejects_non_pod_field() {
    expect_error(
        "<?php packed class Bad { public string $name; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

/// Verifies that buffer_new<T> rejects non-POD element types (string).
#[test]
fn test_error_buffer_new_rejects_non_pod_element_type() {
    expect_error(
        "<?php buffer<string> $names = buffer_new<string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

/// Verifies that buffer_new<T> rejects union element types (int|string).
#[test]
fn test_error_buffer_new_rejects_union_element_type() {
    expect_error(
        "<?php buffer<int|string> $values = buffer_new<int|string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

/// Verifies that a statically known float length is still rejected: only `int` lengths and the
/// runtime-converted `mixed` / `int|float` lengths of int arithmetic size a buffer (task f9ea1d1e).
#[test]
fn test_error_buffer_new_rejects_static_float_length() {
    expect_error(
        "<?php function half(int $n): void { $values = buffer_new<int>($n / 2); } half(4);",
        "buffer_new<T>() length must be integer",
    );
}

/// Verifies that a statically known string length is still rejected, like a string buffer index.
#[test]
fn test_error_buffer_new_rejects_static_string_length() {
    expect_error(
        "<?php string $n = \"2\"; $values = buffer_new<int>($n);",
        "buffer_new<T>() length must be integer",
    );
}

/// Verifies a packed class reusing an ordinary class name (case-insensitively) is still a
/// duplicate now that packed classes are registered before the class schema pass.
#[test]
fn test_error_packed_class_reusing_class_name_is_duplicate() {
    expect_error(
        "<?php class Vec2 {} packed class vec2 { public float $x; }",
        "Duplicate packed class declaration: vec2",
    );
}

/// Verifies an extern class reusing an ordinary class name (case-insensitively) is still a
/// duplicate now that extern classes are registered before the class schema pass.
#[test]
fn test_error_extern_class_reusing_class_name_is_duplicate() {
    expect_error(
        "<?php class Point {} extern class point { public int $x; }",
        "Duplicate class declaration: point",
    );
}

/// Verifies a packed class reusing an enum name is still a duplicate: enums are only
/// pre-declared when the packed prescan runs.
#[test]
fn test_error_packed_class_reusing_enum_name_is_duplicate() {
    expect_error(
        "<?php enum Mode { case A; } packed class Mode { public int $x; }",
        "Duplicate packed class declaration: Mode",
    );
}

/// Verifies a packed class reusing a builtin enum name keeps the builtin redeclaration
/// diagnostic: the packed prescan sees builtin enum names as pointer targets only, not as
/// pre-declared classes that would report a packed duplicate instead.
#[test]
fn test_error_packed_class_reusing_builtin_enum_name_reports_builtin_redeclaration() {
    expect_error(
        "<?php packed class SortDirection { public int $x; }",
        "Cannot redeclare built-in type: SortDirection",
    );
}

/// Verifies that a packed class with a nullable field (?int) is rejected.
#[test]
fn test_error_packed_class_rejects_nullable_field() {
    expect_error(
        "<?php packed class MaybePoint { public ?int $x; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

/// Verifies that assigning a non-buffer element type (bool) to an int buffer element is rejected.
#[test]
fn test_error_buffer_scalar_assign_type_mismatch() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); $values[0] = true;",
        "Buffer element type mismatch",
    );
}

/// Verifies that a statically known string cannot be used to read a buffer,
/// even though a boxed Mixed index is converted to int at runtime.
#[test]
fn test_error_buffer_read_rejects_static_string_index() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); string $index = \"0\"; echo $values[$index];",
        "Buffer index must be integer",
    );
}

/// Verifies that a statically known string cannot be used to write a buffer,
/// preserving the checker boundary around the Mixed runtime conversion.
#[test]
fn test_error_buffer_write_rejects_static_string_index() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); string $index = \"0\"; $values[$index] = 1;",
        "Buffer index must be integer",
    );
}

/// Verifies that a packed buffer element rejects an integer source.
#[test]
fn test_error_buffer_record_copy_local_requires_record() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); $points[0] = 1;",
        "Buffer element type mismatch: expected Vec2 record, got int",
    );
}

/// Verifies a packed buffer element through an object property rejects an integer source.
#[test]
fn test_error_buffer_record_copy_property_requires_record() {
    expect_error(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
class Points { public buffer<Vec2> $values; }
$points = new Points();
$points->values = buffer_new<Vec2>(1);
$points->values[0] = 1;
"#,
        "Buffer element type mismatch: expected Vec2 record, got int",
    );
}

/// Verifies a buffer property uses the same element mismatch diagnostic as a local buffer.
#[test]
fn test_error_buffer_property_scalar_assign_type_mismatch() {
    expect_error(
        r#"<?php
class IntBufferStore { public buffer<int> $values; }
$store = new IntBufferStore();
$store->values = buffer_new<int>(1);
$store->values[0] = "wrong";
"#,
        "Buffer element type mismatch: expected int, got string",
    );
}

/// Verifies a statically known string index is rejected for a buffer property write.
#[test]
fn test_error_buffer_property_write_rejects_static_string_index() {
    expect_error(
        r#"<?php
class IndexedBufferStore { public buffer<int> $values; }
$store = new IndexedBufferStore();
$store->values = buffer_new<int>(1);
string $index = "0";
$store->values[$index] = 1;
"#,
        "Buffer index must be integer",
    );
}

/// Verifies a static buffer property uses the same element mismatch diagnostic as a local buffer.
#[test]
fn test_error_static_buffer_property_scalar_assign_type_mismatch() {
    expect_error(
        r#"<?php
final class StaticIntBuffer { public static buffer<int> $values; }
StaticIntBuffer::$values = buffer_new<int>(1);
StaticIntBuffer::$values[0] = "wrong";
"#,
        "Buffer element type mismatch: expected int, got string",
    );
}

/// Verifies a statically known string index is rejected for a static buffer property write.
#[test]
fn test_error_static_buffer_property_write_rejects_static_string_index() {
    expect_error(
        r#"<?php
final class StaticIndexedBuffer { public static buffer<int> $values; }
StaticIndexedBuffer::$values = buffer_new<int>(1);
string $index = "0";
StaticIndexedBuffer::$values[$index] = 1;
"#,
        "Buffer index must be integer",
    );
}

/// Verifies a packed buffer element through a static property rejects an integer source.
#[test]
fn test_error_buffer_record_copy_static_requires_record() {
    expect_error(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
final class StaticPoints { public static buffer<Vec2> $values; }
StaticPoints::$values = buffer_new<Vec2>(1);
StaticPoints::$values[0] = 1;
"#,
        "Buffer element type mismatch: expected Vec2 record, got int",
    );
}

/// Verifies an append to a static buffer property is rejected with the buffer push diagnostic.
#[test]
fn test_error_static_buffer_property_rejects_push() {
    expect_error(
        r#"<?php
final class StaticPushBuffer { public static buffer<int> $values; }
StaticPushBuffer::$values = buffer_new<int>(1);
StaticPushBuffer::$values[] = 1;
"#,
        "buffer<T> does not support push; allocate with buffer_new<T>(len)",
    );
}

/// Verifies a nested packed field rejects a scalar where a whole record is expected.
#[test]
fn test_error_nested_packed_field_rejects_scalar_record_assignment() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } packed class Body { public Vec2 $pos; } $b = buffer_new<Body>(1); $b[0]->pos = 5;",
        "cannot assign int to packed field Body::pos of type Vec2; a nested packed field takes a Vec2 record",
    );
}

/// Verifies a nested packed field rejects a record of a different packed class.
#[test]
fn test_error_nested_packed_field_rejects_other_record_class() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } packed class Other { public float $x; public float $y; } packed class Body { public Vec2 $pos; } $b = buffer_new<Body>(1); $o = buffer_new<Other>(1); $b[0]->pos = $o[0];",
        "cannot assign ptr<Other> to packed field Body::pos of type Vec2",
    );
}

/// Verifies a packed class that embeds itself has no finite layout and is reported.
#[test]
fn test_error_packed_class_embedding_itself_is_rejected() {
    expect_error(
        "<?php packed class Node { public int $value; public Node $next; }",
        "Packed class Node embeds itself through its packed class fields",
    );
}

/// Verifies packed classes that embed each other in a cycle are reported.
#[test]
fn test_error_packed_classes_embedding_each_other_are_rejected() {
    expect_error(
        "<?php packed class A { public B $b; } packed class B { public A $a; }",
        "Packed class A embeds itself through its packed class fields",
    );
}

/// Verifies predeclared native record names do not make an actually missing pointer target valid.
#[test]
fn test_error_packed_pointer_field_rejects_unknown_target() {
    expect_error(
        "<?php packed class Holder { public ptr<Missing> $value; }",
        "Unknown pointer target type: Missing",
    );
}

/// Verifies reading an undefined field of a nested record names the inner packed class.
#[test]
fn test_error_nested_packed_field_read_of_undefined_inner_field() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } packed class Body { public Vec2 $pos; } $b = buffer_new<Body>(1); echo $b[0]->pos->z;",
        "Undefined packed field: Vec2::z",
    );
}

/// Verifies writing an undefined field of a nested record names the inner packed class.
#[test]
fn test_error_nested_packed_field_write_of_undefined_inner_field() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } packed class Body { public Vec2 $pos; } $b = buffer_new<Body>(1); $b[0]->pos->z = 1.0;",
        "Undefined packed field: Vec2::z",
    );
}

/// Verifies a nested record's scalar field keeps the packed field type check.
#[test]
fn test_error_nested_packed_inner_field_type_mismatch() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } packed class Body { public Vec2 $pos; } $b = buffer_new<Body>(1); $b[0]->pos->x = \"far\";",
        "cannot assign string to packed field Vec2::x of type float",
    );
}

/// Verifies that buffer_len rejects a non-buffer argument (int).
#[test]
fn test_error_buffer_len_requires_buffer_argument() {
    expect_error(
        "<?php echo buffer_len(1);",
        "buffer_len() argument must be buffer<T>",
    );
}

/// Verifies that buffer_ptr rejects a non-buffer argument (int).
#[test]
fn test_error_buffer_ptr_requires_buffer_argument() {
    expect_error(
        "<?php $p = buffer_ptr(1);",
        "buffer_ptr() argument must be buffer<T>",
    );
}

/// Verifies that buffer_ptr rejects calls with more than one argument.
#[test]
fn test_error_buffer_ptr_wrong_arg_count() {
    expect_error(
        "<?php buffer<int> $b = buffer_new<int>(1); $p = buffer_ptr($b, 1);",
        "buffer_ptr() takes exactly 1 argument",
    );
}

/// Verifies that buffer_free rejects a non-buffer argument (int).
#[test]
fn test_error_buffer_free_requires_buffer_argument() {
    expect_error(
        "<?php buffer_free(42);",
        "buffer_free() argument must be buffer<T>",
    );
}

/// Verifies that buffer_free rejects calls with more than one argument.
#[test]
fn test_error_buffer_free_wrong_arg_count() {
    expect_error(
        "<?php buffer<int> $b = buffer_new<int>(1); buffer_free($b, $b);",
        "buffer_free() takes exactly 1 argument",
    );
}

/// Verifies that buffer_free rejects calls with a temporary buffer_new result instead of a local variable.
#[test]
fn test_error_buffer_free_requires_local_variable() {
    expect_error(
        "<?php buffer_free(buffer_new<int>(1));",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that buffer_free rejects when the buffer is passed as a reference parameter.
#[test]
fn test_error_buffer_free_rejects_ref_param() {
    expect_error(
        "<?php function drop(&$buf) { buffer_free($buf); } buffer<int> $buf = buffer_new<int>(1); drop($buf);",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that buffer_free rejects when the buffer is accessed via a global alias inside a function.
#[test]
fn test_error_buffer_free_rejects_global_alias() {
    expect_error(
        "<?php buffer<int> $buf = buffer_new<int>(1); function drop() { global $buf; buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that buffer_free rejects when the buffer is stored in a static variable inside a function.
#[test]
fn test_error_buffer_free_rejects_static_slot() {
    expect_error(
        "<?php function drop() { static $buf = buffer_new<int>(1); buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that extern function parameters with unknown C types (badtype) are rejected.
#[test]
fn test_error_extern_unknown_type() {
    expect_error(
        "<?php extern function foo(badtype $x): int;",
        "Unknown C type: badtype",
    );
}

/// Verifies that a C integer width elephc does not know is rejected like any other unknown C type.
#[test]
fn test_error_extern_unknown_int_width() {
    expect_error(
        "<?php extern function foo(i128 $x): int;",
        "Unknown C type: i128",
    );
}

/// Verifies that a `string` value cannot be stored into an extern class `f32` field: float fields
/// take a float or an int, nothing else.
#[test]
fn test_error_extern_float_field_rejects_string_value() {
    expect_error(
        "<?php extern class FRect { public f32 $x; } $r = ptr_cast<FRect>(ptr_null()); $r->x = \"wide\";",
        "cannot assign Str to extern field FRect::x of type Float",
    );
}

/// Verifies that an extern function with more than eight integer arguments is rejected, whatever
/// widths they declare: this cap is what keeps every AArch64 extern argument in a register, so
/// Apple's natural-size stack packing of narrow arguments never applies.
#[test]
fn test_error_extern_rejects_more_than_eight_integer_arguments() {
    expect_error(
        "<?php extern function nine(i32 $a, i32 $b, i32 $c, i32 $d, i32 $e, i32 $f, i32 $g, i32 $h, i32 $i): i32;",
        "exceeds supported ARM64 register ABI limits",
    );
}

/// Verifies that `ptr_write_f32()` rejects a statically non-numeric value.
#[test]
fn test_error_ptr_write_f32_rejects_string_value() {
    expect_error(
        "<?php $x = 1; $p = ptr($x); ptr_write_f32($p, \"oops\");",
        "ptr_write_f32() value must be float",
    );
}

/// Verifies that `ptr_read_f64()` rejects a non-pointer argument.
#[test]
fn test_error_ptr_read_f64_rejects_int_argument() {
    expect_error("<?php echo ptr_read_f64(42);", "ptr_read_f64()");
}

/// Verifies that an `int` cannot be stored into a one-byte extern `bool` field without a cast.
#[test]
fn test_error_extern_bool_field_rejects_int_value() {
    expect_error(
        "<?php extern class Key { public bool $down; } $k = ptr_cast<Key>(ptr_null()); $k->down = 1;",
        "cannot assign Int to extern field Key::down of type Bool",
    );
}

/// Verifies that a callable cannot be an extern class field: it has no C struct layout.
#[test]
fn test_error_extern_class_callable_field() {
    expect_error(
        "<?php extern class Bad { callable $field; }",
        "Extern class 'Bad' field $field uses an unsupported type",
    );
}

/// Verifies that a float width elephc does not know is rejected like any other unknown C type.
#[test]
fn test_error_extern_unknown_float_width() {
    expect_error(
        "<?php extern function foo(f16 $x): int;",
        "Unknown C type: f16",
    );
}

/// Verifies that explicit C integer widths are rejected on extern globals.
#[test]
fn test_error_extern_global_rejects_int_width() {
    expect_error(
        "<?php extern global u8 $flag;",
        "Extern global $flag uses an explicit C width",
    );
}

/// Verifies that an empty extern block is rejected.
#[test]
fn test_error_extern_block_empty() {
    expect_error("<?php extern \"lib\" { }", "Empty extern block");
}

/// Verifies a malformed extern inside a braced namespace keeps the extern-specific diagnostic
/// and recovers far enough to collect the following malformed statement too.
#[test]
fn test_error_braced_namespace_extern_uses_extern_recovery() {
    let tokens = tokenize("<?php namespace App { extern badkw; echo ; }").unwrap();
    let errors = parse_with_recovery(&tokens).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains(
            "Expected 'function', string literal, 'class', or 'global' after 'extern'"
        )),
        "expected extern-specific error, got {:?}",
        errors,
    );
    assert!(
        errors.len() >= 2,
        "expected namespace recovery to collect both errors, got {:?}",
        errors,
    );
}

/// Verifies that calling an extern function with too few arguments is rejected.
#[test]
fn test_error_extern_wrong_arg_count() {
    expect_error(
        "<?php extern function abs(int $n): int; abs();",
        "Extern function 'abs' expects 1 arguments, got 0",
    );
}

/// Verifies that calling an extern function with a mismatched argument type (int instead of string) is rejected.
#[test]
fn test_error_extern_wrong_arg_type() {
    expect_error(
        "<?php extern function strlen(string $s): int; strlen(123);",
        "Extern function 'strlen' parameter $s expects string, got int",
    );
}

/// Verifies that declaring the same extern function twice is rejected.
#[test]
fn test_error_duplicate_extern_function() {
    expect_error(
        "<?php extern function foo(int $x): int; extern function foo(int $y): int;",
        "Duplicate function declaration: foo",
    );
}

/// Verifies that extern global declarations that would shadow PHP superglobals ($argc, $argv, etc.) are rejected.
#[test]
fn test_error_extern_global_reserved_name() {
    expect_error(
        "<?php extern global int $argc;",
        "extern global $argc would shadow a reserved superglobal",
    );
}

/// Verifies that extern global declarations with void type are rejected.
#[test]
fn test_error_extern_global_void_type() {
    expect_error(
        "<?php extern global void $bad;",
        "Extern global $bad uses an unsupported type",
    );
}

/// Verifies `?` is accepted only before `string` in extern declarations, with a diagnostic that
/// points at `ptr` for nullable pointers.
#[test]
fn test_error_extern_nullable_non_string_type() {
    expect_error(
        "<?php extern function f(?int $x): int;",
        "Only ?string can be nullable in an extern declaration (a ptr already accepts null)",
    );
    expect_error(
        "<?php extern function f(): ?ptr;",
        "Only ?string can be nullable in an extern declaration",
    );
}

/// Verifies `?string` is refused on extern class fields, whose `char*` loads and stores do not
/// model NULL, instead of being laid out silently.
#[test]
fn test_error_extern_class_nullable_string_field() {
    expect_error(
        "<?php extern class Named { public ?string $label; }",
        "Extern class 'Named' field $label cannot be ?string",
    );
}

/// Verifies `?string` is refused on extern globals, which keep the plain `string` conversion.
#[test]
fn test_error_extern_global_nullable_string() {
    expect_error(
        "<?php extern global ?string $name;",
        "Extern global $name cannot be ?string",
    );
}

/// Verifies a `?string` extern parameter still rejects a non-string, non-null argument.
#[test]
fn test_error_extern_nullable_string_param_rejects_int() {
    expect_error(
        "<?php extern function f(?string $s): int; f(123);",
        "Extern function 'f' parameter $s expects string|null, got int",
    );
}

/// Verifies a `?string` extern return must be checked for null before it reaches a plain
/// `string` extern parameter, which cannot pass NULL.
#[test]
fn test_error_extern_nullable_string_return_needs_null_check_for_string_param() {
    expect_error(
        "<?php extern function getenv(string $name): ?string; extern function strlen(string $s): u64; strlen(getenv(\"HOME\"));",
        "Extern function 'strlen' parameter $s expects string, got string|null",
    );
}

/// Verifies `?string` extern parameters accept strings, `null`, and `?string` values, and that a
/// `?string` return becomes a `string` through `??` like any PHP `?string` value.
#[test]
fn test_extern_nullable_string_accepts_string_null_and_nullable_values() {
    expect_no_error(
        r#"<?php
extern function f(?string $s): ?string;
extern function strlen(string $s): u64;
$a = f("x");
$b = f(null);
$c = f($a);
$d = f($argc > 1 ? "y" : null);
echo strlen($b ?? "");
echo f($c) ?? "none";
"#,
    );
}

/// Verifies extern callback string variables are rejected when they are not callable descriptors.
#[test]
fn test_error_extern_callable_requires_literal_function_name() {
    // Verifies that passing a variable string as an extern callback is rejected because it is not a callable descriptor.
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function on_signal($sig) {} $fn = \"on_signal\"; signal(15, $fn);",
        "expects a string literal naming a user function or a callable value",
    );
}

/// Verifies that passing an undefined function name to an extern callable function is rejected.
#[test]
fn test_error_extern_callable_requires_defined_function() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; signal(15, \"missing_handler\");",
        "Undefined callback function: missing_handler",
    );
}

/// Verifies that an extern callable callback with a non-C-compatible return type (string) is rejected.
#[test]
fn test_error_extern_callable_requires_c_compatible_return_type() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function bad_handler($sig) { return \"oops\"; } signal(15, \"bad_handler\");",
        "unsupported return type",
    );
}

/// Verifies that extern class fields with void type are rejected.
#[test]
fn test_error_extern_class_void_field() {
    expect_error(
        "<?php extern class Bad { void $field; }",
        "Extern class 'Bad' field $field uses an unsupported type",
    );
}

/// A statically known-wrong type into a packed `int` field stays a compile error: the
/// runtime-guarded admission is for `Mixed` only, where the value may legitimately be an
/// int and only the runtime tag can tell.
#[test]
fn test_error_packed_int_field_rejects_static_string() {
    expect_error(
        "<?php
        packed class Cell { public int $id; }
        buffer<Cell> $cells = buffer_new<Cell>(1);
        $cells[0]->id = \"x\";
        buffer_free($cells);
        ",
        "cannot assign string to packed field Cell::id of type int",
    );
}

/// A packed `ptr` field takes no value whose type is only known at run time: a pointer boxes
/// under the int tag, so the runtime narrowing scalar fields get could not tell an address from
/// an arbitrary integer.
#[test]
fn test_error_packed_ptr_field_rejects_mixed() {
    expect_error(
        "<?php
        packed class Node { public ptr $next; }
        function load(mixed $v) { return $v; }
        $nodes = buffer_new<Node>(1);
        $nodes[0]->next = load(1);
        ",
        "cannot assign mixed to packed field Node::next of type ptr",
    );
}

/// A statically known int into a packed `bool` field stays a compile error: only a value whose
/// type is decided at run time reaches the strict bool narrowing.
#[test]
fn test_error_packed_bool_field_rejects_static_int() {
    expect_error(
        "<?php
        packed class Cell { public bool $live; }
        $cells = buffer_new<Cell>(1);
        $cells[0]->live = 1;
        ",
        "cannot assign int to packed field Cell::live of type bool",
    );
}

/// A declared `array<int>` local is a contract: an element write that would widen it is a
/// hard error, not the silent `array_to_mixed` conversion an INFERRED array gets.
#[test]
fn test_error_declared_array_element_type_rejects_widening_write() {
    expect_error(
        "<?php array<int> $a = [1, 2, 3]; $a[0] = \"s\"; echo $a[0];",
        "cannot store string into $a declared as array<int>",
    );
}

/// An inferred array keeps widening silently — the contract applies only to declarations.
#[test]
fn test_inferred_array_element_type_still_widens() {
    expect_no_error("<?php $a = [1, 2, 3]; $a[0] = \"s\"; echo $a[0];");
}

/// A bare `array` declaration resolves to `Array(Mixed)`, which absorbs every write without
/// changing, so the declared-element contract must not fire for it.
///
/// Guarding that contract on `elem_ty != val_ty` instead of on the MERGE RESULT made this
/// snippet a spurious error: `Mixed != Str` holds while `merge(Mixed, Str)` is still `Mixed`.
#[test]
fn test_bare_array_declaration_is_not_an_element_contract() {
    expect_no_error("<?php array $a = [1, 2, 3]; $a[0] = \"s\";");
}

/// A declared `array<int>` parameter rejects an `array<string>` argument instead of widening
/// the parameter to `array<mixed>` the way an undeclared parameter would.
#[test]
fn test_error_declared_array_param_rejects_wrong_element_type() {
    expect_error(
        "<?php function firstOf(array<int> $a): int { return $a[0]; } echo firstOf([\"a\"]);",
        "Function 'firstOf' parameter $a expects array<int>, got array<string>",
    );
}

/// A declared `array<T>` RETURN must match the body's element storage.
///
/// Assignability alone does not catch this: `Mixed` is compatible with every type, so an
/// `array<mixed>` body was accepted for a declared `array<int>` and every caller then read
/// the callee's boxed pointers as integers. Measured before the check existed, this exact
/// snippet printed pointer values instead of squares.
#[test]
fn test_error_declared_array_return_rejects_diverging_element_storage() {
    expect_error(
        "<?php function squares(int $n): array<int> { \
         $out = []; for ($i = 1; $i <= $n; $i++) { $out[] = $i * $i; } return $out; }",
        "declares array<int> but returns array<mixed>; the element storage differs",
    );
}

/// Appending the COUNTER ITSELF names `int|float` and says how to fix it.
///
/// The type is what PHP's `++` really produces — an integer that promotes at the overflow
/// boundary — and it is boxed storage, so it cannot be a packed `int` vector however the loop is
/// written. The diagnostic therefore carries the way out rather than leaving a reader to guess
/// it, because "the element storage differs" is true and useless on its own here.
#[test]
fn test_error_appending_a_counter_names_the_int_float_union_and_the_cast() {
    expect_error(
        "<?php function upTo(int $n): array<int> { \
         $out = []; for ($i = 0; $i < $n; $i++) { $out[] = $i; } return $out; }",
        "declares array<int> but returns array<int|float>; the element storage differs. \
         A loop counter is `int|float` after `++`",
    );
}

/// The cast the diagnostic names does satisfy the declared element type.
#[test]
fn test_declared_array_return_accepts_a_cast_counter() {
    expect_no_error(
        "<?php function upTo(int $n): array<int> { \
         $out = []; for ($i = 0; $i < $n; $i++) { $out[] = (int) $i; } return $out; }",
    );
}

/// A matching element type in return position is accepted.
#[test]
fn test_declared_array_return_accepts_matching_element_storage() {
    expect_no_error("<?php function f(): array<int> { return [1, 4, 9]; }");
}

/// A declared bare `array` return imposes no element contract, so a body producing any
/// element type still satisfies it.
#[test]
fn test_bare_array_return_imposes_no_element_contract() {
    expect_no_error(
        "<?php function f(): array { \
         $out = []; for ($i = 0; $i < 3; $i++) { $out[] = $i; } return $out; }",
    );
}

/// An empty-array return satisfies any declared element type: `array<never>` carries no
/// element storage to disagree with.
#[test]
fn test_declared_array_return_accepts_empty_array() {
    expect_no_error("<?php function f(): array<string> { return []; }");
}

/// A declared `array<K, V>` local carries the same contract as the indexed form, and the
/// diagnostic names the form the programmer actually wrote.
#[test]
fn test_error_declared_assoc_array_rejects_widening_write() {
    expect_error(
        "<?php array<string, int> $m = [\"a\" => 1]; $m[\"b\"] = \"nope\"; echo $m[\"a\"];",
        "cannot store string into $m declared as array<string, int>",
    );
}

/// A `mixed` value type absorbs every write, so the contract must not fire for it.
#[test]
fn test_declared_assoc_array_mixed_value_is_not_a_contract() {
    expect_no_error("<?php array<string, mixed> $m = [\"a\" => 1]; $m[\"b\"] = \"ok\";");
}

/// A packed element vector and a hash table are not interchangeable, so an indexed body
/// cannot satisfy a declared associative return.
#[test]
fn test_error_declared_assoc_return_rejects_indexed_body() {
    expect_error(
        "<?php function f(): array<string, int> { return [1, 2, 3]; }",
        "declares array<string, int> but returns array<int>; the storage differs",
    );
}

/// The empty literal carries no keys and no element storage, so it satisfies the
/// associative form too — including a string-keyed one.
#[test]
fn test_declared_assoc_return_accepts_empty_array() {
    expect_no_error("<?php function f(): array<string, int> { return []; }");
}

/// Only `int`, `string` and `mixed` are PHP array keys, so any other key type is rejected
/// at the annotation rather than producing an unreachable hash.
#[test]
fn test_error_assoc_array_rejects_non_key_type() {
    expect_error(
        "<?php function f(array<float, int> $m): int { return 0; }",
        "array<K, V> key type must be int, string, or mixed, got float",
    );
}

/// A generic declaration that is never called is a template and checks clean: its annotations
/// name types with no representation, so it is deliberately never resolved as a function.
#[test]
fn test_uncalled_generic_template_is_not_checked() {
    expect_no_error("<?php function identity<T>(T $value): T { return $value; }");
}

/// A call instantiates the template, and the instantiation type-checks as an ordinary
/// monomorphic function.
#[test]
fn test_generic_call_instantiates_the_template() {
    expect_no_error(
        "<?php function identity<T>(T $value): T { return $value; } echo identity(5);",
    );
}

/// Nothing determines `T` when no parameter position mentions it, and guessing `mixed` would
/// silently give up the storage the annotation exists to pin.
#[test]
fn test_error_generic_call_with_unconstrained_type_param() {
    expect_error(
        "<?php function f<T>(int $x): int { return $x; } echo f(1);",
        "does not determine type parameter <T>",
    );
}

/// Two argument positions bound to one type parameter must agree.
#[test]
fn test_error_generic_call_with_conflicting_bindings() {
    expect_error(
        "<?php function pair<T>(T $a, T $b): T { return $a; } echo pair(1, \"two\");",
        "binds type parameter <T> to both int and string",
    );
}

/// Every distinct argument shape is its own function, so one template can serve several types
/// in the same program without any of them widening.
#[test]
fn test_generic_template_serves_several_types() {
    expect_no_error(
        "<?php \
         function identity<T>(T $value): T { return $value; } \
         echo identity(5), identity(\"s\"), identity(1.5);",
    );
}

/// Inference reaches through an `array<T>` parameter.
#[test]
fn test_generic_call_infers_through_an_array_parameter() {
    expect_no_error(
        "<?php function firstOf<T>(array<T> $xs): T { return $xs[0]; } echo firstOf([1, 2]);",
    );
}

/// A template calling another template instantiates both; the pipeline re-checks until no new
/// instantiation appears.
#[test]
fn test_generic_template_calling_another_template() {
    expect_no_error(
        "<?php \
         function identity<T>(T $value): T { return $value; } \
         function twice<T>(T $value): T { return identity($value); } \
         echo twice(42);",
    );
}

/// A bound states the contract the template's body relies on, so a violating call is rejected
/// at the CALL rather than inside the instantiated body.
#[test]
fn test_error_generic_call_violating_a_bound() {
    expect_error(
        "<?php \
         class Entity { public function __construct(public int $id) {} } \
         function idOf<T : Entity>(T $e): int { return 0; } \
         echo idOf(5);",
        "does not satisfy its bound Entity",
    );
}

/// A SUBCLASS satisfies a class bound.
#[test]
fn test_generic_call_satisfying_a_bound_through_inheritance() {
    expect_no_error(
        "<?php \
         class Entity { public function __construct(public int $id) {} } \
         class User extends Entity {} \
         function idOf<T : Entity>(T $e): int { return $e->id; } \
         echo idOf(new User(1));",
    );
}

/// An IMPLEMENTING class satisfies an interface bound.
#[test]
fn test_generic_call_satisfying_an_interface_bound() {
    expect_no_error(
        "<?php \
         interface Identifiable { public function id(): int; } \
         class Entity implements Identifiable { public function id(): int { return 1; } } \
         function anyId<T : Identifiable>(T $e): int { return $e->id(); } \
         echo anyId(new Entity());",
    );
}

/// An unrelated class does NOT satisfy the bound, even though it has the same members.
///
/// Bound checking asks the checker's class table rather than comparing spellings, so structural
/// resemblance is not enough — this is the case a syntactic check would wrongly admit.
#[test]
fn test_error_generic_call_bound_rejects_an_unrelated_class() {
    expect_error(
        "<?php \
         class Entity { public function id(): int { return 1; } } \
         class Unrelated { public function id(): int { return 0; } } \
         function idOf<T : Entity>(T $e): int { return $e->id(); } \
         echo idOf(new Unrelated());",
        "binds type parameter <T> to Unrelated, which does not satisfy its bound Entity",
    );
}

/// A self-referential bound is checked against the instantiation it names, and the refusal
/// says which: `Cmp<Plain>`, not the template's `Cmp<T>`.
#[test]
fn test_error_f_bounded_call_names_the_substituted_bound() {
    expect_error(
        "<?php \
         interface Cmp<T> { public function cmp(T $o): int; } \
         class Plain { } \
         function smaller<T : Cmp<T>>(T $a, T $b): T { return $a; } \
         echo get_class(smaller(new Plain(), new Plain()));",
        "binds type parameter <T> to Plain, which does not satisfy its bound Cmp<Plain>",
    );
}

/// Implementing the bound's template at ANOTHER argument does not satisfy it: `Other` is a
/// `Cmp<Word>`, and `Sorted<Other>` asks for a `Cmp<Other>`.
#[test]
fn test_error_f_bounded_class_rejects_a_different_instantiation() {
    expect_error(
        "<?php \
         interface Cmp<T> { public function cmp(T $o): int; } \
         class Word implements Cmp<Word> { public function cmp(Word $o): int { return 0; } } \
         class Other implements Cmp<Word> { public function cmp(Word $o): int { return 0; } } \
         class Sorted<T : Cmp<T>> { public function __construct(private T $first) {} } \
         $s = new Sorted<Other>(new Other());",
        "Generic class 'Sorted' binds type parameter <T> to Other, which does not satisfy its \
         bound Cmp<Other>",
    );
}

/// A violated bound is THE error, reported before the instantiated body is checked.
///
/// The bound check used to run after every body, so the first thing reported was whatever the
/// body tripped over — `Undefined method: Plain::label` — which says nothing about the
/// `Shows<Plain>` that caused it.
#[test]
fn test_error_class_bound_is_reported_before_the_body_it_would_break() {
    expect_error(
        "<?php \
         interface Labelled { public function label(): string; } \
         class Plain { } \
         class Shows<T : Labelled> { public function show(T $v): string { return $v->label(); } } \
         $v = new Shows<Plain>();",
        "Generic class 'Shows' binds type parameter <T> to Plain, which does not satisfy its \
         bound Labelled",
    );
}

/// A generic trait's bound is checked at the `use`, and the refusal says it is a trait.
#[test]
fn test_error_generic_trait_use_violating_a_bound() {
    expect_error(
        "<?php \
         interface Labelled { public function label(): string; } \
         class Plain { } \
         trait Shows<T : Labelled> { public function show(T $v): string { return $v->label(); } } \
         class Viewer { use Shows<Plain>; }",
        "Generic trait 'Shows' binds type parameter <T> to Plain, which does not satisfy its \
         bound Labelled",
    );
}

/// A generic trait's members are typed at the instantiation the class uses, so a wrong
/// argument is refused like any typed parameter.
#[test]
fn test_error_generic_trait_member_is_typed_at_its_instantiation() {
    expect_error(
        "<?php \
         trait Holder<T> { private T $item; public function put(T $v): void { $this->item = $v; } } \
         class IntBox { use Holder<int>; } \
         $b = new IntBox(); \
         $b->put(\"seven\");",
        "Method IntBox::put parameter $v expects int, got string",
    );
}

/// A default determines a parameter no argument position constrains, instead of erroring.
#[test]
fn test_generic_type_param_default_replaces_the_unconstrained_error() {
    expect_no_error(
        "<?php function labelFor<K = string>(int $n): int { return $n; } echo labelFor(9);",
    );
}

/// Polymorphic recursion has no finite set of instantiations and is rejected by naming the
/// type that ran away, not by silently giving up after a fixed number of rounds.
#[test]
fn test_error_polymorphic_recursion_is_rejected() {
    expect_error(
        "<?php function deep<T>(T $value): int { return deep([$value]); } echo deep(1);",
        "instantiates itself at an ever-deeper type",
    );
}

/// A type parameter named INSIDE the body — a typed local — is substituted, not left for the
/// checker to report as an unknown type.
#[test]
fn test_generic_body_typed_local_is_substituted() {
    expect_no_error(
        "<?php \
         function hold<T>(T $value): T { T $held = $value; return $held; } \
         echo hold(7), hold(\"seven\");",
    );
}

/// A nested closure's signature is substituted too: the walk reaches every type position, not
/// only the template's own parameter and return types.
#[test]
fn test_generic_body_closure_signature_is_substituted() {
    expect_no_error(
        "<?php \
         function via<T>(T $value): T { $c = function (T $inner): T { return $inner; }; return $c($value); } \
         echo via(7), via(\"seven\");",
    );
}

/// One source position inside a template is reached once per instantiation and legitimately
/// resolves differently each time, so the call-site key carries the enclosing function.
///
/// Keying on the position alone collapsed these two into one and made `twice<string>` invoke
/// `identity<int>`.
#[test]
fn test_one_position_resolves_per_enclosing_instantiation() {
    expect_no_error(
        "<?php \
         function identity<T>(T $value): T { return $value; } \
         function twice<T>(T $value): T { return identity($value); } \
         echo twice(42), twice(\"deep\");",
    );
}


/// A generic CLASS checks its bounds the same way a generic function does, and for the same
/// reason: only the checker's class table can decide whether an argument satisfies one.
#[test]
fn test_error_generic_class_violating_a_bound() {
    expect_error(
        "<?php \
         interface Entity { public function id(): int; } \
         class Vault<T: Entity> { public function __construct(public T $v) {} } \
         $x = new Vault<int>(3);",
        "binds type parameter <T> to int, which does not satisfy its bound Entity",
    );
}

/// A class implementing a generic interface at a violating type is rejected at the clause.
#[test]
fn test_error_generic_interface_implemented_at_a_violating_type() {
    expect_error(
        "<?php \
         interface Entity { public function id(): int; } \
         interface Repository<T: Entity> { public function find(int $id): T; } \
         class Broken implements Repository<int> { public function find(int $id): int { return $id; } }",
        "does not satisfy its bound Entity",
    );
}

/// More type arguments than the template declares is an error, not a silent truncation.
#[test]
fn test_error_generic_class_too_many_type_arguments() {
    expect_error(
        "<?php \
         class Pair<A, B> { public function __construct(public A $a, public B $b) {} } \
         $p = new Pair<int, string, bool>(1, \"x\", true);",
        "takes 2 type argument(s) but 3 were given",
    );
}

/// A parameter with neither an argument nor a default has nothing to bind. Guessing `mixed`
/// would give up exactly the storage the annotation exists to pin.
#[test]
fn test_error_generic_class_missing_type_argument_without_default() {
    expect_error(
        "<?php \
         class Pair<A, B> { public function __construct(public A $a, public B $b) {} } \
         $p = new Pair<int>(1, \"x\");",
        "needs a type argument for <B>, which has no default",
    );
}

/// Type arguments on a class that declares none: the construction position.
#[test]
fn test_error_type_arguments_on_a_non_generic_class_construction() {
    expect_error(
        "<?php class Plain { public int $x = 1; } $p = new Plain<int>();",
        "'Plain' is written with type arguments but declares no type parameters",
    );
}

/// Type arguments on a class that declares none: the annotation position.
///
/// Both positions report the same thing because both are the same mistake; a mention that
/// cannot be instantiated must never reach a pass that has no representation for it.
#[test]
fn test_error_type_arguments_on_a_non_generic_class_annotation() {
    expect_error(
        "<?php class Plain { public int $x = 1; } Plain<int> $p = new Plain();",
        "'Plain' is written with type arguments but declares no type parameters",
    );
}

/// An enum may implement a generic interface at a concrete type. It declares no type parameters
/// of its own — there is no `enum Suit<T>` — so the only generic half it carries is the arguments
/// written on what it implements.
#[test]
fn test_enum_implements_a_generic_interface() {
    expect_no_error(
        "<?php \
         interface Holder<T> { public function get(): T; } \
         enum Status: int implements Holder<int> { \
           case Ready = 1; \
           public function get(): int { return $this->value; } \
         } \
         echo Status::Ready->get();",
    );
}

/// The enum implements the INSTANTIATED interface, so it satisfies a parameter declared at that
/// instantiation — which is the whole point of writing the arguments.
///
/// It does NOT check the enum's method signatures against the interface: elephc does not do that
/// for an enum against any interface, generic or not, and this change deliberately did not make
/// generic interfaces stricter than plain ones.
#[test]
fn test_enum_satisfies_a_parameter_at_its_instantiated_interface() {
    expect_no_error(
        "<?php \
         interface Holder<T> { public function get(): T; } \
         enum Status: int implements Holder<int> { \
           case Ready = 1; \
           public function get(): int { return $this->value; } \
         } \
         function read(Holder<int> $h): int { return $h->get(); } \
         echo read(Status::Ready);",
    );
}

/// A generic class and a generic function compose: neither is instantiated inside the other's
/// template, and both settle in the same fixpoint.
#[test]
fn test_generic_function_over_a_generic_class_parameter() {
    expect_no_error(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function open<T>(Box<T> $b): T { return $b->get(); } \
         echo open(new Box<string>(\"x\"));",
    );
}


/// `new Box(5)` determines `T` from the constructor's declared parameters and the argument
/// types, the same way a generic function call determines its own.
#[test]
fn test_generic_construction_infers_its_type_arguments() {
    expect_no_error(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         echo (new Box(41))->get(), (new Box(\"ok\"))->get();",
    );
}

/// A template whose constructor does not mention `T` has nothing to infer from, and the message
/// names the way out rather than the declaration.
#[test]
fn test_error_generic_construction_with_nothing_to_infer_from() {
    expect_error(
        "<?php class Holder<T> { public function __construct() {} } $h = new Holder();",
        "does not determine type parameter <T>; no constructor parameter mentions it, so write \
         it: new Holder<...>(...)",
    );
}

/// A bound is checked on an INFERRED argument too, and reported at the construction.
#[test]
fn test_error_generic_construction_violating_a_bound() {
    expect_error(
        "<?php \
         interface Entity { public function id(): int; } \
         class Vault<T: Entity> { public function __construct(public T $v) {} } \
         $x = new Vault(3);",
        "Constructing 'Vault' binds type parameter <T> to int, which does not satisfy its bound \
         Entity",
    );
}

/// One construction inside a generic function is reached once per instantiation and means a
/// different class each time. It RESOLVES; it is not rejected.
///
/// The bodies are different AST nodes after splicing — a clone keeps its source spans — so what
/// collided was never the node but the key. Keying on (enclosing function, span), the way the
/// function-call path already did, tells `wrap<int>` from `wrap<string>` apart.
///
/// What that key still cannot separate is two same-named functions in two included files, since
/// a `Span` carries no file identity. That residue stays a compile error naming both classes.
#[test]
fn test_generic_construction_resolves_per_instantiation() {
    expect_no_error(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function wrap<T>(T $v): Box<T> { return new Box($v); } \
         echo wrap(1)->get(), wrap(\"a\")->get();",
    );
}

/// A static factory determines the type arguments the same way `new` does.
///
/// PHP allows exactly one `__construct`, so this is how a real codebase offers more than one
/// way to build something — inference that fired only at `new` would miss most construction.
#[test]
fn test_generic_static_factory_infers_its_type_arguments() {
    expect_no_error(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public static function of(T $v): Box<T> { return new Box<T>($v); } \
         } \
         echo Box::of(7)->get(), Box::of(\"x\")->get();",
    );
}

/// `self` in a type argument is lexical, and is substituted before the class is emitted.
#[test]
fn test_self_as_a_type_argument() {
    expect_no_error(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} } \
         class A { public function wrap(): Box<self> { return new Box<self>($this); } } \
         echo get_class((new A())->wrap());",
    );
}

/// `static` is late-bound, so monomorphization has no single class to name. Refused by name
/// rather than guessed at.
#[test]
fn test_error_static_as_a_type_argument() {
    expect_error(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} } \
         class A { public function f(): Box<static> { return new Box<static>($this); } }",
        "'static' cannot be a type argument",
    );
}

/// `null` says nothing about what a container holds, so inference cannot determine `T` from it.
/// Before this was checked, it substituted through and reported `parameter $v cannot use type
/// void` — a type in a declaration the programmer never wrote.
#[test]
fn test_error_generic_construction_from_a_null_argument() {
    expect_error(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} } \
         $b = new Box(null);",
        "cannot determine <T> from a null argument",
    );
}

/// Writing the type arguments is the way out of that, and it compiles.
///
/// It also exercises the case the checker has to answer on its own: `generics::classes` leaves
/// `new Box<T>` alone because `T` means nothing yet, and the checker then instantiates
/// `wrap<int>` on the fly and walks a body no pass has rewritten.
#[test]
fn test_written_generic_construction_inside_a_generic_function() {
    expect_no_error(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function wrap<T>(T $v): Box<T> { return new Box<T>($v); } \
         echo wrap(1)->get(), wrap(\"a\")->get();",
    );
}

/// `+T` promises the template only ever produces `T`, and a setter consumes it. The diagnostic
/// names the member that breaks the promise, because the declaration is where it is fixable.
#[test]
fn test_covariant_type_parameter_rejected_in_an_input_position() {
    expect_error(
        "<?php \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public function set(T $v): void { $this->v = $v; } \
         } \
         echo (new Box<int>(1))->get();",
        "appears in an input position in method set() parameter $v; a covariant parameter may \
         only appear in an output position",
    );
}

/// The mirror image: `-T` promises the template only consumes.
#[test]
fn test_contravariant_type_parameter_rejected_in_an_output_position() {
    expect_error(
        "<?php \
         class Sink<-T> { \
           public function accept(T $v): void { $x = $v; } \
           public function last(): T { return $this->v; } \
         } \
         $s = new Sink<int>(); $s->accept(1);",
        "appears in an output position in method last() return type; a contravariant parameter \
         may only appear in an input position",
    );
}

/// Polarity COMPOSES. `Sink<T>` in a return type consumes `T` when `Sink` is contravariant, so
/// a `+T` that only ever appears in returns can still be rejected — and has to be, or the
/// marker would promise something the template does not honour.
#[test]
fn test_covariant_parameter_rejected_through_a_contravariant_slot() {
    expect_error(
        "<?php \
         class Sink<-U> { public function accept(U $v): void { $x = $v; } } \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function sink(): Sink<T> { return new Sink<T>(); } \
         } \
         echo get_class(new Box<int>(1));",
        "appears in an input position in method sink() return type",
    );
}

/// A constructor is exempt, and not as a convenience: it cannot be reached through a widened
/// reference, because `new Box<Animal>(...)` names the instantiation it builds. Counting its
/// parameters would make `+T` impossible for every container that stores a `T`.
#[test]
fn test_covariant_parameter_is_allowed_in_the_constructor() {
    expect_no_error(
        "<?php \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         echo (new Box<int>(1))->get();",
    );
}

/// A writable public property is read AND written through a widened reference, so it admits no
/// marker. `readonly` removes the write, and with it the reason.
#[test]
fn test_covariant_parameter_rejected_in_a_writable_public_property() {
    expect_error(
        "<?php \
         class Box<+T> { \
           public T $value; \
           public function get(): T { return $this->value; } \
         } \
         $b = new Box<int>(); echo $b->get();",
        "appears in a position that is both read and written in property $value",
    );
}

/// The STORAGE PROOF. `int` is a register and `mixed` is a boxed tagged cell, so widening them
/// would need a materializing copy rather than the same bytes — which is a different object.
/// The marker is honoured and the widening is still refused.
#[test]
fn test_covariance_refuses_two_arguments_with_different_storage() {
    expect_error(
        "<?php \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function f(Box<mixed> $b): void {} \
         f(new Box<int>(1));",
        "Function 'f' parameter $b expects Box<mixed>, got Box<int>",
    );
}

/// Without a marker, two instantiations stay unrelated — which is what monomorphization makes
/// them, and the default this feature must not disturb.
#[test]
fn test_instantiations_are_invariant_without_a_marker() {
    expect_error(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function f(Box<Animal> $b): void {} \
         f(new Box<Dog>(new Dog()));",
        "Function 'f' parameter $b expects Box<Animal>, got Box<Dog>",
    );
}

/// A refused widening says WHY. Without the hint the message reads as though the marker had been
/// ignored; it was honoured, and the storage is what refused.
#[test]
fn test_refused_widening_explains_the_storage_rule() {
    expect_error(
        "<?php \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function f(Box<mixed> $b): void {} \
         f(new Box<int>(1));",
        "'+T' is covariant, but a widening has to be the same bytes",
    );
}

/// No marker, no hint: the plain mismatch is the whole story there.
#[test]
fn test_invariant_mismatch_carries_no_variance_hint() {
    let error = check_source(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function f(Box<Animal> $b): void {} \
         f(new Box<Dog>(new Dog()));",
    )
    .expect_err("an invariant template must still reject the mismatch");
    assert!(
        !error.contains("covariant") && !error.contains("same bytes"),
        "an invariant template must not be explained as a variance refusal: {error}"
    );
}

/// A variance marker must be kept by the INHERITED surface, not only by the marked class's own
/// members. `Box<+T> extends Holder<T>` inherits `Holder`'s `set(T)`, which consumes `T`.
///
/// Missing this admitted a type confusion: a widened `Box<Animal>` reference wrote a `Cat` into a
/// `Box<Dog>`, and `get(1)` — statically `Dog` — answered `Cat`, exit code 0.
#[test]
fn test_variance_is_checked_against_inherited_members() {
    expect_error(
        "<?php \
         class Holder<T> { \
           public array<T> $items = []; \
           public function set(T $v): void { $this->items[] = $v; } \
           public function get(int $i): T { return $this->items[$i]; } \
         } \
         class Box<+T> extends Holder<T> { \
           public function whoAmI(): string { return get_class($this); } \
         } \
         echo get_class(new Box<int>());",
        "of 'Box' appears in",
    );
}

/// The same promise through an implemented generic interface.
#[test]
fn test_variance_is_checked_against_an_implemented_generic_interface() {
    expect_error(
        "<?php \
         interface Consumer<T> { public function accept(T $v): void; } \
         class Box<+T> implements Consumer<T> { \
           public function accept(T $v): void { $x = $v; } \
         } \
         echo get_class(new Box<int>());",
        "of 'Box' appears in an input position",
    );
}

/// Coverage must not depend on the ORDER of union members. `T|Sink<T>` and `Sink<T>|T` are the
/// same type; recording the first occurrence whatever its polarity accepted one and rejected the
/// other, because `T` at `Out` is admitted under `+T` and the walk stopped there.
#[test]
fn test_variance_violation_is_found_wherever_it_sits_in_a_union() {
    for declared in ["T|Sink<T>|null", "Sink<T>|T|null"] {
        expect_error(
            &format!(
                "<?php \
                 class Sink<-T> {{ public function put(T $x): void {{ $y = $x; }} }} \
                 class Source<+T> {{ \
                   public function __construct(private T $v) {{}} \
                   public function leak(): {} {{ return null; }} \
                 }} \
                 echo get_class(new Source<int>(1));",
                declared
            ),
            "appears in an input position in method leak() return type",
        );
    }
}

/// Polarity composes through a CALLABLE: its parameters flip the enclosing polarity, its return
/// keeps it. Without the arm, the `_` fallback swallowed the whole type and every violation
/// reached through a callback was admitted.
#[test]
fn test_variance_composes_through_a_typed_callable() {
    expect_error(
        "<?php \
         class Sink<-T> { \
           public function __construct(private T $v) {} \
           public function apply(callable(T): void $f): void { $f($this->v); } \
         } \
         echo get_class(new Sink<int>(1));",
        "of 'Sink' appears in an output position in method apply() parameter $f",
    );
    expect_error(
        "<?php \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public function fill(callable(int): T $f): void { $x = $f(1); } \
         } \
         echo get_class(new Box<int>(1));",
        "of 'Box' appears in an input position in method fill() parameter $f",
    );
}

/// A written type argument list longer than the declaration is refused, not truncated.
#[test]
fn test_error_written_type_arguments_reject_extra_arguments() {
    expect_error(
        "<?php function identity<T>(T $v): T { return $v; } echo identity<int, string>(1);",
        "writes 2 type arguments, but it declares 1",
    );
}

/// A written list that stops short of a parameter with no default is refused.
///
/// The alternative — inferring the rest from the call's arguments — would make one list half
/// written and half inferred, which is two rules for one syntax.
#[test]
fn test_error_written_type_arguments_reject_an_unbound_parameter() {
    expect_error(
        "<?php function pair<A, B>(A $a, B $b): string { return 'ok'; } echo pair<int>(1, 'x');",
        "leaves <B> unbound, and it declares no default",
    );
}

/// A bound is checked against the WRITTEN argument exactly as against an inferred one.
#[test]
fn test_error_written_type_argument_must_satisfy_its_bound() {
    expect_error(
        "<?php class Entity { public function id(): int { return 1; } } \
         function idOf<T : Entity>(T $e): int { return $e->id(); } echo idOf<string>('x');",
        "binds type parameter <T> to string, which does not satisfy its bound Entity",
    );
}

/// The same refusal for a generic METHOD, named by class and method.
#[test]
fn test_error_written_type_arguments_on_a_method_reject_extra_arguments() {
    expect_error(
        "<?php class Box<T> { \
           public function __construct(private T $v) {} \
           public function map<U>(callable(T): U $f): Box<U> { return new Box<U>($f($this->v)); } \
         } \
         $b = new Box<int>(1); $b->map<string, int>(fn(int $x): string => 'n');",
        "Call to generic method 'Box<int>::map' writes 2 type arguments, but it declares 1",
    );
}

/// A template declared inside a conditional is named as such, instead of being called empty.
///
/// The old answer — `'Box' is written with type arguments but declares no type parameters` — was
/// false: the declaration is right there and declares one. It is not COLLECTED, because a
/// declaration inside a conditional is not part of the compiled program; an ordinary `class Foo`
/// in the same place is just as invisible (`Undefined class: Foo`), and instantiating a template
/// there would splice its instantiations at the top level, existing unconditionally next to the
/// class that does not.
#[test]
fn test_error_template_declared_inside_a_conditional_says_where_it_is() {
    expect_error(
        "<?php if (!class_exists('Box')) { \
           class Box<T> { public function __construct(private T $v) {} } \
         } \
         $b = new Box<int>(1);",
        "'Box' declares type parameters, but inside a conditional",
    );
}

/// A template at the top level of a NAMESPACE is not conditional, and must keep instantiating.
///
/// The scan that records conditional declarations deliberately does not descend into a namespace
/// block: its body is ordinary top-level code, and listing it would report every namespaced
/// template as unreachable.
#[test]
fn test_namespaced_template_is_not_reported_as_conditional() {
    expect_no_error(
        "<?php namespace App; \
         class Box<T> { public function __construct(private T $v) {} \
                        public function get(): T { return $this->v; } } \
         $b = new Box<int>(1); echo $b->get();",
    );
}

/// A bare INVARIANT template name takes exactly the instantiation it was read as.
///
/// `Box` with no bound reads as `Box<mixed>`, and `Box<int>` is a different class — that is what
/// monomorphization means. The refusal names both, so the reading is visible at the call as well
/// as in the warning at the declaration.
#[test]
fn test_error_bare_template_name_does_not_accept_another_instantiation() {
    expect_error(
        "<?php class Box<T> { public function __construct(private T $v) {} \
                              public function get(): T { return $this->v; } } \
         function unwrap(Box $b) { return $b->get(); } \
         echo unwrap(new Box<int>(5));",
        "parameter $b expects Box<mixed>, got Box<int>",
    );
}

/// Verifies an array element bound to a `?ptr<T>` by-reference parameter is rejected: the callee
/// writes a raw pointer word back, and the element lives in null or boxed element storage.
#[test]
fn test_error_nullable_pointer_by_ref_parameter_rejects_array_element() {
    expect_error(
        "<?php packed class Vec2 { public int $x; } function fill(?ptr<Vec2> &$p): void {} $a = [null]; fill($a[0]);",
        "a pointer by-reference parameter writes a pointer word back through its argument, so pass a variable",
    );
}

/// Rejects an unknown class nested inside a pointer declaration.
#[test]
fn test_error_nested_pointer_unknown_target() {
    expect_error("<?php function f(ptr<ptr<Missing>> $p): void {}", "Unknown pointer target type: ptr<Missing>");
}

/// Preserves the diagnostic for sizeof targets that have no supported storage layout.
#[test]
fn test_error_pointer_sizeof_width_or_nested_target() {
    expect_error("<?php echo ptr_sizeof(\"u8\");", "Unknown type for ptr_sizeof(): u8");
    expect_error("<?php echo ptr_sizeof(\"ptr<u8>\");", "Unknown type for ptr_sizeof(): ptr<u8>");
}

/// A rooted width spelling names a class, not an unqualified C width.
#[test]
fn test_error_rooted_width_pointer_target() {
    expect_error(r"<?php function f(ptr<\u8> $p): void {}", "Unknown pointer target type: u8");
    expect_error(r"<?php function f(ptr<ptr<\u8>> $p): void {}", r"Unknown pointer target type: ptr<\u8>");
}

/// Generic binding preserves a rooted width spelling as a class name.
#[test]
fn test_error_generic_rooted_width_pointer_target() {
    expect_error(r"<?php function id<T>(T $x): T { return $x; } $p = id<ptr<\u8>>(ptr_null());", "Unknown pointer target type: u8");
}

/// A width pointer cannot access a same-named global class's fields.
#[test]
fn test_error_width_pointer_does_not_alias_global_class() {
    expect_error(r"<?php namespace { packed class u8 { public i32 $value; } } namespace App { function f(ptr<u8> $p): int { return $p->value; } }", "Property access requires an object or typed pointer");
}

/// Pointer width tags stay out of diagnostics for extern field writes and missing returns.
#[test]
fn test_error_pointer_width_tag_is_not_exposed() {
    expect_error(r#"<?php extern class Blob { public ptr<u8> $data; } $b = ptr_cast<Blob>(ptr_null()); $b->data = "x";"#, "Pointer(Some(\"u8\"))");
    expect_error("<?php function f(): ptr<u8> { return; }", "Pointer(Some(\"u8\"))");
}

/// A namespaced cast does not fall back to a same-named global class.
#[test]
fn test_error_ptr_cast_width_name_does_not_fallback_to_global_class() {
    expect_error(r"<?php namespace { packed class u8 { public i32 $value; } } namespace App { function f(ptr $p): int { return ptr_cast<u8>($p)->value; } }", r"Unknown ptr_cast target type: App\u8");
}

/// Builtin diagnostics do not expose a pointer's internal width marker.
#[test]
fn test_error_pointer_width_tag_not_exposed_by_builtin() {
    expect_error("<?php function f(ptr<u8> $p): void { pcntl_exec($p); }", "Pointer(Some(\"u8\"))");
}
