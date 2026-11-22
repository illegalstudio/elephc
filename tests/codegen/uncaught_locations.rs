//! Purpose:
//! Locations of uncaught and caught engine errors: a binding `TypeError` raised in the callee's
//! frame, and `Call to a member function m() on null`.
//!
//! Oracle: every expected text below is `php -n` output with the script path replaced by `F`.

use crate::support::*;

/// Replaces every absolute path ending in `test.php` with `F`, so an output can be compared with
/// php's byte for byte apart from where the script lives.
fn strip_script_paths(output: &str) -> String {
    let mut out = String::new();
    let mut rest = output;
    while let Some(end) = rest.find("test.php") {
        let head = &rest[..end];
        let start = head
            .rfind(|c: char| c.is_whitespace() || c == '\'')
            .map_or(0, |index| index + 1);
        out.push_str(&head[..start]);
        out.push('F');
        rest = &rest[end + "test.php".len()..];
    }
    out.push_str(rest);
    out
}

/// Regression for elephc-nsbi: an uncaught `TypeError` from a user function's parameter type is
/// raised inside the callee, so php names the call site in the message, closes it with `and defined
/// in F:<declaration line>`, lists the callee as frame `#0 F(<call line>): f('x')` and reports
/// `thrown in F on line <declaration line>`.
#[test]
fn test_uncaught_user_function_argument_type_error_is_raised_in_the_callee() {
    let err = compile_and_run_expect_failure("<?php\nfunction f(int $i) {}\nf(\"x\");\n");
    let err = strip_script_paths(&err);
    assert!(
        err.contains(
            "Fatal error: Uncaught TypeError: f(): Argument #1 ($i) must be of type int, string given, called in F on line 3 and defined in F:2\nStack trace:\n#0 F(3): f('x')\n#1 {main}\n  thrown in F on line 2"
        ),
        "unexpected report: {err:?}"
    );
}

/// The caught side of the same defect: `getLine()` is the declaration line, `getTrace()` holds the
/// callee frame, `getMessage()` keeps php's `called in` clause without `and defined`, and
/// `(string)$e` adds `and defined in`.
#[test]
fn test_caught_user_function_argument_type_error_is_located_in_the_callee() {
    let out = compile_and_run(
        "<?php\nfunction f(int $i){\n  return 1;\n}\ntry { f(\"x\"); } catch (TypeError $t) {\n echo $t->getLine(),\"\\n\"; echo count($t->getTrace()),\"\\n\"; echo $t->getTraceAsString(),\"\\n\"; echo $t->getMessage(),\"\\n\"; echo (string)$t, \"\\n\"; }\n",
    );
    let out = strip_script_paths(&out);
    assert_eq!(
        out,
        "2\n1\n#0 F(5): f('x')\n#1 {main}\nf(): Argument #1 ($i) must be of type int, string given, called in F on line 5\nTypeError: f(): Argument #1 ($i) must be of type int, string given, called in F on line 5 and defined in F:2\nStack trace:\n#0 F(5): f('x')\n#1 {main}\n"
    );
}

/// The run-time int-coercion path: a string only the call knows is refused inside the callee too.
/// A numeric string still binds.
#[test]
fn test_uncaught_runtime_string_at_int_parameter_is_raised_in_the_callee() {
    let err = compile_and_run_expect_failure(
        "<?php\nfunction f(int $i){\n  return 1;\n}\n$v = \"a\" . $argc;\nf($v);\n",
    );
    let err = strip_script_paths(&err);
    assert!(
        err.contains(
            "Fatal error: Uncaught TypeError: f(): Argument #1 ($i) must be of type int, string given, called in F on line 6 and defined in F:2\nStack trace:\n#0 F(6): f('a1')\n#1 {main}\n  thrown in F on line 2"
        ),
        "unexpected report: {err:?}"
    );
    let out = compile_and_run(
        "<?php\nfunction f(int $i){\n  return $i + 1;\n}\n$v = \"4\" . ($argc - 1);\necho f($v);\n",
    );
    assert_eq!(out, "41");
}

/// Regression for elephc-u5gi: an uncaught `Call to a member function foo() on null` carried no
/// location, so the report lacked ` in F:3` and the `thrown in` line.
#[test]
fn test_uncaught_method_call_on_null_reports_its_location() {
    let err = compile_and_run_expect_failure("<?php\n$o = null;\n$o->foo();\n");
    let err = strip_script_paths(&err);
    assert!(
        err.contains(
            "Fatal error: Uncaught Error: Call to a member function foo() on null in F:3\nStack trace:\n#0 {main}\n  thrown in F on line 3"
        ),
        "unexpected report: {err:?}"
    );
}

/// The caught form answers the call's own line.
#[test]
fn test_caught_method_call_on_null_has_the_call_line() {
    let out = compile_and_run(
        "<?php\n$o = null;\ntry {\n  $o->foo();\n} catch (Error $e) { echo $e->getLine(), \"|\", $e->getMessage(); }\n",
    );
    assert_eq!(out, "4|Call to a member function foo() on null");
}
