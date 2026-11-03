//! Purpose:
//! Covers shared descriptor resolution for unrestricted boxed callbacks.
//!
//! Called from:
//! - Executable callable and heap ownership regressions in CI.
//!
//! Key details:
//! - Named arguments, reference arguments and receiver captures keep their normal invoker ABI.
//! - A throwing callback retires temporary descriptors before the caller catches it.

use crate::support::*;

/// Shared selection preserves every native callback shape and cleanup after a throw.
#[test]
fn test_shared_boxed_callback_dispatch_preserves_arguments_and_owners() {
    let source = r#"<?php
class SharedBoxedCallback {
    public function __construct(public string $label) {}
    public function instance(string $tail): string { return $this->label . $tail; }
    public function __invoke(string $tail): string { return $this->label . $tail; }
    public static function staticCall(string $tail): string { return 'static' . $tail; }
}

function sharedBoxedFunction(string $tail): string { return 'function' . $tail; }
function sharedBoxedReference(mixed &$value): string { $value = 'changed'; return $value; }
function sharedBoxedThrow(): void { throw new Exception('caught'); }
function invokeSharedBoxed(mixed $callback, array $arguments): mixed {
    return call_user_func_array($callback, $arguments);
}
$object = new SharedBoxedCallback('receiver');
$callbacks = ['sharedBoxedFunction', [$object, 'instance'],
    ['SharedBoxedCallback', 'staticCall'], $object, sharedBoxedFunction(...)];
foreach ($callbacks as $callback) { echo invokeSharedBoxed($callback, ['tail' => '!']), '|'; }
$value = 'before'; $arguments = [&$value];
echo invokeSharedBoxed('sharedBoxedReference', $arguments), ':', $value, '|';
try { invokeSharedBoxed('sharedBoxedThrow', []); }
catch (Exception $error) { echo $error->getMessage(), '|'; }
unset($arguments, $value, $callback, $callbacks, $object, $error);
"#;
    for optimize in [true, false] {
        let output = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(output.success, "ir-opt={optimize}: stdout={} stderr={}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "function!|receiver!|static!|receiver!|function!|changed:changed|caught|");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}

/// Unknown foreach values are guarded after publishing a new binding and retiring its predecessor.
#[test]
fn test_foreach_retirement_mixed_object_argument_throws_inside_handler() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class RetiredForeachObject {
    public function __destruct() { throw new Exception('retired'); }
}
function needRetiredForeachObject(RetiredForeachObject $value): void { echo 'unreachable|'; }
function runRetiredForeachObject(array $values): void {
    $value = new RetiredForeachObject();
    try { foreach ($values as $value) { return; } }
    catch (Exception $error) {
        try { needRetiredForeachObject($value); }
        catch (TypeError $guard) { echo $error->getMessage(), ':', get_debug_type($value), '|'; }
    }
}
runRetiredForeachObject([7]);
"#);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "retired:int|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
