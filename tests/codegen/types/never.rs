//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types never, including never return type throws and is caught, never instance method throws and is caught, and never static method throws and is caught.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Verifies a function with `never` return type that throws an exception can be caught via `try`/`catch`.
#[test]
fn test_never_return_type_throws_and_is_caught() {
    let out = compile_and_run(
        "<?php
        function fail(): never {
            throw new \\Exception(\"boom\");
        }
        try {
            fail();
            echo \"unreachable\";
        } catch (\\Exception $e) {
            echo $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "boom");
}

/// Verifies an instance method with `never` return type that throws can be caught via `try`/`catch`
/// when called on an object instance.
#[test]
fn test_never_instance_method_throws_and_is_caught() {
    let out = compile_and_run(
        "<?php
        class Failer {
            public function fail(): never {
                throw new \\Exception(\"method-boom\");
            }
        }
        $f = new Failer();
        try {
            $f->fail();
            echo \"unreachable\";
        } catch (\\Exception $e) {
            echo $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "method-boom");
}

/// Verifies a static method with `never` return type that throws can be caught via `try`/`catch`
/// when called via the class.
#[test]
fn test_never_static_method_throws_and_is_caught() {
    let out = compile_and_run(
        "<?php
        class Failer {
            public static function fail(): never {
                throw new \\Exception(\"static-boom\");
            }
        }
        try {
            Failer::fail();
            echo \"unreachable\";
        } catch (\\Exception $e) {
            echo $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "static-boom");
}

/// Verifies a `never`-returning function that calls `exit` produces no further output and the binary
/// exits gracefully (no "unreachable" echoed).
#[test]
fn test_never_function_calls_exit() {
    let out = compile_and_run_expect_failure(
        "<?php
        function bail(): never {
            exit(1);
        }
        bail();
        echo \"unreachable\";
        ",
    );
    assert!(
        out.is_empty() || !out.contains("unreachable"),
        "unexpected output before exit: {:?}",
        out,
    );
}

/// Verifies that code following a `never`-returning call (`panic("oops")`) is treated as unreachable
/// and does not cause a compile error; the exception is caught and "oops" is echoed.
#[test]
fn test_never_function_call_followed_by_unreachable_code_compiles() {
    let out = compile_and_run(
        "<?php
        function panic(string $msg): never {
            throw new \\Exception($msg);
        }
        try {
            panic(\"oops\");
            $x = 42;
            echo $x;
        } catch (\\Exception $e) {
            echo $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "oops");
}

/// Verifies that a `never`-returning function with no explicit return produces a runtime fatal error
/// containing "never-returning function must not implicitly return".
#[test]
fn test_never_function_implicit_return_fails_at_runtime() {
    let err = compile_and_run_expect_failure(
        "<?php
        function fail(): never {
        }
        fail();
        echo \"unreachable\";
        ",
    );
    assert!(
        err.contains("never-returning function must not implicitly return"),
        "unexpected stderr: {:?}",
        err,
    );
}

/// Verifies that calling `gettype()` on a `never`-returning function does not materialize
/// a never-type value at runtime; the binary must fail and stderr must contain the implicit-return error.
#[test]
fn test_gettype_never_call_does_not_materialize_never_value() {
    let out = compile_and_run_capture(
        "<?php
        function fail(): never {
        }
        echo gettype(fail());
        ",
    );
    assert!(
        !out.success,
        "binary unexpectedly succeeded with stdout={:?}",
        out.stdout,
    );
    assert!(
        out.stderr.contains("never-returning function must not implicitly return"),
        "unexpected stderr: {:?}",
        out.stderr,
    );
    assert!(
        out.stdout.is_empty(),
        "gettype() should not report never as a runtime type: {:?}",
        out.stdout,
    );
}

/// Verifies that a `never`-returning method overrides a `void` return type from a parent class;
/// the derived method throws and the exception is caught.
#[test]
fn test_never_overrides_void_parent() {
    let out = compile_and_run(
        "<?php
        class Base {
            public function run(): void {
                echo \"base\";
            }
        }
        class Derived extends Base {
            public function run(): never {
                throw new \\Exception(\"derived\");
            }
        }
        $d = new Derived();
        try {
            $d->run();
        } catch (\\Exception $e) {
            echo $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "derived");
}

/// Compiles and runs the checked-in `examples/never/main.php` fixture and asserts stdout is
/// "port = 8080\ncaught: config error: workers must be positive, got 0\n".
#[test]
fn test_example_never_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/never/main.php"));
    assert_eq!(out, "port = 8080\ncaught: config error: workers must be positive, got 0\n");
}
