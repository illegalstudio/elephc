//! Purpose:
//! Exercises opaque eval capture calls through the shared native V4 mbstring host.
//!
//! Called from:
//! - The codegen integration harness with the managed Oniguruma fixture.
//!
//! Key details:
//! - PHP source stays opaque so every capture call uses Magician's actual reference adapter.
//! - Cases cover caller aliases, source-order arguments, explicit unpacked references, and exceptions.

use crate::support::*;

/// Activates the provider in AOT while leaving all tested calls inside opaque runtime source.
fn program(body: &str) -> String {
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_match('', ''); $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Preserves optional output, named groups, failure initialization, and pre-initialization errors.
#[test]
fn test_mbstring_regex_capture_eval_public() {
    let body = r#"
var_dump(mb_ereg("bc", "abc"), mb_eregi("bc", "aBC"));
var_dump(mb_ereg("(?<key>a)", "za", $matches));
echo count($matches), ":", $matches[0], ":", $matches[1], ":", $matches["key"], "\n";
var_dump(mb_ereg("x", "a", $matches));
echo count($matches), "\n";
$matches = 7;
try { mb_ereg("", "a", $matches); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
var_dump($matches);
"#;
    assert_eq!(compile_and_run(&program(body)), concat!(
        "bool(true)\nbool(true)\nbool(true)\n3:a:a:a\nbool(false)\n0\n",
        "mb_ereg(): Argument #1 ($pattern) must not be empty\nint(7)\n"));
}

/// Keeps named evaluation order and writes through aliases and eval-declared reference parameters.
#[test]
fn test_mbstring_regex_capture_eval_aliases_and_names() {
    let body = r#"
function input(string $label, string $value): string { echo $label, "\n"; return $value; }
function capture_into(mixed &$matches): bool { return mb_ereg("(?<key>a)", "a", $matches); }
$matches = null;
$alias =& $matches;
var_dump(mb_ereg(matches: $matches, string: input("subject", "za"), pattern: input("pattern", "(?<key>a)")));
echo $alias["key"], "\n";
var_dump(capture_into($alias));
echo $matches[0], "\n";
$capture = "mb_eregi";
var_dump($capture("a", "A", $matches));
echo $alias[0], "\n";
"#;
    assert_eq!(compile_and_run(&program(body)), "subject\npattern\nbool(true)\na\nbool(true)\na\nbool(true)\nA\n");
}

/// Preserves explicitly referenced output elements during positional and named unpacking.
#[test]
fn test_mbstring_regex_capture_eval_spread_references() {
    let body = r#"
$matches = null;
var_dump(mb_ereg(...["(?<key>a)", "za", &$matches]));
echo $matches["key"], "\n";
$args = ["matches" => &$matches, "string" => "A", "pattern" => "a"];
var_dump(mb_eregi(...$args));
echo $matches[0], "\n";
"#;
    assert_eq!(compile_and_run(&program(body)), "bool(true)\na\nbool(true)\nA\n");
}

/// Publishes through persistent aliases while an eval destructor changes settings and raises an exception.
#[test]
fn test_mbstring_regex_capture_eval_destructor() {
    let body = r#"
class EvalCaptureOld {
    public function __construct(public Closure $observe, public bool $fail) {}
    public function __destruct() {
        ($this->observe)();
        mb_regex_set_options("i");
        if ($this->fail) { throw new RuntimeException("eval capture"); }
    }
}

function run_capture(mixed $matches, bool $fail): void {
    $alias =& $matches;
    $observe = function() use (&$alias): void {
        echo $alias === null ? "visible:null\n" : "visible:other\n";
        $alias = ["during" => "callback"];
    };
    $matches = new EvalCaptureOld($observe, $fail);
    mb_regex_set_options("r");
    try { var_dump(mb_ereg("(?<key>a)", "A", $alias)); }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo count($matches), ":", $alias[0], ":", $matches["key"], ":", $alias[1], "\n";
}
run_capture(null, false);
run_capture(null, true);
echo "done\n";
"#;
    assert_eq!(compile_and_run(&program(body)), concat!(
        "visible:null\nbool(true)\n3:A:A:A\n",
        "visible:null\ncaught:eval capture\n3:A:A:A\ndone\n"));
}

/// Releases argument owners, reference pins, capture arrays, and caught validation errors after repeated calls.
#[test]
fn test_mbstring_regex_capture_eval_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = concat!(
            "mb_ereg($pattern, $subject, $matches);\n",
            "mb_eregi(matches: $alias, string: $subject, pattern: $pattern);\n",
            "mb_ereg(...$arguments);\n",
            "try { mb_ereg($invalid, $subject, $matches); } catch (ValueError $error) {}\n",
        ).repeat(count);
        let body = format!(r#"
$pattern = "(?<key>a)";
$subject = "a";
$invalid = "";
$matches = null;
$alias =& $matches;
$arguments = [$pattern, $subject, &$matches];
{calls}
echo $alias["key"];
"#);
        let output = compile_and_run_with_gc_stats(&program(&body));
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "a");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "eval capture calls retained additional native owners");
}

/// Copies call_user_func outputs, warns before validation, and preserves explicit call-array references.
#[test]
fn test_mbstring_regex_capture_eval_callback_values() {
    let body = r#"
function extra_capture_argument(): string { echo "extra\n"; return "extra"; }
var_dump(call_user_func("mb_ereg", "a", "a"));
$matches = 7;
$alias =& $matches;
var_dump(call_user_func("mb_ereg", "a", "a", $matches));
var_dump($alias);
var_dump(call_user_func_array("mb_eregi", ["matches" => &$matches, "string" => "A", "pattern" => "a"]));
echo $alias[0], "\n";
$matches = 9;
try { call_user_func("mb_ereg", "", "a", $alias); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
var_dump($matches);
try { call_user_func("mb_ereg", "a", "a", null, extra_capture_argument()); }
catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
"#;
    let output = compile_and_run_capture(&program(body));
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "bool(true)\nbool(true)\nint(7)\nbool(true)\nA\n",
        "mb_ereg(): Argument #1 ($pattern) must not be empty\nint(9)\nextra\n",
        "mb_ereg() expects at most 3 arguments, 4 given\n"));
    assert_eq!(output.stderr,
        "Warning: mb_ereg(): Argument #3 ($matches) must be passed by reference, value given\n".repeat(3));
}

/// Releases the original temporary output before capture initialization can invoke its destructor.
/// Validation-error cleanup uses PHP with zend.exception_ignore_args=1 as the non-retaining trace oracle.
#[test]
fn test_mbstring_regex_capture_eval_callback_destructor() {
    let body = r#"
class CallbackCaptureOld {
    public function __construct(public bool $fail = false) {}
    public function __destruct() {
        echo "old\n";
        mb_regex_set_options("i");
        if ($this->fail) { throw new RuntimeException("callback capture"); }
    }
}
mb_regex_set_options("r");
var_dump(call_user_func("mb_ereg", "a", "A", new CallbackCaptureOld()));
try { call_user_func("mb_ereg", "a", "A", new CallbackCaptureOld(true)); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
try { call_user_func("mb_ereg", "", "A", new CallbackCaptureOld()); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
echo "done\n";
"#;
    let output = compile_and_run_capture(&program(body));
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!("old\nbool(true)\nold\ncallback capture\nold\n",
        "mb_ereg(): Argument #1 ($pattern) must not be empty\ndone\n"));
    assert_eq!(output.stderr,
        "Warning: mb_ereg(): Argument #3 ($matches) must be passed by reference, value given\n".repeat(3));
}

/// Balances temporary callback wrappers and explicit call-array reference pins on success and errors.
#[test]
fn test_mbstring_regex_capture_eval_callback_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = concat!(
            "call_user_func($callback, $pattern, $subject, $matches);\n",
            "try { call_user_func($callback, $invalid, $subject, $matches); } catch (ValueError $error) {}\n",
            "call_user_func_array($callback, $arguments);\n",
        ).repeat(count);
        let body = format!(r#"
$callback = "mb_ereg";
$pattern = "(?<key>a)";
$subject = "a";
$invalid = "";
$matches = null;
$arguments = [$pattern, $subject, &$matches];
{calls}
echo $matches["key"];
"#);
        let output = compile_and_run_with_gc_stats(&program(&body));
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "a");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "callback capture calls retained temporary reference owners");
}
