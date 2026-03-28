<?php
// FFI — calling C functions from elephc
// Extern declarations tell the compiler about C function signatures.
// The linker resolves them from system libraries.

// Individual extern function declarations
extern function abs(int $n): int;
extern function atoi(string $s): int;
extern function getenv(string $name): string;
extern function getpid(): int;
extern function strlen(string $s): int;
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;
extern global ptr $environ;

// Extern block — multiple functions from one library
extern "System" {
    function strtol(string $s, ptr $endptr, int $base): int;
}

// Call C's abs()
echo "abs(-42) = " . abs(-42) . "\n";

// Parse string to integer with C's atoi()
echo "atoi('999') = " . atoi("999") . "\n";

// Get environment variable
$home = getenv("HOME");
echo "HOME = " . $home . "\n";
echo "environ is null? " . (ptr_is_null($environ) ? "yes" : "no") . "\n";

// Get process ID
echo "PID = " . getpid() . "\n";

// C strlen vs PHP strlen (same name, extern shadows builtin)
echo "strlen('hello') = " . strlen("hello") . "\n";

// Parse hex with strtol
echo "0xFF = " . strtol("FF", ptr_null(), 16) . "\n";

// Parse octal
echo "0o77 = " . strtol("77", ptr_null(), 8) . "\n";

// Pass an elephc function to C as a callback.
// The callback is passed by string name and must use a C-compatible ABI shape.
function on_signal($sig) {
    echo "signal = " . $sig . "\n";
}

signal(15, "on_signal");
raise(15);
