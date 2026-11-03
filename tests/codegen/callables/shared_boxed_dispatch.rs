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
$arguments = ['before']; $value =& $arguments[0];
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
        catch (TypeError $guard) { echo $error->getMessage(), ':', $value, '|'; }
    }
}
runRetiredForeachObject([7]);
"#);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "retired:7|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Native and eval callbacks keep their context and retire one descriptor through both bridges.
#[test]
fn test_eval_callable_argument_shared_bridge_contexts_bundle() {
    let source = r#"<?php
class NativeSharedCastTarget {
    public function __invoke(int $value): int { return $value + 1; }
}
class NativeSharedCastConsumer {
    private mixed $callback;
    public function __construct(callable $callback) { $this->callback = $callback; }
    public function saved(int $value): int { $callback = $this->callback; return $callback($value); }
    public function apply(callable $callback, int $value): int { return $callback($value); }
    public static function applyStatic(callable $callback, int $value): int { return $callback($value); }
    public function reject(callable $first, callable $second): void { throw new Exception('rejected'); }
}
$nativeCallback = new NativeSharedCastTarget();
$code = $argc > 0 ? '
function evalSharedCastCallback(int $value): int { return $value + 2; }
class EvalSharedCastTarget {
    public function __invoke(int $value): int { return $value + 3; }
}
$callbacks = [$nativeCallback, "evalSharedCastCallback", new EvalSharedCastTarget()];
foreach ($callbacks as $callback) {
    $consumer = new NativeSharedCastConsumer($callback);
    echo $consumer->saved(1), ":", $consumer->apply($callback, 2), ":",
        NativeSharedCastConsumer::applyStatic($callback, 3), "|";
    try { $consumer->reject($callback, $callback); }
    catch (Exception $error) { echo $error->getMessage(), "|"; }
    unset($error);
    unset($consumer);
}
unset($callbacks, $callback);
' : '';
eval($code);
unset($nativeCallback, $code);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "2:3:4|rejected|3:4:5|rejected|4:5:6|rejected|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Cancelling suspended native constructor, instance and static bridges retires adapted callbacks.
#[test]
fn test_eval_callable_argument_owners_retire_abandoned_bridge_frames_bundle() {
    let source = r#"<?php
class NativeSharedPausedCast {
    public function __construct(callable $callback, bool $pause) { if ($pause) { $callback(); } }
    public function pause(callable $callback): void { $callback(); }
    public static function pauseStatic(callable $callback): void { $callback(); }
}

eval('function evalSharedPausedCast(): void { Fiber::suspend(7); }');
for ($mode = 0; $mode < 3; $mode++) {
    $fiber = new Fiber(static function () use ($mode): void {
        eval('
            if ($mode === 0) { $consumer = new NativeSharedPausedCast("evalSharedPausedCast", true); }
            elseif ($mode === 1) {
                $consumer = new NativeSharedPausedCast("evalSharedPausedCast", false);
                $consumer->pause("evalSharedPausedCast");
            } else { NativeSharedPausedCast::pauseStatic("evalSharedPausedCast"); }
        ');
    });
    echo $fiber->start(), '|';
    gc_collect_cycles();
    unset($fiber);
}
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "7|7|7|done");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// A saved bridge descriptor contributes a Fiber edge rather than an artificial external root.
#[test]
fn test_eval_callable_argument_owner_fiber_cycle_is_collected() {
    let source = r#"<?php
class NativeCycleCastCallback {
    public mixed $fiber = null;
    public function __invoke(): void { Fiber::suspend(9); }
}
class NativeCycleCastConsumer {
    public function apply(callable $callback): void { $callback(); }
}
$callback = new NativeCycleCastCallback();
$fiber = new Fiber(static function () use ($callback): void {
    eval('$consumer = new NativeCycleCastConsumer(); $consumer->apply($callback);');
});
$callback->fiber = $fiber;
echo $fiber->start(), '|';
unset($callback, $fiber);
gc_collect_cycles();
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "9|done");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
