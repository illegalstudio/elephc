//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of echo vars, including echo hello world, echo empty string, and echo multiple strings.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Phase 1: Echo strings ---

/// Verifies basic echo still reaches stdout after the terminal write is routed
/// through the `__rt_stdout_write` runtime indirection. Non-web binaries leave
/// the `_elephc_web_capture` flag at 0, so output must travel the plain `write(1, …)`
/// syscall path byte-for-byte unchanged.
#[test]
fn echo_routes_through_stdout_write() {
    let out = compile_and_run("<?php echo \"abc\";");
    assert_eq!(out, "abc");
}

/// Compiles `<?php echo "Hello, World!\n";` and asserts stdout is `"Hello, World!\n"`.
#[test]
fn test_echo_hello_world() {
    let out = compile_and_run("<?php echo \"Hello, World!\\n\";");
    assert_eq!(out, "Hello, World!\n");
}

/// Compiles `<?php echo "";` and asserts stdout is empty string.
#[test]
fn test_echo_empty_string() {
    let out = compile_and_run("<?php echo \"\";");
    assert_eq!(out, "");
}

/// Compiles two consecutive `echo` statements and verifies output concatenation order.
/// Input: `<?php echo "foo"; echo "bar"; echo "\n";` → stdout `"foobar\n"`.
#[test]
fn test_echo_multiple_strings() {
    let out = compile_and_run("<?php echo \"foo\"; echo \"bar\"; echo \"\\n\";");
    assert_eq!(out, "foobar\n");
}

/// Compiles `<?php echo "a\tb\nc";` and verifies tab and newline escape sequences are preserved.
#[test]
fn test_echo_escape_sequences() {
    let out = compile_and_run("<?php echo \"a\\tb\\nc\";");
    assert_eq!(out, "a\tb\nc");
}

/// Verifies a variable whose name contains non-ASCII letters (PHP allows identifier
/// bytes 0x80-0xFF) round-trips through the full pipeline and echoes its value.
#[test]
fn test_echo_unicode_variable() {
    let out = compile_and_run("<?php $café = 7; echo $café;");
    assert_eq!(out, "7");
}

/// Verifies a user function with a non-ASCII name is declared, mangled to a valid symbol,
/// and called end-to-end.
#[test]
fn test_call_unicode_function_name() {
    let out = compile_and_run("<?php function 价格() { return 5; } echo 价格();");
    assert_eq!(out, "5");
}

/// Verifies a variable with non-ASCII letters interpolates inside a double-quoted string
/// instead of being truncated at the first non-ASCII byte.
#[test]
fn test_echo_unicode_variable_interpolated() {
    let out = compile_and_run("<?php $café = \"x\"; echo \"v=$café\";");
    assert_eq!(out, "v=x");
}

// --- Phase 2: Variables and integers ---

/// Compiles `<?php echo 42;` and asserts stdout is `"42"`.
#[test]
fn test_echo_integer() {
    let out = compile_and_run("<?php echo 42;");
    assert_eq!(out, "42");
}

/// Compiles `<?php echo 0;` and asserts stdout is `"0"`. Regression guard for zero handling.
#[test]
fn test_echo_zero() {
    let out = compile_and_run("<?php echo 0;");
    assert_eq!(out, "0");
}

/// Compiles `<?php echo -7;` and asserts stdout is `"-7"`. Verifies negative integer sign is emitted correctly.
#[test]
fn test_echo_negative() {
    let out = compile_and_run("<?php echo -7;");
    assert_eq!(out, "-7");
}

/// Compiles `<?php echo 1000000;` and asserts stdout is `"1000000"`. Large integer literal regression guard.
#[test]
fn test_echo_large_number() {
    let out = compile_and_run("<?php echo 1000000;");
    assert_eq!(out, "1000000");
}

/// Compiles `<?php $x = 42; echo $x;` and asserts stdout is `"42"`. Verifies integer variable load and echo.
#[test]
fn test_variable_int() {
    let out = compile_and_run("<?php $x = 42; echo $x;");
    assert_eq!(out, "42");
}

/// Compiles `<?php $s = "hello"; echo $s;` and asserts stdout is `"hello"`. Verifies string variable load and echo.
#[test]
fn test_variable_string() {
    let out = compile_and_run("<?php $s = \"hello\"; echo $s;");
    assert_eq!(out, "hello");
}

/// Compiles `<?php $x = 1; $x = 2; echo $x;` and asserts stdout is `"2"`.
/// Verifies same-type reassignment overwrites the prior value.
#[test]
fn test_variable_reassign_same_type() {
    let out = compile_and_run("<?php $x = 1; $x = 2; echo $x;");
    assert_eq!(out, "2");
}

/// Compiles two variable declarations and interleaved echo statements.
/// Input: `<?php $a = 10; $b = 20; echo $a; echo " "; echo $b; echo "\n";`
/// → stdout `"10 20\n"`. Verifies variable load ordering and string concatenation.
#[test]
fn test_multiple_variables() {
    let out =
        compile_and_run("<?php $a = 10; $b = 20; echo $a; echo \" \"; echo $b; echo \"\\n\";");
    assert_eq!(out, "10 20\n");
}

/// Compiles `<?php $x = -100; echo $x;` and asserts stdout is `"-100"`.
/// Verifies negative integer is stored and loaded correctly from a variable.
#[test]
fn test_variable_negative_int() {
    let out = compile_and_run("<?php $x = -100; echo $x;");
    assert_eq!(out, "-100");
}

/// Compiles `<?php $z = 0; echo $z;` and asserts stdout is `"0"`.
/// Verifies zero value stored in a variable is loaded and echoed correctly.
#[test]
fn test_echo_int_zero_variable() {
    let out = compile_and_run("<?php $z = 0; echo $z;");
    assert_eq!(out, "0");
}
