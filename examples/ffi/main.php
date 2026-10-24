<?php
// FFI — calling C functions from elephc
// Extern declarations tell the compiler about C function signatures.
// Name the C width: `i32` is a C int, `u64` a size_t, and plain `int` is int64_t/long.
// The linker resolves them from system libraries.

// Individual extern function declarations
extern function abs(i32 $n): i32;
extern function atoi(string $s): i32;
// `?string` is a `char*` that may be NULL: PHP null passes NULL, and a NULL return reads as null.
extern function getenv(string $name): ?string;
extern function setlocale(i32 $category, ?string $locale): ?string;
extern function getpid(): i32;
extern function strlen(string $s): u64;
extern function strcmp(string $left, string $right): i32;
extern function signal(i32 $sig, callable $handler): ptr;
extern function raise(i32 $sig): i32;
extern global ptr $environ;
// C `float` is 32 bits: declare it as f32 and elephc rounds and widens at the boundary.
extern function sqrtf(f32 $x): f32;

// Extern block — multiple functions from one library
extern "System" {
    function strtol(string $s, ptr $endptr, i32 $base): int;
}

// Call C's abs()
echo "abs(n: -42) = " . abs(n: -42) . "\n";

// Parse string to integer with C's atoi()
echo "atoi('999') = " . atoi("999") . "\n";

// Get environment variables: getenv() returns NULL for an unset one, which reads as null
$home = getenv("HOME");
echo "HOME = " . ($home ?? "(unset)") . "\n";
$missing = getenv("ELEPHC_EXAMPLE_UNSET_VARIABLE");
echo "unset variable is null? " . ($missing === null ? "yes" : "no") . "\n";

// setlocale() queries the current locale when the name is NULL, and returns NULL for an
// unknown locale. LC_ALL is 0 in Apple's libc and 6 in glibc.
$lc_all = PHP_OS_FAMILY === "Darwin" ? 0 : 6;
echo "locale = " . (setlocale($lc_all, null) ?? "(none)") . "\n";
echo "unknown locale is null? " . (setlocale($lc_all, "xx_NOT_A_LOCALE") === null ? "yes" : "no") . "\n";

echo "environ is null? " . (ptr_is_null($environ) ? "yes" : "no") . "\n";

// Get process ID
echo "PID = " . getpid() . "\n";

// Single-precision square root through libm's sqrtf()
echo "sqrtf(2) = " . sqrtf(2.0) . "\n";

// C strlen vs PHP strlen (same name, extern shadows builtin)
echo "strlen('hello') = " . strlen("hello") . "\n";

// Named arguments work for extern calls, including after a spread prefix.
$cmp_args = ["a"];
echo "strcmp(..., right: 'b') = " . (strcmp(...$cmp_args, right: "b") < 0 ? "lt" : "ge") . "\n";

// Parse hex with strtol
echo "0xFF = " . strtol(base: 16, s: "FF", endptr: ptr_null()) . "\n";

// Parse octal
echo "0o77 = " . strtol("77", ptr_null(), 8) . "\n";

// Pass an elephc function to C as a callback.
// The callback is passed by string name and must use a C-compatible ABI shape.
function on_signal($sig) {
    echo "signal = " . $sig . "\n";
}

signal(15, "on_signal");
raise(15);
