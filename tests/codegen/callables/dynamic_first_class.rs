//! Purpose:
//! Regression coverage for dynamic first-class callable syntax (issue 828).
//!
//! Called from:
//! - The codegen integration test suite.
//!
//! Key details:
//! - Exercises callable conversion, capture lifetime, and single evaluation.

use crate::support::*;

/// The issue's wrapper returns a Closure that preserves its captured environment.
#[test]
fn test_issue_828_dynamic_first_class_callable_wrapper() {
    let out = compile_and_run(r#"<?php
function wrap(callable $method): Closure { return $method(...); }
$prefix = "hello ";
$original = function(string $name) use ($prefix): string { return $prefix . $name; };
$wrapped = wrap($original);
unset($original);
echo get_class($wrapped), "|", $wrapped("Ada"), "|";
$builtin = "strtoupper";
$upper = $builtin(...);
echo $upper("ok");
"#);
    assert_eq!(out, "Closure|hello Ada|OK");
}

/// Postfix and grouped expressions evaluate once before the resulting closure is called.
#[test]
fn test_issue_828_dynamic_first_class_callable_expressions() {
    let out = compile_and_run(r#"<?php
namespace Issue828;
class Closure {}
function factory(): callable {
    echo "created|";
    return fn(int $n): int => $n + 2;
}
$created = factory()(...);
echo $created(3), "|";
$callbacks = [fn(int $n): int => $n * 2];
$element = $callbacks[0](...);
echo $element(4), "|";
$grouped = ($created)(...);
echo $grouped(5), "|", get_class($grouped);
"#);
    assert_eq!(out, "created|5|8|7|Closure");
}

/// Conversion preserves bound receivers and reference parameter writeback.
#[test]
fn test_issue_828_dynamic_first_class_callable_receiver_and_reference() {
    let out = compile_and_run(r#"<?php
class Counter828 {
    public int $offset = 4;
    public function add(int &$value): int { $value += $this->offset; return $value; }
}
$object = new Counter828();
$callback = [$object, "add"];
$closure = $callback(...);
unset($callback);
unset($object);
$value = 3;
echo $closure($value), "|", $value;
"#);
    assert_eq!(out, "7|7");
}
