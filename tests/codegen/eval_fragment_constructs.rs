//! Purpose:
//! End-to-end checks for PHP constructs that runtime `eval()` fragments used to reject:
//! the `@` operator, `throw` expressions, `const` statements, by-reference function
//! declarations and trailing commas in parameter and closure `use` lists.
//!
//! Called from:
//! - `cargo test --test codegen_tests_eval eval_fragment_constructs`.
//!
//! Key details:
//! - The fragment is selected at run time, so it executes in the Magician interpreter
//!   against the generated runtime's real reporting mask and warning dispatcher.
//! - One fixture covers every construct, because each opaque-eval compile lowers the whole
//!   builtin catalog.

use crate::support::*;

/// Runs the constructs through the interpreter bridge and checks output and diagnostics.
#[test]
fn test_eval_fragment_constructs_run_on_the_interpreter_bridge() {
    let output = compile_and_run_capture(r#"<?php
function aot_missing_key(array $values): mixed { return $values['missing']; }
$code = $argc > 0 ? <<<'EVAL'
error_reporting(E_ALL);
$quiet = @$eval_missing_variable;
echo $quiet === null ? 'null' : 'set', '|';
echo @aot_missing_key([]) === null ? 'null' : 'set', '|';
echo error_reporting() === E_ALL ? 'restored' : 'lost', '|';
echo $eval_loud_variable, '|';
const EVAL_FRAGMENT_LIMIT = 3, EVAL_FRAGMENT_DOUBLE = EVAL_FRAGMENT_LIMIT * 2;
const EVAL_FRAGMENT_LIMIT = 4;
echo EVAL_FRAGMENT_LIMIT, EVAL_FRAGMENT_DOUBLE, '|';
function &eval_fragment_counter() { static $count = 0; $count++; return $count; }
$reflected = new ReflectionFunction('eval_fragment_counter');
echo eval_fragment_counter(), $reflected->returnsReference() ? 'R' : 'r', '|';
$pair = function ($left, $right,) use ($quiet,) { return $left . $right; };
echo $pair('a', 'b'), '|';
$ok = false;
$ok || throw new RuntimeException('thrown from eval');
EVAL : '';
try {
    eval($code);
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
"#);

    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "null|null|restored||36|1R|ab|thrown from eval\n");
    assert_eq!(output.stderr.matches("Undefined variable $eval_loud_variable").count(), 1, "{}", output.stderr);
    assert!(!output.stderr.contains("eval_missing_variable"), "{}", output.stderr);
    assert!(!output.stderr.contains("Undefined array key"), "{}", output.stderr);
    assert!(
        output.stderr.contains("Constant EVAL_FRAGMENT_LIMIT already defined, this will be an error in PHP 9"),
        "{}",
        output.stderr,
    );
}

/// Runs the literal `eval()` reproductions from the issues, whichever path compiles them.
#[test]
fn test_eval_fragment_constructs_issue_reproductions() {
    let output = compile_and_run_capture(r#"<?php
eval('var_dump(@strlen("abc"));');
eval('const E1 = 1; echo E1, "\n";');
eval('echo match (1) { 1, => "a", default => "b" }, "\n";');
eval('$a = 1; $f = function () use ($a,) { return $a; }; echo $f(), "\n";');
eval('function &f() { static $x = 1; return $x; } var_dump((new ReflectionFunction("f"))->returnsReference());');
eval('$ok = false; try { $ok || throw new Exception("x"); } catch (Exception $e) { echo $e->getMessage(), "\n"; }');
"#);

    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "int(3)\n1\na\n1\nbool(true)\nx\n");
    assert_eq!(output.stderr, "");
}
