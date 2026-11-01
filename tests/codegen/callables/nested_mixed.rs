//! Purpose:
//! Regression tests for closure arguments whose dynamic values contain nested Mixed wrappers.
//!
//! Called from:
//! - `tests::codegen::callables` through Rust's test harness.
//!
//! Key details:
//! - The fixture mutates an array passed through an indirect closure call, forcing the gradual
//!   array conversion path to peel a wrapper before it preserves PHP array semantics.

use crate::support::*;

/// Verifies an indirect closure call can mutate an array carried by a nested Mixed wrapper.
#[test]
fn test_indirect_closure_array_unset_peels_nested_mixed_wrapper() {
    let out = compile_and_run(
        r#"<?php
class CallbackHolder {
    private Closure $callback;

    public function __construct(Closure $callback) {
        $this->callback = $callback;
    }

    public function apply(mixed $value): mixed {
        return ($this->callback)($value);
    }
}

$holder = new CallbackHolder(static function ($values): array {
    unset($values[0]);

    return $values;
});
$result = $holder->apply(['removed', 'kept']);
echo count($result), ':', $result[1];
"#,
    );
    assert_eq!(out, "1:kept");
}

/// Verifies an array-held dynamic callback retains its runtime result representation.
#[test]
fn test_dynamic_array_slot_callable_preserves_array_result() {
    let out = compile_and_run(
        r#"<?php
class Expression {
    public ?Closure $ifPart = null;
    public ?Closure $thenPart = null;
}

function buildExpressions(array $expressions): array {
    foreach ($expressions as $key => $expression) {
        if ($expression instanceof Expression) {
            $if = $expression->ifPart;
            $then = $expression->thenPart;
            $expressions[$key] = static fn ($value) => $if($value) ? $then($value) : $value;
        }
    }

    return $expressions;
}

$expression = new Expression();
$expression->ifPart = is_array(...);
$expression->thenPart = static fn ($value): array => ['added'];
$callback = buildExpressions([$expression])[0];
$result = $callback(['original']);
echo get_debug_type($result), ':', count($result);
"#,
    );
    assert_eq!(out, "array:1");
}

/// Verifies a function return acquires a Mixed parameter before the caller releases its argument.
#[test]
fn test_mixed_parameter_return_remains_owned_by_the_caller() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function identity(mixed $value): mixed {
    return $value;
}

$source = ['stable'];
$returned = identity($source);
unset($source);
echo get_debug_type($returned), ':', $returned[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "array:stable");
}
