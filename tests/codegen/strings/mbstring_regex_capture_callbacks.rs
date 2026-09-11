//! Purpose:
//! Exercises mbstring capture output ownership across eval callback invocation surfaces.
//!
//! Called from:
//! - The codegen integration harness using the managed Oniguruma provider.
//!
//! Key details:
//! - Opaque source forces Magician dispatch, including dynamic callback wrappers and reflection.
//! - Destructor changes to regex settings make source-array lifetimes observable in match results.
//! - Repeated successful and failing calls check for retained argument owners.

use crate::support::*;

/// Enables the provider in AOT and keeps the callback calls inside opaque eval source.
fn program(body: &str) -> String {
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_match('', ''); $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Preserves explicit output references while warning and using temporary cells for ordinary array values.
#[test]
fn test_mbstring_regex_capture_callback_array_values() {
    let body = r#"
$matches = 7;
$alias =& $matches;
$args = ["a", "a", $matches];
var_dump(call_user_func_array("mb_ereg", $args));
var_dump($args[2], $alias);
$args = ["matches" => &$matches, "string" => "A", "pattern" => "a"];
var_dump(call_user_func_array("mb_eregi", $args));
echo $alias[0], "\n";
$matches = 9;
var_dump(call_user_func_array("mb_ereg", ["pattern" => "a", "string" => "a", "matches" => null]));
try { call_user_func_array("mb_ereg", ["", "a", $matches]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { call_user_func_array("mb_ereg", ["", "a", &$matches]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
var_dump($matches);
"#;
    let output = compile_and_run_capture(&program(body));
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "bool(true)\nint(7)\nint(7)\nbool(true)\nA\nbool(true)\n",
        "mb_ereg(): Argument #1 ($pattern) must not be empty\n",
        "mb_ereg(): Argument #1 ($pattern) must not be empty\nint(9)\n"));
    assert_eq!(output.stderr,
        "Warning: mb_ereg(): Argument #3 ($matches) must be passed by reference, value given\n".repeat(3));
}

/// Matches PHP's different temporary-array lifetimes for direct syntax, dynamic wrappers, and retained arrays.
#[test]
fn test_mbstring_regex_capture_callback_array_lifetimes() {
    let body = r#"
class CallbackArrayOld {
    public function __destruct() { echo "old\n"; mb_regex_set_options("i"); }
}
echo "direct\n";
mb_regex_set_options("r");
var_dump(call_user_func_array("mb_ereg", ["a", "A", new CallbackArrayOld()]));
echo "dynamic\n";
mb_regex_set_options("r");
$invoke = "call_user_func_array";
var_dump($invoke("mb_ereg", ["a", "A", new CallbackArrayOld()]));
echo "retained\n";
mb_regex_set_options("r");
$args = ["a", "A", new CallbackArrayOld()];
var_dump(call_user_func_array("mb_ereg", $args));
unset($args);
echo "dynamic-value\n";
mb_regex_set_options("r");
$invoke = "call_user_func";
var_dump($invoke("mb_ereg", "a", "A", new CallbackArrayOld()));
echo "done\n";
"#;
    let output = compile_and_run_capture(&program(body));
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "direct\nold\nbool(true)\ndynamic\nold\nbool(false)\n",
        "retained\nbool(false)\nold\ndynamic-value\nold\nbool(false)\ndone\n"));
    assert_eq!(output.stderr,
        "Warning: mb_ereg(): Argument #3 ($matches) must be passed by reference, value given\n".repeat(4));
}

/// Adapts evaluated callback values from reflection, array_map, and nested callback wrappers.
#[test]
fn test_mbstring_regex_capture_callback_value_dispatch() {
    let body = r#"
$reflection = new ReflectionFunction("mb_ereg");
$matches = 7;
var_dump($reflection->invoke("a", "a", $matches));
var_dump($matches);
var_dump($reflection->invokeArgs(["a", "a", &$matches]));
echo $matches[0], "\n";
$matches = 9;
$alias =& $matches;
var_dump($reflection->invoke("a", "a", $matches));
var_dump($alias);
$rows = array_map("mb_ereg", ["a"], ["a"], [null]);
var_dump($rows[0]);
var_dump(call_user_func("call_user_func_array", "mb_ereg", ["a", "a", null]));
$invoke = "call_user_func_array";
var_dump($invoke("mb_eregi", ["matches" => &$matches, "string" => "A", "pattern" => "a"]));
echo $matches[0], "\n";
var_dump(("mb_" . "ereg")("b", "b", $matches));
echo $matches[0], "\n";
"#;
    let output = compile_and_run_capture(&program(body));
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "bool(true)\nint(7)\nbool(true)\na\nbool(true)\nint(9)\nbool(true)\nbool(true)\nbool(true)\nA\nbool(true)\nb\n");
    assert_eq!(output.stderr,
        "Warning: mb_ereg(): Argument #3 ($matches) must be passed by reference, value given\n".repeat(4));
}

/// Preserves operand evaluation, Stringable conversion, destructor order, and scalar scratch lifetimes.
#[test]
fn test_mbstring_regex_capture_callback_name_order() {
    let body = r#"
class CallbackNamePart {
    public function __construct(public string $label, public bool $fail = false) { echo "new:", $label, "\n"; }
    public function __toString(): string {
        echo "string:", $this->label, "\n";
        if ($this->fail) { throw new RuntimeException("name conversion"); }
        return $this->label === "left" ? "mb_" : "ereg";
    }
    public function __destruct() { echo "drop:", $this->label, "\n"; }
}
function callback_arguments(): array { echo "arguments\n"; return ["a", "a", null]; }
var_dump(call_user_func_array(new CallbackNamePart("left") . new CallbackNamePart("right"), callback_arguments()));
try { call_user_func_array(new CallbackNamePart("left") . new CallbackNamePart("right", true), callback_arguments()); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
echo (12 . 34), ":", (true . 56), "\n";
class CallbackCondition { public function __destruct() { echo "condition\n"; } }
function selected_callback_name(): string { echo "selected\n"; return "mb_ereg"; }
var_dump(call_user_func_array(new CallbackCondition() ? selected_callback_name() : "missing", ["a", "a"]));
"#;
    let output = compile_and_run_capture(&program(body));
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "new:left\nnew:right\nstring:left\nstring:right\ndrop:left\ndrop:right\narguments\nbool(true)\n",
        "new:left\nnew:right\nstring:left\nstring:right\ndrop:left\ndrop:right\nname conversion\n1234:156\n",
        "condition\nselected\nbool(true)\n"));
    assert_eq!(output.stderr, "Warning: mb_ereg(): Argument #3 ($matches) must be passed by reference, value given\n");
}

/// Compares residual native allocation counts after one and twenty-four invocations of a callback surface.
fn assert_callback_ownership(call: &str) {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = call.repeat(count);
        let body = format!(r#"
class ComputedCallbackPart {{
    public function __construct(public bool $first) {{}}
    public function __toString(): string {{ return $this->first ? "mb_" : "ereg"; }}
}}
$callback = "mb_ereg";
$array_invoke = "call_user_func_array";
$value_invoke = "call_user_func";
$pattern = "(?<key>a)";
$subject = "a";
$invalid = "";
$matches = null;
$plain = [$pattern, $subject, null];
$references = [$pattern, $subject, &$matches];
{calls}
echo "done";
"#);
        let output = compile_and_run_with_gc_stats(&program(&body));
        assert!(output.success, "{call}: {}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "callback invocation retained argument owners: {call}");
}

/// Releases callback-array snapshots and temporary output references for ordinary values.
#[test]
fn test_mbstring_regex_capture_callback_array_ownership() {
    assert_callback_ownership("call_user_func_array($callback, $plain);\n");
}

/// Releases call-array snapshots without retaining additional pins on the caller's output reference.
#[test]
fn test_mbstring_regex_capture_callback_reference_array_ownership() {
    assert_callback_ownership("call_user_func_array($callback, $references);\n");
}

/// Releases both the dynamic wrapper's argument owners and the inner call-array snapshot.
#[test]
fn test_mbstring_regex_capture_callback_dynamic_array_ownership() {
    assert_callback_ownership("$array_invoke($callback, $plain);\n");
}

/// Releases both layers of copied values through a dynamic call_user_func wrapper.
#[test]
fn test_mbstring_regex_capture_callback_dynamic_value_ownership() {
    assert_callback_ownership("$value_invoke($callback, $pattern, $subject, $matches);\n");
}

/// Releases concatenated callback names and their source operands before dropping argument-array snapshots.
#[test]
fn test_mbstring_regex_capture_callback_computed_name_ownership() {
    assert_callback_ownership("call_user_func_array('mb_' . 'ereg', [$pattern, $subject, null]);\n");
}

/// Releases Stringable callback fragments, converted strings, and copied callback argument arrays.
#[test]
fn test_mbstring_regex_capture_callback_stringable_name_ownership() {
    assert_callback_ownership("call_user_func_array(new ComputedCallbackPart(true) . new ComputedCallbackPart(false), $plain);\n");
}

/// Releases a computed dynamic callee while keeping direct reference arguments bound to caller storage.
#[test]
fn test_mbstring_regex_capture_callback_computed_callee_ownership() {
    assert_callback_ownership("('mb_' . 'ereg')($pattern, $subject, $matches);\n");
}

/// Releases array literals and temporary output references after native parameter validation fails.
#[test]
fn test_mbstring_regex_capture_callback_array_error_ownership() {
    assert_callback_ownership("try { call_user_func_array($callback, [$invalid, $subject, null]); } catch (ValueError $error) {}\n");
}
