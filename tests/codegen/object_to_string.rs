//! Purpose:
//! Integration tests for php's object-to-string conversion failure: a class without
//! `__toString` — a `Closure` included — raises a CATCHABLE `Error`, not a fatal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc wrote `Fatal error: Object of class A could not be converted to string` straight to
//!   STDERR and exited 1. php raises `Error`, which `catch (Error $e)` observes; uncaught, it
//!   reports on STDOUT through the output buffer and exits 255. Three things were wrong at once:
//!   the catch never ran, the stream was wrong, and so was the status.
//! - Making the throw catchable was only half of it. The try/catch DCE pass drops every `catch`
//!   clause when its body "cannot throw", and that analysis is SYNTACTIC: it read `echo $o` as
//!   unable to throw, so the handler was gone before codegen ever emitted the `Error`. A string
//!   conversion of anything but a literal now reports that it may throw, the same shape the pass
//!   already used for `/` and `%` raising `DivisionByZeroError`.
//! - First-class callable syntax (`strlen(...)`) builds a Closure, so it lands on the same rule.
//!   elephc used to REFUSE those programs outright — `unsupported EIR backend feature: i_to_str
//!   for PHP type Callable` — which is why the shapes are pinned together here.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// The message php uses for a class with no `__toString`.
const MESSAGE: &str = "Object of class A could not be converted to string";

/// Verifies `echo` of an object without `__toString` is caught and execution continues.
#[test]
fn test_echo_of_an_object_without_tostring_is_a_catchable_error() {
    let out = compile_and_run_capture(
        r#"<?php
class A
{
}
$o = new A();
try {
    echo $o;
} catch (Error $e) {
    echo "caught: ", $e->getMessage(), "\n";
}
echo "still running\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, format!("caught: {MESSAGE}\nstill running\n"));
    assert_eq!(out.exit_code, Some(0));
}

/// Verifies all four conversion syntaxes raise the same catchable `Error`.
///
/// `echo`, `print`, `.` and `(string)` reach the conversion through different lowerings, and an
/// interpolation is a concatenation the lexer builds — four mechanisms, one php rule.
#[test]
fn test_every_conversion_syntax_raises_the_catchable_error() {
    let out = compile_and_run_capture(
        r#"<?php
class A
{
}
$o = new A();
try {
    echo "x" . $o;
} catch (Error $e) {
    echo "concat: ", $e->getMessage(), "\n";
}
try {
    $s = (string) $o;
    echo $s;
} catch (Error $e) {
    echo "cast: ", $e->getMessage(), "\n";
}
try {
    print $o;
} catch (Error $e) {
    echo "print: ", $e->getMessage(), "\n";
}
try {
    echo "interp: {$o}";
} catch (Error $e) {
    echo "interp: ", $e->getMessage(), "\n";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        format!("concat: {MESSAGE}\ncast: {MESSAGE}\nprint: {MESSAGE}\ninterp: {MESSAGE}\n")
    );
}

/// Verifies the UNCAUGHT conversion reports the way php reports it, down to the exit status.
///
/// The old path wrote to stderr and exited 1. php writes the uncaught report to stdout — where
/// `ob_start()` can capture it — names the throw site, prints a stack trace, and exits 255.
#[test]
fn test_an_uncaught_object_conversion_matches_phps_report() {
    let out = compile_and_run_capture(
        r#"<?php
class A
{
}
$o = new A();
echo "before\n";
echo $o;
echo "never reached\n";
"#,
    );
    assert!(!out.success, "the program should have exited non-zero");
    assert_eq!(out.exit_code, Some(255));
    assert_eq!(
        out.located_diagnostics,
        format!("Fatal error: Uncaught Error: {MESSAGE} in test.php:7\n")
    );
    // The trace lines carry no `Kind: ` prefix, so the harness leaves them in `stdout` beside
    // what the program printed. The `thrown in` line names an absolute temp path that changes
    // every run, so only its tail is asserted.
    assert!(
        out.stdout.starts_with("before\nStack trace:\n#0 {main}\n  thrown in "),
        "unexpected stdout: {:?}",
        out.stdout
    );
    assert!(
        out.stdout.ends_with("test.php on line 7\n"),
        "unexpected stdout tail: {:?}",
        out.stdout
    );
}

/// Verifies a first-class callable in a string context raises the Closure form of the Error.
///
/// `strlen(...)` is a `Closure`, and a Closure has no `__toString`. elephc refused the whole
/// program at compile time, so `try { echo $f; } catch (Error $e)` could not even be written.
#[test]
fn test_a_first_class_callable_in_a_string_context_is_a_catchable_error() {
    let out = compile_and_run_capture(
        r#"<?php
$f = strlen(...);
try {
    echo "x" . $f;
} catch (Error $e) {
    echo "concat: ", $e->getMessage(), "\n";
}
try {
    echo $f;
} catch (Error $e) {
    echo "echo: ", $e->getMessage(), "\n";
}
echo "still running\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let closure = "Object of class Closure could not be converted to string";
    assert_eq!(
        out.stdout,
        format!("concat: {closure}\necho: {closure}\nstill running\n")
    );
}

/// Verifies a class that DOES publish `__toString` still converts, and raises nothing.
///
/// The control for the rule above: the fix must not turn a working conversion into an Error, and
/// the `may_throw` widening in the effects pass must not change what the program prints.
#[test]
fn test_a_class_with_tostring_still_converts() {
    let out = compile_and_run_capture(
        r#"<?php
class B
{
    public function __toString(): string
    {
        return "bee";
    }
}
$o = new B();
try {
    echo $o, "|", "x" . $o, "|", (string) $o, "\n";
} catch (Error $e) {
    echo "unexpected: ", $e->getMessage(), "\n";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bee|xbee|bee\n");
    assert_eq!(out.diagnostics, "");
}
