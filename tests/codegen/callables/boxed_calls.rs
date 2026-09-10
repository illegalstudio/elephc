//! Purpose:
//! Exercises direct calls through boxed values read from declared PHP arrays.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - Unknown signatures must use descriptor argument binding and retain callback owners.

use crate::support::*;

/// Variable and expression calls dispatch every boxed callable shape after the source is released.
#[test]
fn test_boxed_array_reads_dispatch_direct_callable_shapes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boxedGreeting(string $name): string { return "function:" . $name; }
class BoxedGreeter {
    public function greet(string $name): string { return "method:" . $name; }
    public static function staticGreet(string $name): string { return "static:" . $name; }
    public function __invoke(string $name): string { return "object:" . $name; }
}
function boxedCallbacks(string $prefix): array {
    $object = new BoxedGreeter();
    return [
        function(string $name) use ($prefix): string { return $prefix . $name; },
        "boxedGreeting", [$object, "greet"], ["BoxedGreeter", "staticGreet"], $object
    ];
}
$callbacks = boxedCallbacks(str_repeat("c", 3));
$closure = $callbacks[0];
$function = $callbacks[1];
$method = $callbacks[2];
$static = $callbacks[3];
$object = $callbacks[4];
echo $callbacks[0]("expr"), "|";
unset($callbacks);
echo $closure("local"), "|", $function("name"), "|", $method("name"), "|";
echo $static("name"), "|", $object("name");
unset($closure, $function, $method, $static, $object);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "cccexpr|ccclocal|function:name|method:name|static:name|object:name", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed direct calls preserve named binding, positional unpacking and by-reference arguments.
#[test]
fn test_boxed_direct_calls_bind_named_spread_and_reference_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boxedCallTargets(): array {
    return [
        function(int $left, int $right): int { return $left * 10 + $right; },
        function(int &$value): void { $value += 7; }
    ];
}
$callbacks = boxedCallTargets();
$callback = $callbacks[0];
echo $callback(right: 2, left: 1), "|", $callbacks[0](...[3, 4]), "|";
$value = 5;
$callbacks[1]($value);
echo $value;
unset($callback, $callbacks);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "12|34|12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
