//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC stack args, including function call supports stack passed overflow args, instance method call supports stack passed overflow args, and constructor call supports stack passed overflow args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies that a user function with 9 scalar arguments (exceeding the 8 integer register
/// argument limit on ARM64) correctly passes overflow arguments via the stack. The callee sums
/// all 9 arguments and echoes the result. Only the first 8 args land in registers; the 9th must
/// be materialised from the caller's stack frame.
#[test]
fn test_function_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
sum9(1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

/// Verifies that an instance method with 7 parameters (exceeding the 6-register method-argument
/// limit on ARM64) correctly passes the final string argument via the stack when the preceding
/// six slots are filled with integer registers. The receiver `$this` occupies a register, so only
/// 6 additional registers are available for parameters. The last (7th) argument must be loaded
/// from the caller's stack by the callee.
#[test]
fn test_instance_method_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class GreeterOverflow {
    public function greet($a, $b, $c, $d, $e, $f, string $message) {
        echo $message;
    }
}
$g = new GreeterOverflow();
$g->greet(1, 2, 3, 4, 5, 6, "hello");
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies that a constructor with 7 parameters (6 integer registers + 1 overflow string on
/// the stack) correctly receives the overflow string argument via the caller's stack frame. The
/// string is stored on `$this->message` and echoed after construction.
#[test]
fn test_constructor_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class ConstructorOverflow {
    public $message;
    public function __construct($a, $b, $c, $d, $e, $f, string $message) {
        $this->message = $message;
    }
}
$value = new ConstructorOverflow(1, 2, 3, 4, 5, 6, "stack");
echo $value->message;
"#,
    );
    assert_eq!(out, "stack");
}

/// Verifies that a static method with 8 parameters (exceeding the 7-register method-argument
/// limit on ARM64 when the receiver is not present) correctly passes overflow arguments via the
/// caller's stack frame. Only 7 args land in registers; the 8th is materialised from the stack.
#[test]
fn test_static_method_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class StaticOverflow {
    public static function pick($a, $b, $c, $d, $e, $f, $g, $h) {
        echo $h;
    }
}
StaticOverflow::pick(1, 2, 3, 4, 5, 6, 7, 8);
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies that a callable-variable call (first-class callable syntax `sum9(...)`) with 9
/// arguments (exceeding the 8-register argument limit) correctly passes overflow arguments via
/// the caller's stack frame. The callable is resolved at runtime and the overflow 9th argument
/// must be readable by the callee from the caller's stack.
#[test]
fn test_callable_variable_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
$fn = sum9(...);
$fn(1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

/// Verifies that by-ref arguments are correctly handled when the calling function has a very large
/// stack frame (520 scalar locals). This exercises large-frame stack slot indexing: the
/// `bump(&$slot519)` call must correctly address `$slot519` at offset 519 from the frame pointer,
/// and the callee `bump` must be able to write back through the reference to the correct slot.
/// Regression guard for incorrect frame-index calculation with large stack frames.
#[test]
fn test_by_ref_argument_supports_large_stack_offsets() {
    let mut source = String::from("<?php\nfunction bump(&$value) { $value = $value + 1; }\nfunction large_frame() {\n");
    for i in 0..520 {
        source.push_str(&format!("    $slot{} = {};\n", i, i));
    }
    source.push_str("    bump($slot519);\n    echo $slot519;\n}\nlarge_frame();\n");

    let out = compile_and_run(&source);
    assert_eq!(out, "520");
}

/// Verifies that a function with 9 float parameters (exceeding the 8-register floating-point
/// argument limit on ARM64) correctly passes overflow arguments via the caller's stack frame.
/// Floating-point arguments occupy `d0`–`d7` registers; a 9th float argument must be loaded from
/// the caller's stack by the callee. The sum is cast to int before echo.
#[test]
fn test_float_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9f(float $a, float $b, float $c, float $d, float $e, float $f, float $g, float $h, float $i) {
    echo (int) ($a + $b + $c + $d + $e + $f + $g + $h + $i);
}
sum9f(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
"#,
    );
    assert_eq!(out, "45");
}

/// Verifies that `call_user_func_array` correctly passes 9 arguments (exceeding the 8-register
/// argument limit) to the called function via the caller's stack frame. The array is unpacked at
/// runtime; the overflow 9th element must be materialised from the stack by the callee. This
/// exercises the variadic-call path for `call_user_func_array`.
#[test]
fn test_call_user_func_array_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
call_user_func_array("sum9", [1, 2, 3, 4, 5, 6, 7, 8, 9]);
"#,
    );
    assert_eq!(out, "45");
}
