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

/// Names discovered at runtime keep otherwise unreachable functions and methods alive.
#[test]
fn test_issue_828_dynamic_first_class_callable_reachability() {
    let out = compile_and_run(r#"<?php
function hidden828(): string { return "function|"; }
function pick828(): string { return "hidden828"; }
$name = $argc > 0 ? "hidden828" : "hidden828";
$function = $name(...);
echo $function();
$factory = pick828()(...);
echo $factory();
class Hidden828 {
    public function ping(): string { return "method"; }
    public function pong(): string { return "other"; }
}
$box = new Hidden828();
$method = $argc > 1 ? "pong" : "ping";
$callback = [$box, $method];
$closure = $callback(...);
echo $closure();
"#);
    assert_eq!(out, "function|function|method");
}

/// Invalid targets throw Error for syntax and TypeError for the explicit PHP factory.
#[test]
fn test_issue_828_dynamic_first_class_callable_errors() {
    let out = compile_and_run(r#"<?php
$name = "missing_callable_828";
try { $f = $name(...); } catch (Error $e) { echo get_class($e), "|"; }
$number = 1;
try { $f = $number(...); } catch (Error $e) { echo get_class($e), "|"; }
$invalid = $argc > 0 ? 1 : "missing_callable_828";
try { $f = $invalid(...); } catch (Error $e) { echo get_class($e), "|"; }
try { $f = Closure::fromCallable($name); } catch (TypeError $e) { echo get_class($e); }
"#);
    assert_eq!(out, "Error|Error|Error|TypeError");
}
