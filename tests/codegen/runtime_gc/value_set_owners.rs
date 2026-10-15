//! Purpose:
//! Heap regressions for values copied by array difference and intersection helpers.
//!
//! Called from:
//! - The codegen integration test harness through `runtime_gc`.
//!
//! Key details:
//! - Read or invoke survivors after an owned literal/source has been released.

use crate::support::compile_and_run_with_heap_debug;

/// PHP rejects a Closure in the first diff array even when the comparison array is empty.
#[test]
fn test_value_set_diff_rejects_callable_with_empty_comparison() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$callback = function(int $x): int { return $x + 1; };
try {
    $result = array_diff(["keep" => $callback], []);
    echo "accepted\n";
    unset($result);
} catch (Error $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
    unset($error);
}
unset($callback);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Error:Object of class Closure could not be converted to string\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An associative difference owns callable values copied out of a temporary literal.
#[test]
fn test_value_set_assoc_diff_retains_callable_literal() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function filtered(int $n): int {
    $result = array_diff_assoc(["keep" => function(int $x) use ($n): int { return $n + $x; }], ["other" => 0]);
    $callback = $result["keep"];
    $answer = $callback(7);
    unset($callback, $result);
    return $answer;
}
echo filtered(35);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "42");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested arrays copied from literals remain owned on both assoc helper modes.
#[test]
fn test_value_set_assoc_retains_nested_literal_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$result = array_diff_assoc(["keep" => [42]], ["other" => [99]]);
echo json_encode($result), "\n";
unset($result);
$result = @array_intersect_assoc(["keep" => [42]], ["keep" => [99]]);
echo json_encode($result), "\n";
unset($result);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"keep\":[42]}\n{\"keep\":[42]}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Closure comparisons throw PHP's Error instead of comparing two empty strings.
#[test]
fn test_value_set_rejects_closure_string_comparisons() {
    for operation in ["array_diff", "array_intersect", "array_diff_assoc", "array_intersect_assoc"] {
        let source = format!(r#"<?php
$first = function(int $x): int {{ return $x + 1; }};
$second = function(int $x): int {{ return $x + 2; }};
foreach ([true, false] as $same) {{
    $left = ["copied" => "retained-before-failure", "callback" => $first];
    $right = ["callback" => $same ? $first : $second];
    try {{
        $result = {operation}($left, $right);
        echo "accepted\n";
        unset($result);
    }} catch (Error $error) {{
        echo get_class($error), ":", $error->getMessage(), "\n";
        unset($error);
    }}
    unset($left, $right);
}}
unset($first, $second);
"#);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{operation}: {}", out.stderr);
        assert_eq!(out.stdout, "Error:Object of class Closure could not be converted to string\n".repeat(2));
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{operation}: {}", out.stderr);
    }
}

/// An unused result must not let AST/EIR dead-code elimination remove the catchable Error.
#[test]
fn test_value_set_unused_comparison_keeps_error_effects() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$callback = function(int $x): int { return $x; };
try {
    array_diff_assoc(["callback" => $callback], ["callback" => 1]);
    echo "accepted\n";
} catch (Error $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
    unset($error);
}
unset($callback);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Error:Object of class Closure could not be converted to string\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Left-side Stringable effects occur before rejecting a right-side Closure.
#[test]
fn test_value_set_preserves_left_string_conversion_before_error() {
    for operation in ["array_diff", "array_intersect", "array_diff_assoc", "array_intersect_assoc"] {
        let source = format!(r#"<?php
class SetStringValue {{
    public function __toString(): string {{ echo "cast\n"; return "value"; }}
}}
$callback = function(int $x): int {{ return $x; }};
try {{
    $result = {operation}(["key" => new SetStringValue()], ["key" => $callback]);
    echo "accepted\n";
    unset($result);
}} catch (Error $error) {{
    echo get_class($error), ":", $error->getMessage(), "\n";
    unset($error);
}}
unset($callback);
"#);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{operation}: {}", out.stderr);
        assert_eq!(out.stdout, "cast\nError:Object of class Closure could not be converted to string\n");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{operation}: {}", out.stderr);
    }
}

/// An unused successful comparison preserves user effects and releases its fresh result.
#[test]
fn test_value_set_unused_stringable_result_is_released() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SetUnusedValue {
    public function __toString(): string { echo "cast\n"; return "value"; }
    public function __destruct() { echo "drop\n"; }
}
array_diff_assoc(["key" => new SetUnusedValue()], ["key" => "other"]);
echo "DONE\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "cast\ndrop\nDONE\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
/// A heterogeneous source hash retains a borrowed descriptor before comparisons can throw.
#[test]
fn test_value_set_mixed_source_retains_borrowed_callable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$callback = function(int $x): int { return $x + 1; };
$source = ["string" => "copied", "callback" => $callback];
unset($source);
echo $callback(41);
unset($callback);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "42");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
