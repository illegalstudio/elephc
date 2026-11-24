//! Purpose:
//! Regression tests for FFI review findings: CType query diagnostics, string operands,
//! signed size/dimension validation, callback return conversion, pointer warnings, place
//! fallbacks, and temporary-address lifetime.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests_runtime` binary's FFI module.
//!
//! Key details:
//! - Fixtures compile inline PHP and compare stdout. Deprecation warnings from static
//!   `FFI::new()`/`cast()` calls are avoided by using an FFI context instance.

use super::*;

/// Verifies `CType` queries on the wrong kind throw php-src's exact diagnostics.
#[test]
fn test_ffi_ctype_queries_reject_wrong_kinds() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$type = $ffi->type('int*');
foreach (['getArrayLength', 'getArrayElementType', 'getStructFieldNames', 'getFuncABI', 'getFuncReturnType', 'getEnumKind'] as $method) {
    try { $type->$method(); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
}
try { $type->getFuncParameterType(0); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "FFI\\CType is not an array\n\
FFI\\CType is not an array\n\
FFI\\CType is not a structure\n\
FFI\\CType is not a function\n\
FFI\\CType is not a function\n\
FFI\\CType is not an enumeration\n\
FFI\\CType is not a function\n"
    );
}

/// Verifies function-pointer fields reject signatures that differ from their declaration.
#[test]
fn test_ffi_function_pointer_fields_enforce_signatures() {
    let out = compile_and_run(
        r#"<?php
$compatible = FFI::cdef('struct G { int (*fn)(int); }; int abs(int);');
$record = $compatible->new('struct G');
$record->fn = $compatible->abs;
var_dump(call_user_func($record->fn, -5));
$incompatible = FFI::cdef('struct F { double (*fn)(double); }; int abs(int);');
$other = $incompatible->new('struct F');
try { $other->fn = $incompatible->abs; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "int(5)\nIncompatible types when assigning to type 'double(*)()' from type 'int32_t(*)()'\n"
    );
}

/// Verifies integer callbacks do not apply the `char` one-byte-string rule.
#[test]
fn test_ffi_callback_return_conversion_follows_c_type() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct C { char (*ccb)(void); int (*icb)(void); };');
$record = $ffi->new('struct C');
$record->icb = fn() => '9';
var_dump(call_user_func($record->icb));
$record->ccb = fn() => '9';
var_dump(call_user_func($record->ccb));
"#,
    );
    assert_eq!(out, "int(9)\nstring(1) \"9\"\n");
}

/// Verifies pointer arithmetic through a CData operand emits PHP's object-to-int warning.
#[test]
fn test_ffi_pointer_arithmetic_warns_on_cdata_conversion() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $level, string $message): bool {
    if ($level === E_WARNING) { echo $message, "\n"; }
    return true;
});
$ffi = FFI::cdef('');
$array = $ffi->new('int[2]');
$other = $ffi->new('char[2]');
$scalar = $ffi->new('int');
$first = $array + $array;
echo "add\n";
$second = $array - $other;
echo "sub\n";
$third = $array - $scalar;
echo "subscalar\n";
"#,
    );
    assert_eq!(
        out,
        "Object of class FFI\\CData could not be converted to int\nadd\n\
Object of class FFI\\CData could not be converted to int\nsub\n\
Object of class FFI\\CData could not be converted to int\nsubscalar\n"
    );
}

/// Verifies place-view rewrites fall back to ordinary PHP property and offset reads.
#[test]
fn test_ffi_place_views_fall_back_to_php_reads() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
function read_property(mixed $value): string { return FFI::string($value->text); }
function read_dynamic(mixed $value, string $name): string { return FFI::string($value->{$name}); }
function read_offset(mixed $value): int { return FFI::sizeof($value[0]); }
function read_text(mixed $value): string { return FFI::string($value[1]); }
$holder = new stdClass();
$holder->text = 'abcd';
var_dump(read_property($holder));
var_dump(read_dynamic($holder, 'text'));
$items = [$ffi->new('int'), 'wxyz'];
var_dump(read_offset($items));
var_dump(read_text($items));
"#,
    );
    assert_eq!(out, "string(4) \"abcd\"\nstring(4) \"abcd\"\nint(4)\nstring(4) \"wxyz\"\n");
}

/// Verifies `FFI::addr()` retains a helper result's storage instead of leaving it dangling.
#[test]
fn test_ffi_addr_keeps_a_returned_temporary_alive() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$makeData = fn(): FFI\CData => $ffi->new('int');
$pointer = FFI::addr($makeData());
$pointer[0] = 42;
for ($index = 0; $index < 64; $index++) {
    $temporary = $ffi->new('int[2]');
    $temporary[0] = $index;
}
var_dump($pointer[0]);
"#,
    );
    assert_eq!(out, "int(42)\n");
}

/// Verifies `is_callable()` accepts a CData function-pointer property and it remains callable.
#[test]
fn test_ffi_is_callable_property_read() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('int abs(int);');
var_dump(is_callable($ffi->abs));
var_dump($ffi->abs(-5));
"#,
    );
    assert_eq!(out, "bool(true)\nint(5)\n");
}

/// Verifies nested `foreach` loops over the same CData array keep independent positions.
#[test]
fn test_ffi_nested_foreach_uses_independent_positions() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$array = $ffi->new('int[2]');
$array[0] = 1;
$array[1] = 2;
foreach ($array as $outer) {
    foreach ($array as $inner) {
        echo $outer, $inner, "\n";
    }
}
"#,
    );
    assert_eq!(out, "11\n12\n21\n22\n");
}

/// Verifies `memcmp()` accepts a string for either operand and enforces string bounds.
#[test]
fn test_ffi_memcmp_accepts_strings_in_either_position() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
var_dump(FFI::memcmp('a', 'b', 1));
var_dump(FFI::memcmp('b', 'a', 1));
var_dump(FFI::memcmp('a', 'a', 1));
$buffer = $ffi->new('char[2]');
$buffer[0] = 'a';
$buffer[1] = "\0";
$pointer = $ffi->cast('char*', $buffer);
var_dump(FFI::memcmp('a', $pointer, 1));
var_dump(FFI::memcmp($pointer, 'a', 1));
try { FFI::memcmp('a', 'b', 2); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
var_dump(FFI::memcmp('a', 'b', 1));
"#,
    );
    assert_eq!(
        out,
        "int(-1)\nint(1)\nint(0)\nint(0)\nint(0)\nattempt to read over string boundary\nint(-1)\n"
    );
}

/// Verifies the four memory APIs reject negative sizes before touching native memory.
#[test]
fn test_ffi_memory_operations_reject_negative_sizes() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$pointer = $ffi->cast('char*', $ffi->new('char[4]'));
foreach (['memset', 'memcpy', 'memcmp', 'string'] as $method) {
    try {
        if ($method === 'memset') { FFI::memset($pointer, 0, -1); }
        elseif ($method === 'memcpy') { FFI::memcpy($pointer, $pointer, -1); }
        elseif ($method === 'memcmp') { FFI::memcmp($pointer, $pointer, -1); }
        else { FFI::string($pointer, -1); }
    } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
}
FFI::memset($pointer, 65, 1);
var_dump(FFI::string($pointer, 1));
"#,
    );
    assert_eq!(
        out,
        "FFI::memset(): Argument #3 ($size) must be greater than or equal to 0\n\
FFI::memcpy(): Argument #3 ($size) must be greater than or equal to 0\n\
FFI::memcmp(): Argument #3 ($size) must be greater than or equal to 0\n\
FFI::string(): Argument #2 ($size) must be greater than or equal to 0\n\
string(1) \"A\"\n"
    );
}

/// Verifies `FFI::arrayType()` rejects negative dimensions instead of creating `usize::MAX`.
#[test]
fn test_ffi_array_type_rejects_negative_dimensions() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
try { FFI::arrayType($ffi->type('char'), [-1]); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
$valid = FFI::arrayType($ffi->type('char'), [2]);
var_dump($valid->getArrayLength());
"#,
    );
    assert_eq!(out, "negative array index\nint(2)\n");
}

/// Verifies a CData function-pointer argument must match the declared callback signature.
#[test]
fn test_ffi_call_rejects_incompatible_function_pointer_arguments() {
    let out = compile_and_run(
        r#"<?php
$libc = FFI::cdef('
    typedef int (*compar_fn)(const void *, const void *);
    void qsort(void *base, size_t nmemb, size_t size, compar_fn compar);
    int abs(int);
');
$values = $libc->new('int[2]');
foreach ([$libc->new('int'), $libc->abs] as $callback) {
    try {
        $libc->qsort($values, 2, 4, $callback);
        echo "accepted\n";
    } catch (FFI\Exception $error) {
        echo $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "Passing incompatible argument 4 of C function 'qsort', expecting 'int32_t(*)()', found PHP 'FFI\\CData'\n\
Passing incompatible argument 4 of C function 'qsort', expecting 'int32_t(*)()', found PHP 'FFI\\CData'\n"
    );
}

/// Verifies over-aligned C types are allocated with their declared alignment.
#[test]
fn test_ffi_over_aligned_cdata_allocation() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct __attribute__((aligned(64))) aligned_record { int a; };');
$type = $ffi->type('struct aligned_record');
var_dump($type->getAlignment());
$value = $ffi->new('struct aligned_record');
$value->a = 7;
var_dump($value->a);
"#,
    );
    assert_eq!(out, "int(64)\nint(7)\n");
}

/// Verifies `getAttributes()` exposes ATTR_CONST where php-src records the qualifier.
#[test]
fn test_ffi_const_attributes_follow_php() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct S { int i; };');
var_dump($ffi->type('const int')->getAttributes());
var_dump($ffi->type('const int *')->getAttributes() & FFI\CType::ATTR_CONST);
var_dump($ffi->type('int * const')->getAttributes() & FFI\CType::ATTR_CONST);
var_dump($ffi->type('const int[2]')->getAttributes() & FFI\CType::ATTR_CONST);
var_dump($ffi->type('const int **')->getPointerType()->getAttributes() & FFI\CType::ATTR_CONST);
var_dump($ffi->type('const struct S *')->getAttributes() & FFI\CType::ATTR_CONST);
"#,
    );
    assert_eq!(out, "int(0)\nint(1)\nint(0)\nint(1)\nint(1)\nint(1)\n");
}

/// Verifies named arguments only rewrite parameters that actually need a CData place view.
#[test]
fn test_ffi_named_arguments_only_rewrite_place_parameters() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct S { int fill; int count; };');
$state = $ffi->new('struct S');
$buffer = $ffi->new('char[4]');
$state->fill = 65;
FFI::memset($buffer, value: $state->fill, size: 4);
var_dump($buffer[0]);
$state->fill = 66;
FFI::memset(ptr: $buffer, value: $state->fill, size: 1);
var_dump($buffer[0]);
$buffer[0] = 'x';
$buffer[1] = 'x';
$buffer[2] = 'x';
$state->count = 2;
FFI::memset($buffer, 67, size: $state->count);
var_dump($buffer[0], $buffer[1], $buffer[2]);
"#,
    );
    assert_eq!(
        out,
        "string(1) \"A\"\nstring(1) \"B\"\nstring(1) \"C\"\nstring(1) \"C\"\nstring(1) \"x\"\n"
    );
}

/// Verifies `signed char`/`unsigned char` are integers while bare `char` stays a character.
#[test]
fn test_ffi_signed_and_unsigned_char_are_integers() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('typedef unsigned char byte; struct B { signed char a; unsigned char b; };');
foreach (['char', 'signed char', 'unsigned char', 'byte'] as $name) {
    $type = $ffi->type($name);
    echo $name, ':', $type->getKind(), ':', $type->getName(), ':';
    $value = $ffi->new($name);
    if ($name === 'char') { $value->cdata = 'A'; } else { $value->cdata = 255; }
    var_dump($value->cdata);
}
$record = $ffi->new('struct B');
$record->a = 255;
$record->b = 255;
var_dump($record->a, $record->b);
"#,
    );
    assert_eq!(
        out,
        "char:14:char:string(1) \"A\"\n\
signed char:5:int8_t:int(-1)\n\
unsigned char:4:uint8_t:int(255)\n\
byte:4:uint8_t:int(255)\n\
int(-1)\nint(255)\n"
    );
}

/// Verifies CData scalar assignments read the source type and convert instead of storing zero.
#[test]
fn test_ffi_cdata_scalar_assignments_convert_by_source_type() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct S { int i; };');
$double = $ffi->new('double');
$double->cdata = 3.7;
$int = $ffi->new('int');
$int->cdata = $double;
var_dump($int->cdata);
$record = $ffi->new('struct S');
$record->i = $double;
var_dump($record->i);
$array = $ffi->new('int[1]');
$array[0] = $double;
var_dump($array[0]);
$float = $ffi->new('float');
$float->cdata = $int;
var_dump($float->cdata);
$bool = $ffi->new('bool');
$bool->cdata = $double;
var_dump($bool->cdata);
try { $char = $ffi->new('char'); $char->cdata = $double; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
try { $int->cdata = $ffi->new('int[1]'); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "int(3)\nint(3)\nint(3)\nfloat(3)\nbool(true)\n\
Incompatible types when assigning to type 'char' from type 'double'\n\
Incompatible types when assigning to type 'int32_t' from type 'int32_t[1]'\n"
    );
}

/// Verifies pointer parameters reject bool/scalar/aggregate CData instead of passing an address.
#[test]
fn test_ffi_pointer_arguments_reject_incompatible_values() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('size_t strlen(const char *s); struct S { int i; };');
try {
    $ffi->strlen(true);
    echo "bool accepted\n";
} catch (FFI\Exception $error) {
    echo str_contains($error->getMessage(), 'Passing incompatible argument 1') ? "rejected" : $error->getMessage(), "\n";
}
$record = $ffi->new('struct S');
try {
    $ffi->strlen($record);
    echo "struct accepted\n";
} catch (FFI\Exception $error) {
    echo str_contains($error->getMessage(), 'Passing incompatible argument 1') ? "rejected" : $error->getMessage(), "\n";
}
$buffer = $ffi->new('char[2]');
$buffer[0] = 'a';
$buffer[1] = "\0";
$pointer = $ffi->cast('char*', $buffer);
var_dump($ffi->strlen($pointer));
"#,
    );
    assert_eq!(out, "rejected\nrejected\nint(1)\n");
}

/// Verifies NULL and freed CData pointers reject arithmetic instead of manufacturing addresses.
#[test]
fn test_ffi_null_and_freed_pointer_arithmetic_throws() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$null = $ffi->new('int*');
try { $result = $null + 1; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
try { $result = $null - 1; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
try { $result = $null - $null; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "NULL pointer dereference\nNULL pointer dereference\nNULL pointer dereference\n"
    );
}

/// Verifies a NULL `char *` return is PHP null rather than an empty string.
#[test]
fn test_ffi_null_char_pointer_return_is_php_null() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('char *getenv(const char *name);');
var_dump($ffi->getenv('ELEPHC_FFI_MISSING_ENV_VAR_12345'));
"#,
    );
    assert_eq!(out, "NULL\n");
}

/// Verifies `is_callable()` on an FFI property defers to the runtime value: a scalar global is
/// not callable, while a function-pointer CData is.
#[test]
fn test_ffi_is_callable_scalar_global_and_function_pointer() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('extern int optind; int abs(int);');
var_dump(is_callable([$ffi->optind, "anything"]));
var_dump(is_callable($ffi->optind));
var_dump(is_callable($ffi->abs));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nbool(true)\n");
}

/// Verifies `FFI::addr()` keeps a live owner and an element view alive independently.
#[test]
fn test_ffi_addr_view_lifetime_keeps_owner_alive() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$owner = $ffi->new('int[1]', true);
$view = FFI::addr($owner[0]);
unset($view);
$owner[0] = 7;
echo $owner[0], "\n";
$element = FFI::addr($owner[0]);
$owner[0] = 8;
echo $element[0], "\n";
unset($owner);
echo $element[0], "\n";
"#,
    );
    assert_eq!(out, "7\n8\n8\n");
}

/// Verifies a CData passed through a declared `object` parameter still raises php-src's
/// method-call Error instead of running a prelude method such as `count()`.
#[test]
fn test_ffi_object_receiver_still_rejects_cdata_methods() {
    let out = compile_and_run(
        r#"<?php
class ObjectReadBox {
    public function count(): int { return 5; }
}
function call_object(object $value): mixed {
    return $value->count();
}
$ffi = FFI::cdef('');
$cdata = $ffi->new('int[2]');
try { var_dump(call_object($cdata)); } catch (Error $error) { echo $error->getMessage(), "\n"; }
var_dump(call_object(new ObjectReadBox()));
"#,
    );
    assert_eq!(
        out,
        "Object of type FFI\\CData does not support method calls\nint(5)\n"
    );
}

/// Verifies a function-pointer CData stored in a variable is callable directly when its
/// declared receiver type is `FFI\CData`.
#[test]
fn test_ffi_cdata_function_pointer_variable_invokes() {
    let out = compile_and_run(
        r#"<?php
function invoke_cdata(\FFI\CData $callback, int $value): mixed {
    return $callback($value);
}
$ffi = FFI::cdef('int abs(int);');
$fn = $ffi->abs;
var_dump(is_callable($fn));
var_dump(invoke_cdata($fn, -3));
"#,
    );
    assert_eq!(out, "bool(true)\nint(3)\n");
}

/// Verifies callable strings fill C function-pointer fields and scalar CData through the
/// same libffi registration path as closures.
#[test]
fn test_ffi_callable_strings_fill_function_pointers() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct C { int (*fn)(int); }; int abs(int);');
$record = $ffi->new('struct C');
$record->fn = 'abs';
var_dump(call_user_func($record->fn, -3));
$scalar = $ffi->new('int(*)(int)');
$scalar->cdata = 'abs';
var_dump($scalar(-3));
"#,
    );
    assert_eq!(out, "int(3)\nint(3)\n");
}

/// Verifies a `long double` CData scalar and struct field round-trip through the target
/// encoding: IEEE double on Apple, x87 80-bit or binary128 on Linux.
#[test]
fn test_ffi_long_double_cdata_values() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct LD { long double x; };');
$scalar = $ffi->new('long double');
$scalar->cdata = 2.5;
var_dump($scalar->cdata);
$record = $ffi->new('struct LD');
$record->x = -1.25;
var_dump($record->x);
"#,
    );
    assert_eq!(out, "float(2.5)\nfloat(-1.25)\n");
}

/// Verifies `long double` parameters, returns, and callbacks use the target ABI while PHP
/// still sees ordinary `float` values.
#[test]
fn test_ffi_long_double_calls_and_callbacks() {
    let out = compile_and_run(
        r#"<?php
$libc = FFI::cdef('long double strtold(const char *nptr, char **endptr);');
var_dump($libc->strtold("2.5", null));
$ffi = FFI::cdef('typedef long double (*ld_cb)(long double);');
$callback = $ffi->new('ld_cb');
$callback->cdata = fn(float $value): float => $value * 2.0;
var_dump($callback(1.25));
"#,
    );
    assert_eq!(out, "float(2.5)\nfloat(2.5)\n");
}
