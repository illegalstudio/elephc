//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of FFI syntax and callbacks, including FFI extern block syntax, FFI extern lib function syntax, and FFI extern void return.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies extern block declares multiple FFI extern functions and they are callable from PHP.
#[test]
fn test_ffi_extern_block_syntax() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function abs(int $n): int;
    function atoi(string $s): int;
}
echo abs(-7) . "," . atoi("99");
"#,
    );
    assert_eq!(out, "7,99");
}

/// Verifies single-line `extern "System" function` syntax declares an FFI extern callable from PHP.
#[test]
fn test_ffi_extern_lib_function_syntax() {
    let out = compile_and_run(
        r#"<?php
extern "System" function abs(int $n): int;
echo abs(-1);
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies FFI extern function with `void` return type and `int` argument is compiled correctly.
#[test]
fn test_ffi_extern_void_return() {
    let out = compile_and_run(
        r#"<?php
extern function abs(i32 $n): i32;
$x = abs(-5);
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies FFI extern function with `float` argument and `float` return type is compiled correctly.
#[test]
fn test_ffi_extern_float_arg_and_return() {
    let out = compile_and_run(
        r#"<?php
extern function sqrt(float $x): float;
echo sqrt(144.0);
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies FFI extern function with `ptr` and `int` mixed arguments is compiled correctly.
#[test]
fn test_ffi_extern_multiple_args() {
    let out = compile_and_run(
        r#"<?php
extern function strtol(string $s, ptr $endptr, i32 $base): int;
echo strtol("FF", ptr_null(), 16);
"#,
    );
    assert_eq!(out, "255");
}

/// Verifies FFI extern function with two `string` arguments is compiled correctly.
#[test]
fn test_ffi_extern_multiple_string_args() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): i32;
echo strcmp("aa", "ab") < 0 ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that borrowed C-string temporaries from two FFI extern calls with `string` args are freed without leaking.
#[test]
fn test_ffi_extern_multiple_string_args_free_all_borrowed_cstr_temps() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
extern function strcmp(string $left, string $right): i32;
"#,
    );
    assert!(
        baseline.success,
        "baseline program failed: {}",
        baseline.stderr
    );
    let out = compile_and_run_with_gc_stats(
        r#"<?php
extern function strcmp(string $left, string $right): i32;
strcmp("aa", "ab");
strcmp("bb", "bb");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(
        allocs - baseline_allocs,
        frees - baseline_frees,
        "{}",
        out.stderr
    );
}

/// Verifies `extern global ptr $environ` declares an FFI external global pointer variable.
#[test]
fn test_ffi_extern_global() {
    let out = compile_and_run(
        r#"<?php
extern global ptr $environ;
echo ptr_is_null($environ) ? "fail" : "ok";
"#,
    );
    assert_eq!(out, "ok");
}

// Verifies FFI extern `signal` and `raise` are callable and a PHP function can be used as a signal handler.
/// Verifies extern callback string literals resolve function names case-insensitively.
#[test]
fn test_ffi_callback_signal_handler() {
    let out = compile_and_run(
        r#"<?php
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;

function on_signal($sig) {
    echo $sig;
}

signal(15, "ON_SIGNAL");
raise(15);
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies FFI `callable` parameters accept environment-free first-class callable descriptors.
#[test]
fn test_ffi_callback_signal_handler_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;

function on_signal(int $sig): void {
    echo $sig + 1;
}

$handler = on_signal(...);
signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "16");
}

/// Verifies FFI `callable` parameters accept closure descriptors without captures.
#[test]
fn test_ffi_callback_signal_handler_closure_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;

$handler = function(int $sig): void {
    echo $sig + 2;
};

signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "17");
}

/// Verifies FFI `callable` parameters preserve closure capture environments through trampolines.
#[test]
fn test_ffi_callback_signal_handler_closure_capture_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;

$delta = 3;
$handler = function(int $sig) use ($delta): void {
    echo $sig + $delta;
};

signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "18");
}

/// Verifies FFI callback trampolines preserve first-class method receiver environments.
#[test]
fn test_ffi_callback_signal_handler_first_class_method_receiver_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;

class Handler {
    public int $delta;

    public function __construct(int $delta) {
        $this->delta = $delta;
    }

    public function onSignal(int $sig): void {
        echo $sig + $this->delta;
    }
}

$handler = (new Handler(4))->onSignal(...);
signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "19");
}

/// Verifies FFI callback trampolines preserve branch-selected descriptor environments.
#[test]
fn test_ffi_callback_signal_handler_branch_selected_descriptor() {
    let out = compile_and_run(
        r#"<?php
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;

$delta = 5;
$handler = true
    ? function(int $sig) use ($delta): void { echo $sig + $delta; }
    : function(int $sig): void { echo $sig; };

signal(15, $handler);
raise(15);
"#,
    );
    assert_eq!(out, "20");
}

/// Smoke test: verifies non-string FFI extern function returning a C `int` (`i32`) is callable and returns a positive pid.
#[test]
fn test_ffi_extern_non_string_global_smoke() {
    let out = compile_and_run(
        r#"<?php
extern function getpid(): i32;
$pid = getpid();
echo $pid > 0 ? "ok" : "fail";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies FFI extern function declared inside a PHP function scope is callable and resolves correctly.
#[test]
fn test_ffi_extern_in_function() {
    let out = compile_and_run(
        r#"<?php
extern function abs(i32 $n): i32;
function my_abs($x) {
    return abs($x);
}
echo my_abs(-10);
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies program exit releases the descriptor each extern callback slot retains, so a
/// captured object is destroyed and the heap report is clean.
#[test]
fn test_ffi_callback_slot_descriptors_released_at_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

class Probe {
    public function __construct(public int $delta) {}
    public function __destruct() { echo "[gone {$this->delta}]"; }
}

function plain(int $sig): void { echo $sig, ";"; }

for ($i = 0; $i < 3; $i++) {
    $probe = new Probe($i);
    signal(15, function(int $sig) use ($probe): void { echo $sig + $probe->delta, ";"; });
    raise(15);
}
unset($probe);
signal(10, "plain");
raise(10);
echo "end";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "15;[gone 0]16;[gone 1]17;10;end[gone 2]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a direct CData `__destruct()` call raises php-src's method-call Error and
/// leaves the value intact.
#[test]
fn test_ffi_cdata_destructor_is_private() {
    let out = compile_and_run(
        r#"<?php
$value = FFI::new("int");
$value->cdata = 7;
try {
    $value->__destruct();
    echo "called";
} catch (\Error $error) {
    echo get_class($error), ": ", $error->getMessage();
}
var_dump($value->cdata);
"#,
    );
    assert!(
        out.contains("Object of type FFI\\CData does not support method calls"),
        "{out}"
    );
    assert!(out.contains("int(7)"), "{out}");
}

/// Verifies a scalar cannot be written into a C function-pointer slot.
#[test]
fn test_ffi_function_pointer_rejects_scalar_assignment() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef("typedef void (*cb_t)(void); struct S { cb_t cb; };");
$value = $ffi->new("struct S");
try {
    $value->cb = 12345;
    echo "assigned";
} catch (\FFI\Exception $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Attempt to assign an invalid callback, no array or string given"
    );
}

/// Verifies a closure stored in a struct function-pointer field can be called by C.
#[test]
fn test_ffi_struct_field_callback_runs_from_c() {
    let out = compile_and_run(
        r#"<?php
$libc = FFI::cdef("
    typedef int (*compar_fn)(const void *, const void *);
    struct Holder { compar_fn cb; };
    void qsort(void *base, size_t nmemb, size_t size, compar_fn compar);
");
$holder = $libc->new("struct Holder");
$holder->cb = function (\FFI\CData $left, \FFI\CData $right): int {
    $a = FFI::cast("int*", $left)[0];
    $b = FFI::cast("int*", $right)[0];
    if ($a < $b) { return -1; }
    if ($a > $b) { return 1; }
    return 0;
};
$values = $libc->new("int[3]");
$values[0] = 3;
$values[1] = 1;
$values[2] = 2;
$libc->qsort($values, 3, 4, $holder->cb);
echo $values[0], $values[1], $values[2], "\n";
"#,
    );
    assert_eq!(out.trim_end(), "123");
}

/// Verifies a caught use-after-free exception leaves the var_dump walker reusable.
#[test]
fn test_ffi_freed_cdata_debuginfo_can_be_dumped_twice() {
    let out = compile_and_run_capture(
        r#"<?php
$value = FFI::new("uint16_t[2]", false);
FFI::free($value);
try { var_dump($value); } catch (\Throwable $error) { echo "caught1\n"; }
try { var_dump($value); } catch (\Throwable $error) { echo "caught2\n"; }
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout.matches("object(FFI\\CData:uint16_t[2])").count(), 2);
    assert!(out.stdout.contains("caught1\nobject(FFI\\CData:uint16_t[2])"), "{}", out.stdout);
    assert!(out.stdout.contains("caught2"), "{}", out.stdout);
    assert!(!out.stdout.contains("*RECURSION*"), "{}", out.stdout);
}

/// Verifies the P2 php-src parity fixes: scalar CData arithmetic rejects operands like PHP,
/// pointer arithmetic/difference still work, scalar arrays iterate as PHP scalars (while
/// by-reference loops keep writable CData views), string assignment follows php-src's
/// char/char[] diagnostics and numeric coercions, and `is_callable` distinguishes a function
/// pointer from other CData.
#[test]
fn test_ffi_scalar_parity_fixes() {
    let out = compile_and_run(
        r#"<?php
$i = FFI::new("int");
$i->cdata = 5;
echo $i->cdata, "\n";
foreach ([fn() => $i + 3, fn() => $i * 2, fn() => $i / 2] as $operation) {
    try { $operation(); } catch (\TypeError $error) { echo $error->getMessage(), "\n"; }
}
$p = FFI::new("int[3]");
$p[1] = 10;
echo get_class($p + 1), "\n";
echo $p - $p, "\n";
foreach ($p as $key => $value) {
    echo $key, "=", $value, "\n";
}
$by_ref = FFI::new("int[2]");
foreach ($by_ref as &$value) {
    $value->cdata += 3;
}
echo $by_ref[0], $by_ref[1], "\n";
$record = FFI::new("struct { char c; char d[4]; int i; double g; }");
try { $record->c = "ab"; } catch (\FFI\Exception $error) { echo $error->getMessage(), "\n"; }
try { $record->d = "ab"; } catch (\FFI\Exception $error) { echo $error->getMessage(), "\n"; }
$record->c = "a";
$record->i = "12";
$record->g = "1.5";
echo $record->c, $record->i, ":", $record->g, "\n";
$libc = FFI::cdef("int (*abs)(int);");
$function = $libc->abs;
var_dump(is_callable($function), is_callable($i));
"#,
    );
    assert_eq!(
        out.trim_end(),
        "5\n\
Unsupported operand types: FFI\\CData + int\n\
Unsupported operand types: FFI\\CData * int\n\
Unsupported operand types: FFI\\CData / int\n\
FFI\\CData\n\
0\n\
0=0\n\
1=10\n\
2=0\n\
33\n\
Incompatible types when assigning to type 'char' from PHP 'string'\n\
Incompatible types when assigning to type 'char[4]' from PHP 'string'\n\
a12:1.5\n\
bool(true)\n\
bool(false)"
    );
}

/// Verifies the PHP 8.5 static `FFI::new()` deprecation is emitted for the profile that
/// declares it and stays silent on earlier profiles.
#[test]
fn test_ffi_static_new_deprecation_follows_php_version() {
    let source = r#"<?php FFI::new("int");"#;
    let php85 = compile_and_run_capture_merged_with_php_version(
        source,
        elephc::php_version::PhpVersion::Php85,
    );
    assert!(php85.success, "{}", php85.stdout);
    assert!(
        php85.stdout.contains("Deprecated: Calling FFI::new() statically is deprecated"),
        "{}",
        php85.stdout
    );
    let php84 = compile_and_run_capture_merged_with_php_version(
        source,
        elephc::php_version::PhpVersion::Php84,
    );
    assert!(php84.success, "{}", php84.stdout);
    assert!(
        !php84.stdout.contains("Calling FFI::new() statically is deprecated"),
        "{}",
        php84.stdout
    );
}

/// Verifies CData's PHP-visible interface and method surface matches php-src's internal
/// handlers: no ArrayAccess/Countable/Iterator/Stringable and no public methods, while array
/// access, count, and iteration still work.
#[test]
fn test_ffi_cdata_reflection_matches_php_src_internal_handlers() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_implements(FFI\CData::class));
var_dump(get_class_methods(FFI\CData::class));
$value = FFI::new("int");
var_dump(
    $value instanceof ArrayAccess,
    $value instanceof Countable,
    $value instanceof Iterator,
    $value instanceof Traversable,
    $value instanceof Stringable
);
var_dump(is_a($value, ArrayAccess::class));
var_dump(method_exists($value, "offsetGet"));
$array = FFI::new("int[2]");
$array[0] = 7;
$array[1] = 8;
var_dump($array[0], count($array));
foreach ($array as $element) { echo $element; }
echo "\n";
"#,
    );
    assert_eq!(
        out.trim_end(),
        "array(0) {\n}\n\
array(0) {\n}\n\
bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n\
bool(false)\n\
bool(false)\n\
int(7)\nint(2)\n\
78"
    );
}

/// Verifies `is_callable()` on a CData boxed in a `mixed` slot uses the bridge's declared
/// C-type check: a scalar CData is not callable, a function pointer is, and the generic
/// string/closure/array callable path keeps working.
#[test]
fn test_ffi_mixed_is_callable_distinguishes_cdata_types() {
    let out = compile_and_run(
        r#"<?php
function probe(mixed $value): bool { return is_callable($value); }
$libc = FFI::cdef("int (*abs)(int);");
$scalar = FFI::new("int");
$function = $libc->abs;
var_dump(probe($scalar), probe($function));
var_dump(probe("strlen"), probe(fn() => 1));
"#,
    );
    assert_eq!(
        out.trim_end(),
        "bool(false)\nbool(true)\nbool(true)\nbool(true)"
    );
}

/// Verifies CData comparison, cast and iterable surfaces match php-src's internal handlers:
/// incompatible comparisons throw, object-to-int/float casts warn and yield 1, pointer/array
/// string casts throw, and CData is not Traversable.
#[test]
fn test_ffi_comparison_cast_and_iterable_surfaces() {
    let out = compile_and_run_capture_merged_with_php_version(
        r#"<?php
$i = FFI::new("int");
$i->cdata = 5;
$j = FFI::new("int");
$j->cdata = 7;
$p = FFI::new("int[2]");
$p[0] = 1;
$p[1] = 2;
foreach ([fn() => $i == $j, fn() => $i < $j, fn() => $p < ($p + 1)] as $compare) {
    try { var_dump($compare()); } catch (\FFI\Exception $error) { echo $error->getMessage(), "\n"; }
}
var_dump($p == $p);
var_dump((int) $i);
var_dump((float) $i);
var_dump((string) $i);
try { var_dump((string) $p); } catch (\Error $error) { echo $error->getMessage(), "\n"; }
var_dump(is_iterable($p));
try { iterator_to_array($p); } catch (\TypeError $error) { echo $error->getMessage(), "\n"; }
"#,
        elephc::php_version::PhpVersion::Php84,
    );
    assert!(out.success, "{}", out.stdout);
    assert_eq!(
        out.stdout.trim_end(),
        "Comparison of incompatible C types\n\
Comparison of incompatible C types\n\
Comparison of incompatible C types\n\
bool(true)\n\
Warning: Object of class FFI\\CData could not be converted to int\n\
int(1)\n\
Warning: Object of class FFI\\CData could not be converted to float\n\
float(1)\n\
string(1) \"5\"\n\
Object of class FFI\\CData could not be converted to string\n\
bool(false)\n\
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, FFI\\CData given"
    );
}

/// Verifies a PHP closure installed into a C function pointer can return a struct by value
/// through the libffi closure trampoline (php-src supports aggregate callback returns).
#[test]
fn test_ffi_callback_struct_by_value_return() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef("
typedef struct { uint64_t b; double c; } S;
typedef S (*cb_t)(void);
extern cb_t elephc_ffi_test_cb;
S elephc_ffi_test_struct_callback(cb_t cb);
");
$ffi->elephc_ffi_test_cb = function () use ($ffi): \FFI\CData {
    $result = $ffi->new("S");
    $result->b = 42;
    $result->c = 1.5;
    return $result;
};
$value = $ffi->elephc_ffi_test_struct_callback($ffi->elephc_ffi_test_cb);
var_dump($value->b, $value->c);
"#,
    );
    assert_eq!(out.trim_end(), "int(42)\nfloat(1.5)");
}

/// Verifies a variadic function pointer rejects a PHP closure exactly like php-src.
#[test]
fn test_ffi_variadic_callback_is_rejected() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef("struct S { void (*cb)(int, ...); };");
$value = $ffi->new("struct S");
try {
    $value->cb = function (int $first, ...$rest) {};
    echo "assigned";
} catch (\FFI\Exception $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(out, "Variadic function closures are not supported");
}

/// Verifies `--ini ffi.enable=0` bakes php-src's API restriction into every guarded entry
/// point, including the exact exception type and message.
#[test]
fn test_ffi_enable_directive_disables_api() {
    /// Clears the compile-time INI overrides even when the assertion fails.
    struct IniRestore;
    impl Drop for IniRestore {
        fn drop(&mut self) {
            elephc::codegen::set_ini_overrides(Vec::new());
        }
    }

    let _restore = IniRestore;
    elephc::codegen::set_ini_overrides(vec![(
        "ffi.enable".to_string(),
        "0".to_string(),
    )]);
    let out = compile_and_run(
        r#"<?php
try {
    FFI::cdef("int abs(int);");
    echo "enabled";
} catch (\FFI\Exception $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "FFI API is restricted by \"ffi.enable\" configuration directive"
    );
}

/// Verifies a PHP closure passed directly to a C function-pointer parameter becomes a
/// temporary libffi callback, and that a string callable takes the same path.
#[test]
fn test_ffi_callable_as_function_pointer_argument() {
    let out = compile_and_run(
        r#"<?php
function php_ascending(\FFI\CData $left, \FFI\CData $right): int {
    $a = FFI::cast("int*", $left)[0];
    $b = FFI::cast("int*", $right)[0];
    return $a <=> $b;
}
$libc = FFI::cdef("
    typedef int (*compar_fn)(const void *, const void *);
    void qsort(void *base, size_t nmemb, size_t size, compar_fn compar);
");
$values = $libc->new("int[4]");
$values[0] = 3;
$values[1] = 1;
$values[2] = 4;
$values[3] = 2;
$libc->qsort($values, 4, 4, function (\FFI\CData $left, \FFI\CData $right): int {
    $a = FFI::cast("int*", $left)[0];
    $b = FFI::cast("int*", $right)[0];
    return $b <=> $a;
});
echo $values[0], $values[1], $values[2], $values[3], "\n";
$libc->qsort($values, 4, 4, "php_ascending");
echo $values[0], $values[1], $values[2], $values[3], "\n";
"#,
    );
    assert_eq!(out.trim_end(), "4321\n1234");
}

/// Verifies a non-callable value for a function-pointer parameter reports php-src's exact
/// invalid-callback diagnostics instead of leaking an address into C.
#[test]
fn test_ffi_invalid_function_pointer_argument_is_rejected() {
    let out = compile_and_run(
        r#"<?php
$libc = FFI::cdef("
    typedef int (*compar_fn)(const void *, const void *);
    void qsort(void *base, size_t nmemb, size_t size, compar_fn compar);
");
$values = $libc->new("int[2]");
foreach ([42, "no_such_function"] as $callback) {
    try {
        $libc->qsort($values, 2, 4, $callback);
        echo "accepted\n";
    } catch (\FFI\Exception $error) {
        echo $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out.trim_end(),
        "Attempt to assign an invalid callback, no array or string given\nAttempt to assign an invalid callback, function \"no_such_function\" not found or invalid function name"
    );
}

/// Verifies an array callable whose receiver is CData raises php-src's method-call Error
/// during `is_callable()` instead of silently reporting false.
#[test]
fn test_ffi_is_callable_array_with_cdata_receiver_throws() {
    let out = compile_and_run(
        r#"<?php
function probe_mixed_receiver(mixed $value): bool {
    return is_callable([$value, "anything"]);
}
function cdata_receiver(\FFI\CData $value): \FFI\CData {
    echo "receiver\n";
    return $value;
}
function mixed_receiver(mixed $value): mixed {
    echo "receiver\n";
    return $value;
}
function probe_method_name(): string {
    echo "method\n";
    return "anything";
}
$libc = FFI::cdef("int (*abs)(int);");
$value = FFI::new("int");
foreach ([fn() => is_callable([$value, "anything"]), fn() => probe_mixed_receiver($value)] as $probe) {
    try {
        var_dump($probe());
    } catch (\Error $error) {
        echo get_class($error), ": ", $error->getMessage(), "\n";
    }
}
var_dump(probe_mixed_receiver(new \stdClass()));
try {
    is_callable([cdata_receiver($value), probe_method_name()]);
} catch (\Error $error) {
    echo $error->getMessage(), "\n";
}
try {
    is_callable([mixed_receiver($value), probe_method_name()]);
} catch (\Error $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out.trim_end(),
        "Error: Object of type FFI\\CData does not support method calls\n\
Error: Object of type FFI\\CData does not support method calls\n\
bool(false)\n\
receiver\nmethod\n\
Object of type FFI\\CData does not support method calls\n\
receiver\nmethod\n\
Object of type FFI\\CData does not support method calls"
    );
}

/// Verifies a method call on CData is a catchable runtime Error, including internal
/// prelude method names and dynamic names, and that arguments are never evaluated.
#[test]
fn test_ffi_cdata_method_calls_throw_at_runtime() {
    let out = compile_and_run(
        r#"<?php
function cdata_argument_side_effect(): int {
    echo "argument\n";
    return 1;
}
function call_dynamic_method(mixed $object, string $method): void {
    $object->{$method}();
}
$libc = FFI::cdef("int (*abs)(int);");
$value = FFI::new("int");
$method = "dynamic_name";
foreach ([
    fn() => $value->foo(),
    fn() => $value->{$method}(),
    fn() => $value->offsetGet(0),
    fn() => $value?->foo(),
    fn() => $value?->{$method}(),
] as $call) {
    try {
        $call();
    } catch (\Error $error) {
        echo $error->getMessage(), "\n";
    }
}
try {
    $value->foo(cdata_argument_side_effect());
} catch (\Error $error) {
    echo "arguments-skipped\n";
}
try {
    call_dynamic_method($value, "mixed_name");
} catch (\Error $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out.trim_end(),
        "Object of type FFI\\CData does not support method calls\n\
Object of type FFI\\CData does not support method calls\n\
Object of type FFI\\CData does not support method calls\n\
Object of type FFI\\CData does not support method calls\n\
Object of type FFI\\CData does not support method calls\n\
arguments-skipped\n\
Object of type FFI\\CData does not support method calls"
    );
}
