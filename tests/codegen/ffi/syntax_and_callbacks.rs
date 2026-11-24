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
    // PHP frames each conversion warning with a leading blank line and an `in FILE on line N`
    // suffix; the fixture compiles into a per-test temporary directory, so compare the message
    // text without the volatile location.
    assert_eq!(
        normalize_diagnostic_text(&out.stdout).trim_end(),
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

/// Regression coverage for ext/ffi's `@prefer-ref` place arguments, binary strings, enum
/// constants and php-src's data-boundary checks.
///
/// A direct scalar field read feeding `FFI::memset`/`memcpy` must resolve to that field at
/// the argument's evaluation point, not to the last field read later in the argument list.
#[test]
fn test_ffi_memory_place_arguments_strings_and_boundaries() {
    let out = compile_and_run_capture_merged_with_php_version(
        r#"<?php
$ffi = FFI::cdef("
typedef struct { int a; int b; } Pair;
typedef struct { char bytes[6]; } Bytes;
enum Color { RED = 11, GREEN = 22 };
");
$p = $ffi->new("Pair");
$p->a = 0x01010101;
$p->b = 0x02030406;
FFI::memset($p->a, $p->b, 1);
printf("memset=%08x,%08x\n", $p->a, $p->b);
$p->a = 0x01010101;
$p->b = 0x02030406;
FFI::memcpy($p->a, $p->b, 1);
printf("memcpy=%08x,%08x\n", $p->a, $p->b);
$ptr = FFI::addr($p->a);
FFI::memset($ptr, 0x7f, 1);
printf("addr=%08x\n", $p->a);
$b = $ffi->new("Bytes");
for ($i = 0; $i < 6; $i++) { $b->bytes[$i] = chr(97 + $i); }
$b->bytes[1] = "\0";
$b->bytes[3] = "\0";
$text = FFI::string($b->bytes, 6);
printf("string=%d:%s\n", strlen($text), bin2hex($text));
printf("cstr=%d\n", strlen(FFI::string($b->bytes)));
printf("memcmp=%d,%d\n", FFI::memcmp($b->bytes, "abc", 3), FFI::memcmp($b->bytes, $text, 3));
printf("enum=%d\n", $ffi->RED);
var_dump(is_iterable($ffi), $ffi instanceof Traversable);
var_dump(class_implements($ffi));
$scoped = FFI::cdef('#define FFI_SCOPE "TEST_CASE"
int puts(const char*);');
printf("scope=%s\n", FFI::scope("test_case") instanceof FFI ? "ok" : "bad");
printf("cast=%d,%d\n", FFI::cast("int", $p->a)->cdata, FFI::cast("int", 5)->cdata);
printf("type=%s\n", FFI::type("int")->getName());
printf("intro=%d,%d,%s\n", FFI::sizeof($p->a), FFI::alignof($p->a), FFI::typeof($p->a)->getName());
printf("named=%d\n", FFI::sizeof(ptr: $p->a));
printf("itype=%s,icast=%d\n", $ffi->type("Pair")->getName(), $ffi->cast("Pair*", $p) instanceof FFI\CData ? 1 : 0);
try { $b->bytes[99]; } catch (Throwable $error) { }
printf("recover=%d\n", strlen(FFI::string($b->bytes, 1)));
$walk = $ffi->new("int");
var_dump(array_walk($walk, fn($value) => null));
$dump = print_r([$ffi->new("int")], true);
printf("dump=%d\n", strpos($dump, "FFI\\CData Object") !== false ? 1 : 0);
$grid = $ffi->new("int[2][3]");
$grid[1][2] = 42;
$row = $grid[1];
unset($grid);
printf("view=%d\n", $row[2]);
$ints = $ffi->new("int[2]");
$ints[0] = 1;
$ints[1] = 2;
foreach ($ints as &$view) { $view->cdata += 1; }
$sum = 0;
foreach ($ints as $value) { $sum += is_int($value) ? $value : 100; }
printf("iter=%d\n", $sum);
$array_type = FFI::arrayType(FFI::typeof($p->a), [2, 3]);
printf("dims=%d,%d\n", $array_type->getArrayLength(), $array_type->getArrayElementType()->getArrayLength());
printf("castptr=%d,%d\n", FFI::isNull(FFI::cast("void*", 5)) ? 1 : 0, FFI::isNull(FFI::cast("void*", 0)) ? 1 : 0);
$ffi_abs = FFI::cdef("int abs(int);");
var_dump(is_callable($ffi_abs->abs));
$double_value = $ffi->new("double");
$double_value->cdata = 3;
$int_value = $ffi->new("int");
$int_value->cdata = 1.7;
printf("coerce=%.1f,%d\n", $double_value->cdata, $int_value->cdata);
$globals = FFI::cdef("extern int optind;");
$global_name = "optind";
var_dump(is_int($globals->optind), is_int($globals->$global_name));
$printf = FFI::cdef("int printf(const char *format, ...);");
$double_cdata = $ffi->new("double");
$double_cdata->cdata = 2.5;
$printf->printf("%g\n", $double_cdata);
$libc = FFI::cdef("void *memcpy(void *dest, const void *src, size_t n);");
$argbuf = $libc->new("char[4]");
$libc->memcpy($argbuf, "a\0b", 3);
printf("argnul=%s\n", bin2hex(FFI::string($argbuf, 3)));
foreach ([
    fn() => $p->missing,
    fn() => $b->bytes[99],
    fn() => FFI::isNull($p->a),
    fn() => FFI::string($p->a),
    fn() => FFI::free($p->a),
] as $probe) {
    try { $probe(); echo "ok\n"; } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
foreach ([
    fn() => FFI::memset(5, 0, 1),
    fn() => FFI::free(5),
    fn() => FFI::memcpy($b, $b, 1000),
    fn() => FFI::memcmp($b, $b, 1000),
    fn() => FFI::string($b, 1000),
    fn() => FFI::memset($b, 0, 1000),
    fn() => FFI::string($ffi->new("int")),
    fn() => FFI::free($ffi->new("int")),
    fn() => FFI::cdef("int definitely_not_a_symbol_xyz(int);"),
    fn() => FFI::cdef("extern int definitely_not_a_global_xyz;"),
] as $probe) {
    try { $probe(); echo "ok\n"; } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
"#,
        elephc::php_version::PhpVersion::Php85,
    );
    assert!(out.success, "{}", out.stdout);
    assert_eq!(
        normalize_diagnostic_text(&out.stdout).trim_end(),
        "memset=01010106,02030406\n\
memcpy=01010106,02030406\n\
addr=0101017f\n\
string=6:610063006566\n\
cstr=1\n\
memcmp=-1,0\n\
enum=11\n\
bool(false)\n\
bool(false)\n\
array(0) {\n\
}\n\
scope=ok\n\
Deprecated: Calling FFI::cast() statically is deprecated\n\
Deprecated: Calling FFI::cast() statically is deprecated\n\
cast=16843135,5\n\
Deprecated: Calling FFI::type() statically is deprecated\n\
type=int32_t\n\
intro=4,4,int32_t\n\
named=4\n\
itype=struct <anonymous>,icast=1\n\
recover=1\n\
bool(true)\n\
dump=1\n\
view=42\n\
iter=5\n\
dims=2,3\n\
Deprecated: Calling FFI::cast() statically is deprecated\n\
Deprecated: Calling FFI::cast() statically is deprecated\n\
castptr=0,1\n\
bool(true)\n\
coerce=3.0,1\n\
bool(true)\n\
bool(true)\n\
2.5\n\
argnul=610062\n\
FFI\\Exception:Attempt to read undefined field 'missing' of C struct/union\n\
FFI\\Exception:C array index out of bounds\n\
FFI\\Exception:FFI\\Cdata is not a pointer\n\
FFI\\Exception:FFI\\Cdata is not a C string\n\
FFI\\Exception:free() non a C pointer\n\
TypeError:FFI::memset(): Argument #1 ($ptr) must be of type FFI\\CData, int given\n\
TypeError:FFI::free(): Argument #1 ($ptr) must be of type FFI\\CData, int given\n\
FFI\\Exception:Attempt to write over data boundary\n\
FFI\\Exception:attempt to read over data boundary\n\
FFI\\Exception:attempt to read over data boundary\n\
FFI\\Exception:attempt to write over data boundary\n\
FFI\\Exception:FFI\\Cdata is not a C string\n\
FFI\\Exception:free() non a C pointer\n\
FFI\\Exception:Failed resolving C function 'definitely_not_a_symbol_xyz'\n\
FFI\\Exception:Failed resolving C variable 'definitely_not_a_global_xyz'"
    );
}

/// Internal prelude helpers are not a way around `ffi.enable=0`.
#[test]
fn test_ffi_disabled_api_rejects_internal_wrappers() {
    struct Restore;
    impl Drop for Restore {
        /// Clears the test's compile-time INI override.
        fn drop(&mut self) {
            elephc::codegen::set_ini_overrides(Vec::new());
        }
    }
    elephc::codegen::set_ini_overrides(vec![("ffi.enable".to_string(), "0".to_string())]);
    let _restore = Restore;
    let out = compile_and_run_capture(
        r#"<?php
foreach ([
    fn() => FFI::__elephc_wrap(0),
    fn() => FFI::cdef("int abs(int);"),
] as $probe) {
    try { $probe(); echo "ok\n"; } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
"#,
    );
    assert!(out.success, "{}", out.stdout);
    assert_eq!(
        normalize_diagnostic_text(&out.stdout).trim_end(),
        "FFI\\Exception:FFI API is restricted by \"ffi.enable\" configuration directive\n\
FFI\\Exception:FFI API is restricted by \"ffi.enable\" configuration directive"
    );
}

/// C integer radix, parser line accounting and long-double kind follow php-src.
///
/// A leading-zero literal is octal (never decimal), preprocessor lines keep their source line in
/// later diagnostics, and `FFI\CType` reports `TYPE_LONGDOUBLE` for `long double`.
#[test]
fn test_ffi_c_parser_radix_lines_and_long_double_kind() {
    let out = compile_and_run(
        r##"<?php
$ffi = FFI::cdef('enum { MODE = 0644 };');
var_dump($ffi->MODE);
try {
    FFI::cdef("#define A 1\n#define B 2\nint broken(];");
} catch (FFI\ParserException $error) {
    echo str_ends_with($error->getMessage(), 'at line 3') ? "line3\n" : $error->getMessage() . "\n";
}
var_dump($ffi->type('long double')->getKind() === FFI\CType::TYPE_LONGDOUBLE);
"##,
    );
    assert_eq!(out, "int(420)\nline3\nbool(true)\n");
}

/// `iterator_to_array()` rejects FFI contexts and CTypes with php-src's TypeError.
///
/// Their internal iterator handlers serve `foreach`; the SPL helper must not enter them.
#[test]
fn test_ffi_iterator_to_array_rejects_ffi_receivers() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('int abs(int);');
try { iterator_to_array($ffi); echo "no-error\n"; }
catch (TypeError $error) { echo $error->getMessage(), "\n"; }
try { iterator_to_array($ffi->type('int')); echo "no-error\n"; }
catch (TypeError $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, FFI given\n\
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, FFI\\CType given\n"
    );
}

/// C global string assignment follows php-src's conversion and diagnostic rules.
///
/// A `char*` global rejects a PHP string with the exact incompatible-types message, while a
/// numeric global accepts the numeric prefix instead of throwing.
#[test]
fn test_ffi_global_string_assignment_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('extern char *__progname; extern int optind;');
try { $ffi->__progname = 'elephc'; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
$ffi->optind = '12';
var_dump($ffi->optind);
"#,
    );
    assert_eq!(
        out,
        "Incompatible types when assigning to type 'char*' from PHP 'string'\nint(12)\n"
    );
}

/// CType field lookups raise php-src's diagnostics instead of returning sentinels.
#[test]
fn test_ffi_ctype_struct_field_lookup_diagnostics() {
    let out = compile_and_run(
        r#"<?php
$struct = FFI::type('struct { int a; }');
foreach (['getStructFieldOffset', 'getStructFieldType'] as $method) {
    try { $struct->$method('b'); echo "no-error\n"; }
    catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
}
$scalar = FFI::type('int');
foreach (['getStructFieldOffset', 'getStructFieldType'] as $method) {
    try { $scalar->$method('a'); echo "no-error\n"; }
    catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "Wrong field name\nWrong field name\nFFI\\CType is not a structure\nFFI\\CType is not a structure\n"
    );
}

/// Consecutive 64-bit bit-fields past the first 16 bytes round-trip through CData fields.
///
/// The third field starts at bit 128; a fixed 16-byte window truncated it and panicked on
/// shift overflow, while php-src reads and writes all three fields.
#[test]
fn test_ffi_wide_bitfield_fields_round_trip() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct s { unsigned long long a:64; unsigned long long b:64; unsigned long long c:64; };');
$value = $ffi->new('struct s');
$value->a = 1; $value->b = 2; $value->c = 3;
var_dump($value->a, $value->b, $value->c, FFI::sizeof($value));
"#,
    );
    assert_eq!(out, "int(1)\nint(2)\nint(3)\nint(24)\n");
}

/// Nullsafe FFI instance methods keep the receiver's cdef context.
#[test]
fn test_ffi_nullsafe_instance_methods_use_receiver_context() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('typedef struct { int a; } S;');
$value = $ffi?->new('S'); var_dump(FFI::sizeof($value));
$copy = $ffi?->cast('S', $value); var_dump(FFI::sizeof($copy));
var_dump($ffi?->type('int')->getKind());
"#,
    );
    assert_eq!(out, "int(4)\nint(4)\nint(9)\n");
}

/// Runtime CData field names read through `__get`, and bool writes follow php-src.
#[test]
fn test_ffi_dynamic_cdata_fields_and_bool_writes() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct s { int i; char c; double g; };');
$cdata = $ffi->new('struct s');
$name = 'i'; $cdata->{$name} = 3; var_dump($cdata->{$name});
$name = 'c'; $cdata->{$name} = 'a'; var_dump($cdata->{$name});
$cdata->g = true; var_dump($cdata->g);
try { $cdata->c = false; } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "int(3)\nstring(1) \"a\"\nfloat(1)\n\
Incompatible types when assigning to type 'char' from PHP 'false'\n"
    );
}

/// Nested `foreach` loops over one CData array keep independent cursors.
#[test]
fn test_ffi_nested_cdata_foreach_cursors() {
    let out = compile_and_run(
        r#"<?php
$values = FFI::new('int[3]');
$values[0] = 1; $values[1] = 2; $values[2] = 3;
foreach ($values as $outer) {
    foreach ($values as $inner) { echo $outer, $inner, ','; }
}
echo "\n";
$ref = FFI::new('int[2]');
$ref[0] = 1; $ref[1] = 2;
foreach ($ref as &$value) { $value->cdata += 3; }
echo $ref[0], $ref[1], "\n";
"#,
    );
    assert_eq!(out, "11,12,13,21,22,23,31,32,33,\n45\n");
}

/// Pointer casts convert scalars, share aggregate storage, and sized reads honor their length.
#[test]
fn test_ffi_cast_conversions_and_sized_string_reads() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct Node { int value; struct Node *next; }; struct D { double g; }; double sqrt(double); char *strdup(const char*);');
$first = $ffi->new('struct Node');
$second = $ffi->new('struct Node', false);
$second->value = 7;
$first->next = $second;
var_dump($first->next->value);
$double = $ffi->cast('double', 5); var_dump($double->cdata);
$integer = $ffi->cast('int', 1.5); var_dump($integer->cdata);
$chars = FFI::new('char[4]'); $chars[0] = 'a';
$bytes = FFI::cast('uint8_t[4]', $chars); $bytes[0] = 66; var_dump($chars[0]);
$buffer = FFI::new('char[4]'); $pointer = FFI::cast('char*', $buffer); $pointer[0] = 'b'; var_dump($buffer[0]);
$record = $ffi->new('struct D'); $record->g = '.5'; var_dump($record->g);
$text = $ffi->strdup('abcdef'); var_dump(FFI::string($text, 3));
class CastProbeBox { public mixed $callback = 'strlen'; }
function probeCastBox(mixed $box): bool { return is_callable($box->callback); }
var_dump(probeCastBox(new CastProbeBox()));
$dead = FFI::new('double', false);
FFI::free($dead);
try { $ffi->sqrt($dead); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "int(7)\nfloat(5)\nint(1)\nstring(1) \"B\"\nstring(1) \"b\"\nfloat(0.5)\nstring(3) \"abc\"\nbool(true)\nUse after free()\n"
    );
}

/// `is_countable()` sees CData's internal Countable handler through boxed and direct paths.
#[test]
fn test_ffi_cdata_is_countable_through_boxed_receivers() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('');
$data = $ffi->new('int[2]');
function countProbe(mixed $value): bool { return is_countable($value); }
var_dump(is_countable($data), countProbe($data), count($data));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nint(2)\n");
}

/// Mixed receivers keep ordinary method dispatch while CData raises its method-call Error.
#[test]
fn test_ffi_mixed_receiver_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class MixedAccessBox implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed { return 9; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
function readMixedMethod(mixed $value): mixed { return $value->offsetGet(0); }
$data = FFI::new('int[2]'); $data[0] = 7;
try { var_dump(readMixedMethod($data)); } catch (Error $error) { echo $error->getMessage(), "\n"; }
var_dump(readMixedMethod(new MixedAccessBox()));
"#,
    );
    assert_eq!(
        out,
        "Object of type FFI\\CData does not support method calls\nint(9)\n"
    );
}

/// Non-CData objects passed to C functions are rejected before recording a fake handle.
#[test]
fn test_ffi_non_cdata_object_argument_is_rejected() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('int abs(int);');
try { $ffi->abs($ffi->type('int')); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
try { $ffi->abs(new stdClass()); } catch (FFI\Exception $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Passing incompatible argument for a C parameter, found PHP 'FFI\\CType'\n\
Passing incompatible argument for a C parameter, found PHP 'stdClass'\n"
    );
}

/// `sizeof()` and `addr()` on a CData field keep the field storage, including pointer slots.
#[test]
fn test_ffi_field_views_preserve_scalar_and_pointer_slots() {
    let out = compile_and_run(
        r#"<?php
$ffi = FFI::cdef('struct S { int *p; int i; };');
$record = $ffi->new('struct S');
var_dump(FFI::sizeof($record->p));
$pointer = FFI::addr($record->p);
var_dump($pointer[0] === null);
$record->i = 5;
$slot = FFI::addr($record->i);
$slot[0] = 6;
var_dump($record->i);
"#,
    );
    assert_eq!(out, "int(8)\nbool(true)\nint(6)\n");
}
