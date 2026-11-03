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
/// The callback is declared in the eval activation that adapts it; PHP's shared declaration is guarded.
#[test]
fn test_eval_callable_argument_owners_retire_abandoned_bridge_frames_bundle() {
    let source = r#"<?php
class NativeSharedPausedCast {
    public function __construct(callable $callback, bool $pause) { echo 'ctor|'; if ($pause) { $callback(); } }
    public function pause(callable $callback): void { echo 'method|'; $callback(); }
    public static function pauseStatic(callable $callback): void { echo 'static|'; $callback(); }
}

echo 'ready|';
for ($mode = 0; $mode < 3; $mode++) {
    $fiber = new Fiber(static function () use ($mode): void {
        echo 'fiber|';
        eval('
            echo "eval|";
            if (!function_exists("evalSharedPausedCast")) {
                function evalSharedPausedCast(): void { echo "callback|"; Fiber::suspend(7); }
            }
            if ($mode === 0) { $consumer = new NativeSharedPausedCast("evalSharedPausedCast", true); }
            elseif ($mode === 1) {
                $consumer = new NativeSharedPausedCast("evalSharedPausedCast", false);
                $consumer->pause("evalSharedPausedCast");
            } else { NativeSharedPausedCast::pauseStatic("evalSharedPausedCast"); }
        ');
    });
    echo 'start|';
    echo $fiber->start(), '|';
    gc_collect_cycles();
    unset($fiber);
}
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "ready|start|fiber|eval|ctor|callback|7|start|fiber|eval|ctor|method|callback|7|start|fiber|eval|static|callback|7|done");
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
    public function __construct(callable $callback, bool $pause) { if ($pause) { $callback(); } }
    public function apply(callable $callback): void { $callback(); }
    public static function applyStatic(callable $callback): void { $callback(); }
}
class NativeCycleMagicConsumer {
    public function __call(string $name, array $arguments): mixed { return $arguments[0](); }
    public static function __callStatic(string $name, array $arguments): mixed { return $arguments[0](); }
}
class NativeCycleOwnedReceiver {
    public mixed $fiber = null;
    public function pause(): void { $this->fiber = Fiber::getCurrent(); Fiber::suspend(9); }
}
function nativeCycleCastFunction(callable $callback): void { $callback(); }
$function = nativeCycleCastFunction(...);
$selectedMode = $argc > 1 ? (int)$argv[1] : -1;
for ($mode = 0; $mode < 10; $mode++) {
    if ($selectedMode >= 0 && $mode !== $selectedMode) { continue; }
    $callback = new NativeCycleCastCallback();
    $fiber = new Fiber(static function () use ($callback, $mode, $function): void {
        eval('
            if ($mode === 0) { $consumer = new NativeCycleCastConsumer($callback, true); }
            elseif ($mode === 1) {
                $consumer = new NativeCycleCastConsumer($callback, false);
                $consumer->apply($callback);
            } elseif ($mode === 2) { NativeCycleCastConsumer::applyStatic($callback); }
            elseif ($mode === 3) { nativeCycleCastFunction($callback); }
            elseif ($mode === 4) {
                $consumer = new NativeCycleMagicConsumer(); $consumer->missing($callback);
            } elseif ($mode === 5) { NativeCycleMagicConsumer::missing($callback); }
            elseif ($mode === 7) { call_user_func($function, $callback); }
            elseif ($mode === 9) { (new NativeCycleOwnedReceiver())->pause(); }
            else {
                if (!class_exists("EvalCycleMagicConsumer", false)) {
                    class EvalCycleMagicConsumer {
                        public static function __callStatic(string $name, array $arguments): mixed {
                            return $arguments[0]();
                        }
                        public function __call(string $name, array $arguments): mixed {
                            return $arguments[0]();
                        }
                    }
                }
                if ($mode === 6) { EvalCycleMagicConsumer::missing($callback); }
                else { $consumer = new EvalCycleMagicConsumer(); $consumer->missing($callback); }
            }
        ');
    });
    $callback->fiber = $fiber;
    echo $fiber->start(), '|';
    unset($callback, $fiber);
    gc_collect_cycles();
}
echo 'done';
"#;
    let argvs: &[&[&str]] = &[&[], &["0"], &["1"], &["2"], &["3"], &["4"],
        &["5"], &["6"], &["7"], &["8"], &["9"]];
    let outputs = compile_and_run_with_heap_debug_per_argv(source, argvs);
    let diagnostics = outputs.iter().zip(argvs).filter(|(output, _)|
        !output.success || !output.stderr.contains("HEAP DEBUG: leak summary: clean"))
        .map(|(output, args)| format!("argv={args:?} status={} stdout={} stderr={}",
            output.status, output.stdout, output.stderr)).collect::<Vec<_>>().join("\n");
    for (index, output) in outputs.iter().enumerate() {
        assert!(output.success, "argv={:?}: {diagnostics}", argvs[index]);
        assert_eq!(output.stdout, if index == 0 { "9|9|9|9|9|9|9|9|9|9|done" }
            else { "9|done" }, "argv={:?}: {diagnostics}", argvs[index]);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "argv={:?}: {diagnostics}", argvs[index]);
    }
}

/// A result held while an internal default's destructor suspends is an edge of that Fiber.
#[test]
fn test_eval_native_result_owner_survives_suspending_argument_cleanup_cycle() {
    let source = r#"<?php
class NativeCleanupPauseArgument {
    public function __destruct() { Fiber::suspend(9); }
}
class NativeCleanupResultFactory {
    public static function result(NativeCleanupPauseArgument $argument = new NativeCleanupPauseArgument()): mixed {
        return Fiber::getCurrent();
    }
}
$fiber = new Fiber(static function (): void {
    eval('$held = NativeCleanupResultFactory::result();');
});
echo $fiber->start(), '|';
unset($fiber);
gc_collect_cycles();
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "9|done");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// An instance result stays owned while its default argument's destructor suspends cleanup.
#[test]
fn test_eval_native_instance_result_owner_survives_suspending_argument_cleanup_cycle() {
    let source = r#"<?php
class NativeInstanceCleanupPauseArgument {
    public function __destruct() { Fiber::suspend(9); }
}
class NativeInstanceCleanupResultFactory {
    public function result(NativeInstanceCleanupPauseArgument $argument = new NativeInstanceCleanupPauseArgument()): mixed {
        return Fiber::getCurrent();
    }
}
$fiber = new Fiber(static function (): void {
    $factory = new NativeInstanceCleanupResultFactory();
    eval('$held = $factory->result();');
});
echo $fiber->start(), '|';
unset($fiber);
gc_collect_cycles();
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "9|done");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Opaque eval can suspend and consume the independently owned value supplied on resume.
#[test]
fn test_eval_static_fiber_suspend_preserves_resume_value_owners() {
    let source = r#"<?php
$source = $argc > 0 ? 'echo Fiber::suspend(7), "|";' : '';
$fiber = new Fiber(static function () use ($source): void { eval($source); });
echo $fiber->start(), '|';
$fiber->resume(9);
unset($fiber, $source);
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "7|9|done");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
