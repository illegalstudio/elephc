//! Purpose:
//! Exercises dynamically sized mbstring positional calls through shared argument preparation.
//!
//! Called from:
//! - The focused codegen string suite.
//!
//! Key details:
//! - Runtime-selected lengths prevent static spread expansion from hiding arity errors.
//! - Multiple spreads and throwing later expressions exercise value capture and guard relocation.

use crate::support::*;

/// Preserves excess and missing argument counts as catchable PHP errors while keeping valid calls usable.
#[test]
fn test_mbstring_dynamic_spread_arity() {
    let source = r#"<?php
$args = $argc > 0 ? ["a", "UTF-8", "extra"] : ["a"];
try { var_dump(mb_strlen(...$args)); }
catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
$args = $argc > 0 ? [] : ["a"];
try { var_dump(mb_strlen(...$args)); }
catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
$args = $argc > 0 ? ["猫é", "UTF-8"] : ["a"];
var_dump(mb_strlen(...$args));
$args = $argc > 0 ? [] : ["UTF-8"];
echo count(mb_list_encodings(...$args));
echo ":";
$list = mb_list_encodings(...);
echo count($list(...$args));
$args = $argc > 0 ? [1] : [];
try { mb_list_encodings(...$args); }
catch (ArgumentCountError $error) { echo "\n", $error->getMessage(); }
"#;
    assert_eq!(compile_and_run(source), concat!(
        "mb_strlen() expects at most 2 arguments, 3 given\n",
        "mb_strlen() expects at least 1 argument, 0 given\nint(2)\n79:79\n",
        "mb_list_encodings() expects exactly 0 arguments, 1 given"));
}

/// Copies spread elements before later source effects and retains scalar and array results without casts.
#[test]
fn test_mbstring_dynamic_spread_capture_and_results() {
    let source = r#"<?php
function replace_spread_subject(array &$values): array { $values[0] = "changed"; return ["UTF-8"]; }
function split_arguments(): array { return ["A猫éB", 2, "UTF-8"]; }
$values = [str_repeat("é", 32)];
var_dump(mb_strlen(...$values, ...replace_spread_subject($values)));
echo (string)$values[0], "\n";
$tail = $argc > 0 ? [1, 2, "UTF-8"] : [1];
var_dump(mb_substr("A猫éB", ...$tail));
foreach (mb_str_split(...split_arguments()) as $part) { echo (string)$part, ":"; }
echo "\n";
$empty = $argc > 0 ? [] : ["SJIS"];
mb_internal_encoding("UTF-8");
var_dump(mb_internal_encoding(...$empty));
"#;
    assert_eq!(compile_and_run(source), "int(32)\nchanged\nstring(5) \"猫é\"\nA猫:éB:\nstring(5) \"UTF-8\"\n");
}

/// Keeps strict source parameter rules when a dynamic array contains original Mixed scalar values.
#[test]
fn test_mbstring_dynamic_spread_strict_types() {
    let source = r#"<?php declare(strict_types=1);
$args = $argc > 0 ? [123] : ["valid"];
try { mb_strlen(...$args); }
catch (TypeError $error) { echo $error->getMessage(), "\n"; }
$args = $argc > 0 ? ["猫"] : ["valid"];
var_dump(mb_strlen(...$args));
"#;
    assert_eq!(compile_and_run(source), "mb_strlen(): Argument #1 ($string) must be of type string, int given\nint(1)\n");
}

/// Releases grown argument arrays after normal, arity-error, and later-expression exception exits.
#[test]
fn test_mbstring_dynamic_spread_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = r#"
mb_strlen(...$valid);
try { mb_strlen(...$overflow); } catch (ArgumentCountError) {}
try { mb_strlen(...$overflow, ...stopped_spread()); } catch (RuntimeException) {}
"#.repeat(count);
        let source = format!(r#"<?php
function stopped_spread(): array {{ throw new RuntimeException("later source"); }}
$subject = str_repeat("x", 64); $valid = [$subject, "UTF-8"];
$overflow = array_fill(0, 12, $subject);
{calls}
echo "done";
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "dynamic spread calls retained copied values or grown argument containers");
}

/// Keeps C argument pointers aligned after mixed string allocations fragment the native heap.
#[test]
fn test_mbstring_dynamic_spread_heap_alignment() {
    let source = include_str!("../../../examples/mbstring/main.php");
    assert!(compile_and_run(source).contains("Preview: パン 東\n"));
}

/// Preserves boxed builtin return types inside indexed and associative argument literals.
#[test]
fn test_mbstring_array_literal_result_types() {
    let source = r#"<?php
$indexed = [0, mb_internal_encoding()];
$named = ["offset" => 0, "encoding" => mb_internal_encoding()];
var_dump($indexed, $named);
"#;
    let expected = concat!(
        "array(2) {\n  [0]=>\n  int(0)\n  [1]=>\n  string(5) \"UTF-8\"\n}\n",
        "array(2) {\n  [\"offset\"]=>\n  int(0)\n  [\"encoding\"]=>\n  string(5) \"UTF-8\"\n}\n");
    assert_eq!(compile_and_run(source), expected);
}
