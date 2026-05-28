//! Purpose:
//! Codegen regression tests for the SPL exception hierarchy.
//! Verifies direct catches, parent catches, and user subclasses of SPL exception classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - The tests compile small PHP programs and assert the emitted native binary output.

use crate::support::*;

/// Verifies LogicException is catchable by a matching catch clause.
#[test]
fn test_logic_exception_caught_directly() {
    let out = compile_and_run(
        r#"<?php
try { throw new LogicException("logic"); }
catch (LogicException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "logic");
}

/// Verifies InvalidArgumentException (a LogicException subclass) is catchable by a LogicException catch clause.
#[test]
fn test_invalid_argument_caught_by_logic_parent() {
    let out = compile_and_run(
        r#"<?php
try { throw new InvalidArgumentException("bad arg"); }
catch (LogicException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "bad arg");
}

/// Verifies BadMethodCallException (a BadFunctionCallException subclass) is catchable by a BadFunctionCallException catch clause.
#[test]
fn test_bad_method_call_caught_by_function_call_parent() {
    let out = compile_and_run(
        r#"<?php
try { throw new BadMethodCallException("nope"); }
catch (BadFunctionCallException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "nope");
}

/// Verifies RuntimeException is catchable by a matching catch clause.
#[test]
fn test_runtime_exception_caught_directly() {
    let out = compile_and_run(
        r#"<?php
try { throw new RuntimeException("runtime"); }
catch (RuntimeException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "runtime");
}

/// Verifies OutOfBoundsException (a RuntimeException subclass) is catchable by a RuntimeException catch clause.
#[test]
fn test_out_of_bounds_caught_by_runtime_parent() {
    let out = compile_and_run(
        r#"<?php
try { throw new OutOfBoundsException("idx"); }
catch (RuntimeException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "idx");
}

/// Verifies OverflowException (a RuntimeException subclass) is catchable by the Exception root catch clause.
#[test]
fn test_overflow_caught_by_exception_root() {
    let out = compile_and_run(
        r#"<?php
try { throw new OverflowException("over"); }
catch (Exception $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "over");
}

/// Verifies DomainException (a LogicException subclass) is catchable by the Exception root catch clause.
#[test]
fn test_domain_exception_caught_by_exception_root() {
    let out = compile_and_run(
        r#"<?php
try { throw new DomainException("dom"); }
catch (Exception $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "dom");
}

/// Verifies a user-defined class extending LogicException is catchable by a LogicException catch clause.
#[test]
fn test_user_extends_logic_exception() {
    let out = compile_and_run(
        r#"<?php
class CustomLogic extends LogicException {}
try { throw new CustomLogic("custom"); }
catch (LogicException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "custom");
}

/// Verifies a user-defined class extending RuntimeException is catchable by a RuntimeException catch clause.
#[test]
fn test_user_extends_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
class CustomRuntime extends RuntimeException {}
try { throw new CustomRuntime("rt"); }
catch (RuntimeException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "rt");
}

/// Verifies all 13 SPL exception classes (LogicException and RuntimeException hierarchies) are throwable and catchable by Exception.
/// The 13 classes are: LogicException, BadFunctionCallException, BadMethodCallException, DomainException,
/// InvalidArgumentException, LengthException, OutOfRangeException, RuntimeException, OutOfBoundsException,
/// OverflowException, RangeException, UnderflowException, UnexpectedValueException.
#[test]
fn test_all_thirteen_spl_exceptions_throwable() {
    let out = compile_and_run(
        r#"<?php
$names = [
    "LogicException",
    "BadFunctionCallException",
    "BadMethodCallException",
    "DomainException",
    "InvalidArgumentException",
    "LengthException",
    "OutOfRangeException",
    "RuntimeException",
    "OutOfBoundsException",
    "OverflowException",
    "RangeException",
    "UnderflowException",
    "UnexpectedValueException",
];
$count = 0;
try { throw new LogicException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new BadFunctionCallException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new BadMethodCallException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new DomainException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new InvalidArgumentException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new LengthException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new OutOfRangeException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new RuntimeException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new OutOfBoundsException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new OverflowException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new RangeException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new UnderflowException(); } catch (Exception $e) { $count = $count + 1; }
try { throw new UnexpectedValueException(); } catch (Exception $e) { $count = $count + 1; }
echo $count;
"#,
    );
    assert_eq!(out, "13");
}
