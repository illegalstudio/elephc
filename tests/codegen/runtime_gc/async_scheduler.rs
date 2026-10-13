//! Purpose:
//! Heap-debug acceptance tests for Async scheduler task and reactor ownership.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs`.
//!
//! Key details:
//! - Success, cancellation, failure, and descriptor readiness must release every
//!   task, Fiber, outcome, registration, captured value, and scheduler array.

use crate::support::compile_and_run_with_heap_debug;

/// Reads the complete compact Throwable payload after a rejected Fiber state transition.
#[test]
fn fiber_state_error_initializes_code_and_previous() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(static function (): void {});
try {
    $fiber->resume();
} catch (FiberError $error) {
    echo get_class($error), "|", $error->getMessage(), "|", $error->getCode(), "|";
    echo $error->getPrevious() === null ? "none" : "previous";
}
unset($fiber);
"#,
    );
    assert_clean(out, "FiberError|Cannot resume a fiber that is not suspended|0|none");
}

/// Requires both the expected native output and a clean heap-debug teardown for each fixture.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Retires successful task results, captured arrays and the drained scheduler state.
#[test]
fn async_success_releases_task_results_and_captures() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$value = run(function (TaskGroup $tasks): string {
    $payload = ["answer" => 42];
    $child = $tasks->spawn(function () use ($payload): string {
        return "value:" . $payload["answer"];
    });
    return $child->await();
});
echo $value;
unset($value);
"#,
    );
    assert_clean(out, "value:42");
}

/// Reclaims repeated empty Async scopes after an explicit collection checkpoint.
#[test]
fn async_empty_scope_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$i = 0;
while ($i < 2) {
    $result = run(function (TaskGroup $tasks): void {});
    unset($result);
    $i = $i + 1;
}
\__elephc_async_gc_collect();
echo "empty";
"#,
    );
    assert_clean(out, "empty");
}

/// Keeps unused injected declarations free of runtime allocations.
#[test]
fn async_prelude_without_runtime_entry_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
echo "unused";
"#,
    );
    assert_clean(out, "unused");
}

/// Releases manually constructed scope objects after detachment without creating a Fiber.
#[test]
fn async_internal_objects_without_a_fiber_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__Scheduler;
use Elephc\Async\__ScopeState;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$scope->detach();
unset($group);
unset($scope);
unset($scheduler);
unset($state);
\__elephc_async_gc_collect();
echo "objects";
"#,
    );
    assert_clean(out, "objects");
}

/// Releases the runner closure, argument array and Fiber wrapper used by Async task starts.
#[test]
fn fiber_wrapper_shape_used_by_async_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FiberArgumentHolder {}

function driveFiber(callable $body, array $args): void {
    $fiber = new Fiber(function () use ($body, $args): mixed {
        return $body(...$args);
    });
    $signal = $fiber->start();
    unset($signal);
    unset($fiber);
    unset($body);
    unset($args);
}

$holder = new FiberArgumentHolder();
driveFiber(function (FiberArgumentHolder $value): void {}, [$holder]);
unset($holder);
echo "fiber";
"#,
    );
    assert_clean(out, "fiber");
}

/// Reclaims a Fiber/argument/owner cycle after its retaining array entry is cleared.
#[test]
fn fiber_start_argument_cycle_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FiberOwner {
    public array $fibers = [];
}
final class FiberArgument {
    public function __construct(public FiberOwner $owner) {}
}

$owner = new FiberOwner();
$argument = new FiberArgument($owner);
$fiber = new Fiber(function (FiberArgument $value): void {});
$owner->fibers[] = $fiber;
$signal = $fiber->start($argument);
unset($signal);
$owner->fibers[0] = null;
unset($fiber);
unset($argument);
unset($owner);
echo "cycle";
"#,
    );
    assert_clean(out, "cycle");
}

/// Releases a retained Mixed argument when a second Fiber start is rejected.
#[test]
fn rejected_second_fiber_start_releases_mixed_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FiberStartPayload {}

function start_terminated_fiber(Fiber $fiber, mixed $payload): void {
    try {
        $fiber->start($payload);
    } catch (FiberError $error) {
    }
}

$fiber = new Fiber(static function (mixed $payload): void {});
$fiber->start(new FiberStartPayload());
$payload = new FiberStartPayload();
start_terminated_fiber($fiber, $payload);
unset($payload);
unset($fiber);
echo "fiber";
"#,
    );
    assert_clean(out, "fiber");
}

/// Retires Mixed and typed call-result arguments on a rejected second Fiber start.
#[test]
fn rejected_second_fiber_start_releases_mixed_call_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FiberStartCallPayload {}
function make_mixed_fiber_start_payload(): mixed {
    return new FiberStartCallPayload();
}
function make_typed_fiber_start_payload(): FiberStartCallPayload {
    return new FiberStartCallPayload();
}
$fiber = new Fiber(static function (mixed $payload): void {});
$fiber->start(new FiberStartCallPayload());
try {
    $fiber->start(make_mixed_fiber_start_payload());
} catch (FiberError $error) {
    echo "start-error";
}
try {
    $fiber->start(make_typed_fiber_start_payload());
} catch (FiberError $error) {
    echo "-start-error";
}
unset($fiber);
"#,
    );
    assert_clean(out, "start-error-start-error");
}

/// Releases a temporary never-started Fiber when resume raises a state error.
#[test]
fn temporary_fiber_resume_state_error_releases_receiver() {
    let source = r#"<?php
try {
    (new Fiber(static function (): void {}))->resume();
} catch (FiberError $error) {
    echo "resume-error";
}
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert_clean(out, "resume-error");
}

/// Retires a boxed object argument when resume rejects the Fiber state.
#[test]
fn rejected_fiber_resume_releases_boxed_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class ResumePayload {}
$fiber = new Fiber(static function (): void {});
$payload = new ResumePayload();
try {
    $fiber->resume($payload);
} catch (FiberError $error) {
    echo "resume-error";
}
unset($payload);
unset($fiber);
"#,
    );
    assert_clean(out, "resume-error");
}

/// Retires a retained Mixed argument on an invalid Fiber resume.
#[test]
fn rejected_fiber_resume_releases_retained_mixed_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mixed_resume_payload(): mixed {
    return "payload";
}
$fiber = new Fiber(static function (): void {});
$payload = mixed_resume_payload();
try {
    $fiber->resume($payload);
} catch (FiberError $error) {
    echo "resume-error";
}
unset($payload);
unset($fiber);
"#,
    );
    assert_clean(out, "resume-error");
}

/// Releases a fresh Mixed call-result argument when Fiber resume fails.
#[test]
fn rejected_fiber_resume_releases_mixed_call_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class ResumeCallPayload {}
function make_mixed_resume_payload(): mixed {
    return new ResumeCallPayload();
}
$fiber = new Fiber(static function (): void {});
try {
    $fiber->resume(make_mixed_resume_payload());
} catch (FiberError $error) {
    echo "resume-error";
}
unset($fiber);
"#,
    );
    assert_clean(out, "resume-error");
}

/// Releases a fresh typed object argument when Fiber resume fails.
#[test]
fn rejected_fiber_resume_releases_typed_call_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class ResumeObjectPayload {}
$fiber = new Fiber(static function (): void {});
try {
    $fiber->resume(new ResumeObjectPayload());
} catch (FiberError $error) {
    echo "resume-error";
}
unset($fiber);
"#,
    );
    assert_clean(out, "resume-error");
}

/// Retires a temporary Fiber and injected throwable when throw rejects its state.
#[test]
fn rejected_temporary_fiber_throw_releases_receiver_and_throwable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
try {
    (new Fiber(static function (): void {}))->throw(new RuntimeException("rejected"));
} catch (FiberError $error) {
    echo "throw-error";
}
"#,
    );
    assert_clean(out, "throw-error");
}

/// Retires a temporary suspended Fiber and throwable after injected exception escape.
#[test]
fn escaped_temporary_fiber_throw_releases_receiver_and_throwable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function started_suspended_fiber(): Fiber {
    $fiber = new Fiber(static function (): void {
        Fiber::suspend();
    });
    $fiber->start();
    return $fiber;
}
try {
    started_suspended_fiber()->throw(new RuntimeException("escape"));
} catch (RuntimeException $error) {
    echo "escaped";
}
"#,
    );
    assert_clean(out, "escaped");
}

/// Preserves borrowed Fiber/exception owners while retiring the injected call lease.
#[test]
fn escaped_borrowed_fiber_throw_releases_injected_exception() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function started_suspended_fiber(): Fiber {
    $fiber = new Fiber(static function (): void {
        Fiber::suspend();
    });
    $fiber->start();
    return $fiber;
}
$fiber = started_suspended_fiber();
$thrown = new RuntimeException("escape");
try {
    $fiber->throw($thrown);
} catch (RuntimeException $error) {
    echo "escaped";
}
unset($fiber, $thrown, $error);
echo "done";
"#,
    );
    assert_clean(out, "escapeddone");
}

/// Releases a resumed Fiber after its body throws beyond the Fiber boundary.
#[test]
fn escaped_temporary_fiber_resume_releases_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(static function (): void {
    Fiber::suspend();
    throw new Exception();
});
$fiber->start();
try {
    $fiber->resume();
} catch (Exception $error) {
    echo "escaped";
}
unset($fiber, $error);
echo "done";
"#,
    );
    assert_clean(out, "escapeddone");
}

/// Runs the suspended frame's local destructor when its Fiber owner is dropped.
#[test]
fn suspended_fiber_local_object_is_released_when_the_fiber_is_dropped() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class FiberLocalPayload { public function __destruct() { echo "Y"; } }
$fiber = new Fiber(static function (): void {
    $payload = new FiberLocalPayload();
    Fiber::suspend();
});
$fiber->start();
echo "drop";
unset($fiber);
echo "done";
"#,
    );
    assert_clean(out, "dropYdone");
}

/// Releases a returned temporary Fiber when restarting it raises a state error.
#[test]
fn returned_temporary_fiber_start_state_error_releases_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function already_started_fiber(): Fiber {
    $fiber = new Fiber(static function (): void {});
    $fiber->start();
    return $fiber;
}
try {
    already_started_fiber()->start();
} catch (FiberError $error) {
    echo "start-error";
}
"#,
    );
    assert_clean(out, "start-error");
}

/// Releases object elements owned through an array property.
#[test]
fn mixed_property_array_releases_object_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class MixedOwner {
    public array $items = [];
}
final class MixedPayload {}

$owner = new MixedOwner();
$payload = new MixedPayload();
$owner->items[] = $payload;
unset($owner);
unset($payload);
echo "array";
"#,
    );
    assert_clean(out, "array");
}

/// Retires terminated Fiber entries and scope arguments held in private typed arrays.
#[test]
fn private_typed_array_releases_terminated_fiber() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__ScopeState;
use Elephc\Async\__Scheduler;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
final class FiberOwner {
    private array $items = [];
    public function add(mixed $value): void { $this->items[] = $value; }
    public function clear(): void { $this->items[0] = null; }
}

$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$owner = new FiberOwner();
$fiber = new Fiber(function (TaskGroup $tasks): void {});
$signal = $fiber->start($group);
unset($signal);
$owner->add($fiber);
$owner->clear();
$scope->detach();
unset($fiber);
unset($group);
unset($scope);
unset($scheduler);
unset($state);
unset($owner);
echo "private";
"#,
    );
    assert_clean(out, "private");
}

/// Releases ordinary object entries when a private array property is cleared.
#[test]
fn private_typed_array_releases_ordinary_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class ObjectOwner {
    private array $items = [];
    public function add(mixed $value): void { $this->items[] = $value; }
    public function clear(): void { $this->items = []; }
}
final class ObjectPayload {}

$owner = new ObjectOwner();
$payload = new ObjectPayload();
$owner->add($payload);
$owner->clear();
unset($payload);
unset($owner);
echo "object";
"#,
    );
    assert_clean(out, "object");
}

/// Releases a TaskGroup argument when a bare Fiber and detached scope are retired.
#[test]
fn bare_fiber_releases_task_group_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__ScopeState;
use Elephc\Async\__Scheduler;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$fiber = new Fiber(function (TaskGroup $tasks): void {});
$signal = $fiber->start($group);
unset($signal);
$scope->detach();
unset($fiber);
unset($group);
unset($scope);
unset($scheduler);
unset($state);
echo "bare";
"#,
    );
    assert_clean(out, "bare");
}

/// Retires a capture-free terminated Fiber and its start signal.
#[test]
fn bare_terminated_fiber_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): void {});
$signal = $fiber->start();
unset($signal);
unset($fiber);
echo "fiber";
"#,
    );
    assert_clean(out, "fiber");
}

/// Retires an object argument retained by a terminated Fiber.
#[test]
fn bare_terminated_fiber_releases_object_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FiberPayload {}
$payload = new FiberPayload();
$fiber = new Fiber(function (FiberPayload $value): void {});
$signal = $fiber->start($payload);
unset($signal);
unset($fiber);
unset($payload);
echo "payload";
"#,
    );
    assert_clean(out, "payload");
}

/// Retires a terminated Fiber's string return and every extracted result owner.
#[test]
fn bare_terminated_fiber_releases_return_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): string { return "value"; });
$signal = $fiber->start();
$result = $fiber->getReturn();
unset($signal);
unset($result);
unset($fiber);
echo "return";
"#,
    );
    assert_clean(out, "return");
}

/// Releases signal and return ownership after normal suspension and resumption.
#[test]
fn suspended_then_resumed_fiber_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): string {
    Fiber::suspend();
    return "done";
});
$signal = $fiber->start();
unset($signal);
$signal = $fiber->resume();
unset($signal);
$result = $fiber->getReturn();
unset($result);
unset($fiber);
echo "resume";
"#,
    );
    assert_clean(out, "resume");
}

/// Releases an object capture after its Fiber resumes and terminates normally.
#[test]
fn suspended_then_resumed_fiber_releases_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FiberCapture {}
$capture = new FiberCapture();
$fiber = new Fiber(function () use ($capture): string {
    Fiber::suspend();
    return "done";
});
$signal = $fiber->start();
unset($signal);
$signal = $fiber->resume();
unset($signal);
$result = $fiber->getReturn();
unset($result);
unset($fiber);
unset($capture);
echo "capture";
"#,
    );
    assert_clean(out, "capture");
}

/// Reclaims an unawaited child's result and captures during structured scope exit.
#[test]
fn async_unawaited_child_releases_result_at_scope_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $payload = ["answer" => 42];
    $tasks->spawn(function () use ($payload): string {
        return "value:" . $payload["answer"];
    });
    unset($payload);
});
echo "scope";
"#,
    );
    assert_clean(out, "scope");
}

/// Reclaims child closures that capture their own TaskGroup.
#[test]
fn async_child_capturing_task_group_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->spawn(function () use ($tasks): string { return "done"; });
});
echo "group-capture";
"#,
    );
    assert_clean(out, "group-capture");
}

/// Retires a sleeping child's timer/task state after normal wake and await.
#[test]
fn async_sleeping_child_releases_state_after_normal_wake() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $child = $tasks->spawn(function () use ($tasks): string {
        $tasks->sleep(0.001);
        return "awake";
    });
    echo $child->await();
});
"#,
    );
    assert_clean(out, "awake");
}

/// Releases root-task timer state after a normal sleep.
#[test]
fn async_root_sleep_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->sleep(0.001);
});
echo "root-sleep";
"#,
    );
    assert_clean(out, "root-sleep");
}

/// Releases a caught standalone CancelledException with an unused Async prelude.
#[test]
fn cancelled_exception_is_heap_clean_after_catch() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
try {
    throw new CancelledException();
} catch (CancelledException $error) {
    echo "caught";
}
"#,
    );
    assert_clean(out, "caught");
}

/// Retires a dynamically supplied Fiber callable and its captured payload.
#[test]
fn fiber_releases_dynamic_captured_callable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$payload = ["answer" => 42];
$body = function () use ($payload): void {
    $payload["answer"];
};
$fiber = new Fiber($body);
$signal = $fiber->start();
unset($signal);
unset($fiber);
unset($body);
unset($payload);
echo "dynamic";
"#,
    );
    assert_clean(out, "dynamic");
}

/// Retires Fiber start argument leases staged inside another function frame.
#[test]
fn function_frame_releases_fiber_start_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class FramePayload {}
function startInFrame(Fiber $fiber, FramePayload $payload): void {
    $signal = $fiber->start($payload);
    unset($signal);
}

$payload = new FramePayload();
$fiber = new Fiber(function (FramePayload $value): void {});
startInFrame($fiber, $payload);
unset($fiber);
unset($payload);
echo "frame";
"#,
    );
    assert_clean(out, "frame");
}

/// Retires start argument ownership when the Fiber receiver was loaded through Mixed storage.
#[test]
fn mixed_fiber_receiver_releases_start_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class MixedFiberPayload {}
$payload = new MixedFiberPayload();
$fiber = new Fiber(function (MixedFiberPayload $value): void {});
$items = [$fiber];
$live = $items[0];
$signal = $live->start($payload);
unset($signal);
$items[0] = null;
unset($live);
unset($fiber);
unset($payload);
unset($items);
echo "mixed";
"#,
    );
    assert_clean(out, "mixed");
}

/// Releases an unused closure descriptor and its captured array owner.
#[test]
fn captured_closure_releases_its_descriptor_and_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$payload = ["answer" => 42];
$closure = function () use ($payload): string {
    return "value:" . $payload["answer"];
};
unset($closure);
unset($payload);
echo "closure";
"#,
    );
    assert_clean(out, "closure");
}

/// Retires callback invocation leases for a closure with captures.
#[test]
fn descriptor_invoker_releases_captured_closure_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$payload = ["answer" => 42];
$closure = function () use ($payload): void {
    $payload["answer"];
};
call_user_func($closure);
unset($closure);
unset($payload);
echo "invoker";
"#,
    );
    assert_clean(out, "invoker");
}

/// Retires descriptor invocation ownership for a capture-free closure.
#[test]
fn descriptor_invoker_releases_capture_free_closure() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$closure = function (): void {};
call_user_func($closure);
unset($closure);
echo "invoker-free";
"#,
    );
    assert_clean(out, "invoker-free");
}

/// Releases the task-start capsule, Fiber and scope objects after explicit start and detachment.
#[test]
fn async_task_start_capsule_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__Scheduler;
use Elephc\Async\__TaskStart;
use Elephc\Async\__ScopeState;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$fiber = new Fiber(function (TaskGroup $tasks): void {});
$start = __TaskStart::fromArguments($fiber, [$group]);
$owner = new class {
    public array $fibers = [];
    public array $starts = [];
};
$owner->fibers[] = $fiber;
$owner->starts[] = $start;
$live = $owner->fibers[0];
$before = $live->isStarted();
$liveStart = $owner->starts[0];
$owner->starts[0] = null;
$signal = $liveStart->start();
unset($signal);
$after = $live->isTerminated();
$return = $live->getReturn();
$owner->fibers[0] = null;
unset($after);
unset($return);
unset($before);
unset($live);
unset($liveStart);
unset($start);
unset($fiber);
$scope->detach();
unset($group);
unset($scope);
unset($scheduler);
unset($state);
unset($owner);
echo "capsule";
"#,
    );
    assert_clean(out, "capsule");
}

/// Preserves the task-start failure while retiring capsule and root argument owners.
#[test]
fn async_task_start_capsule_rethrows_root_failure_without_refcount_loss() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__TaskStart;
use Elephc\Async\__ScopeState;
use Elephc\Async\__Scheduler;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$fiber = new Fiber(function (TaskGroup $tasks): void {
    throw new RuntimeException("capsule-failure");
});
$start = __TaskStart::fromArguments($fiber, [$group]);
try {
    $start->start();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
$scope->detach();
unset($start);
unset($fiber);
unset($group);
unset($scope);
unset($scheduler);
unset($state);
"#,
    );
    assert_clean(out, "capsule-failure");
}

/// Retires a dynamic callable's capsule and Fiber after start failure.
#[test]
fn async_task_start_dynamic_callable_failure_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__TaskStart;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$body = function (): void {
    throw new RuntimeException("task-start-dynamic");
};
$fiber = new Fiber($body);
$start = __TaskStart::fromNoArgs($fiber);
try {
    $start->start();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
unset($start);
unset($fiber);
unset($body);
"#,
    );
    assert_clean(out, "task-start-dynamic");
}

/// Keeps a caught failure owned by both outcome and cancellation state until rethrow and cleanup.
#[test]
fn async_caught_fiber_failure_can_be_stored_and_rethrown_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__TaskOutcome;
use Elephc\Async\__TaskStart;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$outcome = new __TaskOutcome();
$body = function (): void {
    throw new RuntimeException("caught-fiber");
};
$fiber = new Fiber($body);
$start = __TaskStart::fromNoArgs($fiber);
try {
    $start->start();
} catch (Throwable $error) {
    $outcome->recordFailure($error);
    $state->request($error);
}
unset($error);
try {
    $outcome->throwIfFailed();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
$state->__clearReason();
unset($start);
unset($fiber);
unset($body);
unset($outcome);
unset($state);
"#,
    );
    assert_clean(out, "caught-fiber");
}

/// Keeps a caught start failure live while querying the Fiber's terminal status.
#[test]
fn terminal_fiber_status_after_start_failure_keeps_caught_error_live() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): void {
    throw new RuntimeException("terminal-status");
});
try {
    $fiber->start();
} catch (RuntimeException $caught) {
    $terminal = $fiber->isTerminated();
    echo $terminal ? $caught->getMessage() : "not-terminal";
    unset($terminal);
    unset($caught);
}
unset($fiber);
"#,
    );
    assert_clean(out, "terminal-status");
}

/// Retires a single empty public Async root without leaking scheduler state.
#[test]
fn async_empty_root_scope_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {});
echo "root";
"#,
    );
    assert_clean(out, "root");
}

/// Retires public root-scope ownership after the root callable throws.
#[test]
fn async_root_failure_is_heap_clean_through_public_scope() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$body = function (TaskGroup $tasks): void {
    throw new RuntimeException("root-direct");
};
try {
    run($body);
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
unset($body);
"#,
    );
    assert_clean(out, "root-direct");
}

/// Retires sleeping task state when the child catches requested cancellation and returns.
#[test]
fn async_cancellation_releases_sleeping_task_state() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $child = $tasks->spawn(function () use ($tasks): string {
        try {
            $tasks->sleep(60.0);
        } catch (CancelledException $error) {
            return "cleaned";
        }
        return "unexpected";
    });
    $tasks->reschedule();
    $tasks->cancel();
    echo $child->await();
});
"#,
    );
    assert_clean(out, "cleaned");
}

/// Retires a sleeping child's state after uncaught requested cancellation.
#[test]
fn async_cancelled_sleeping_child_without_catch_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->spawn(function () use ($tasks): void {
        $tasks->sleep(60.0);
    });
    $tasks->reschedule();
    $tasks->cancel();
});
echo "cancelled";
"#,
    );
    assert_clean(out, "cancelled");
}

/// Releases root state after yielding and requesting cancellation.
#[test]
fn async_root_reschedule_then_cancel_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->reschedule();
    $tasks->cancel();
});
echo "root-cancel";
"#,
    );
    assert_clean(out, "root-cancel");
}

/// Reclaims a cancelled suspended child without a TaskGroup capture cycle.
#[test]
fn async_cancelled_suspended_child_without_task_group_capture_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->spawn(function (): void {
        Fiber::suspend(2);
    });
    $tasks->reschedule();
    $tasks->cancel();
});
echo "suspended";
"#,
    );
    assert_clean(out, "suspended");
}

/// Reclaims a cancelled suspended child whose closure captures the TaskGroup.
#[test]
fn async_cancelled_suspended_child_with_task_group_capture_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->spawn(function () use ($tasks): void {
        if ($tasks->cancellation()->isRequested()) {
            echo "";
        }
        Fiber::suspend(2);
    });
    $tasks->reschedule();
    $tasks->cancel();
});
echo "captured";
"#,
    );
    assert_clean(out, "captured");
}

/// Releases a sleeping child after root-requested cancellation and scope drain.
#[test]
fn scheduler_root_cancellation_releases_sleeping_child_state() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $tasks->spawn(function () use ($tasks): void {
        $tasks->sleep(60.0);
    });
    $tasks->reschedule();
    $tasks->cancel();
});
echo "cancelled";
"#,
    );
    assert_clean(out, "cancelled");
}

/// Retires suspended Fiber state when resumption throws an escaping exception.
#[test]
fn suspended_fiber_escaping_exception_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): void {
    Fiber::suspend();
    throw new RuntimeException("boom");
});
$fiber->start();
try {
    $fiber->resume();
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
unset($fiber);
"#,
    );
    assert_clean(out, "boom");
}

/// Releases an object capture when a resumed Fiber throws beyond its boundary.
#[test]
fn suspended_fiber_escaping_exception_releases_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class EscapingFiberCapture {}
$capture = new EscapingFiberCapture();
$fiber = new Fiber(function () use ($capture): void {
    Fiber::suspend();
    throw new RuntimeException("boom");
});
$fiber->start();
try {
    $fiber->resume();
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
unset($fiber);
unset($capture);
"#,
    );
    assert_clean(out, "boom");
}

/// Releases captures when a nested function throws out of a resumed Fiber.
#[test]
fn suspended_fiber_escaping_nested_exception_releases_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class NestedEscapingFiberCapture {}
function nestedFiberFailure(): void {
    throw new RuntimeException("boom");
}
$capture = new NestedEscapingFiberCapture();
$fiber = new Fiber(function () use ($capture): void {
    Fiber::suspend();
    nestedFiberFailure();
});
$fiber->start();
try {
    $fiber->resume();
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
unset($fiber);
unset($capture);
"#,
    );
    assert_clean(out, "boom");
}

/// Releases captured TaskGroup scope cycles after an escaping Fiber exception.
#[test]
fn suspended_fiber_escaping_exception_releases_task_group_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__Scheduler;
use Elephc\Async\__ScopeState;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$fiber = new Fiber(function () use ($group): void {
    Fiber::suspend();
    throw new RuntimeException("boom");
});
$fiber->start();
try {
    $fiber->resume();
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
$scope->detach();
unset($fiber);
unset($group);
unset($scope);
unset($scheduler);
unset($state);
"#,
    );
    assert_clean(out, "boom");
}

/// Retires a suspended Fiber after a user-created CancelledException escapes.
#[test]
fn suspended_fiber_escaping_cancelled_exception_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$fiber = new Fiber(function (): void {
    Fiber::suspend();
    throw new CancelledException();
});
$fiber->start();
try {
    $fiber->resume();
} catch (CancelledException $error) {
    echo "cancelled";
}
unset($fiber);
"#,
    );
    assert_clean(out, "cancelled");
}

/// Retires array-held Fiber receiver ownership after resumed cancellation-exception escape.
#[test]
fn array_stored_suspended_fiber_escaping_cancelled_exception_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$fiber = new Fiber(function (): void {
    Fiber::suspend();
    throw new CancelledException();
});
$fibers = [$fiber];
$fiber->start();
try {
    $fibers[0]->resume();
} catch (CancelledException $error) {
    echo "cancelled";
}
$fibers = [];
unset($fiber);
"#,
    );
    assert_clean(out, "cancelled");
}

/// Retires array-held Fiber receiver ownership when its initial start throws.
#[test]
fn array_stored_started_fiber_escaping_exception_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$fiber = new Fiber(function (): void {
    throw new CancelledException();
});
$fibers = [$fiber];
try {
    $fibers[0]->start();
} catch (CancelledException $error) {
    echo "started";
}
$fibers = [];
unset($fiber);
"#,
    );
    assert_clean(out, "started");
}

/// Releases an array-loaded receiver lease after the Fiber catches an injected exception.
#[test]
fn array_stored_fiber_throw_releases_receiver_after_caught_delivery() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): string {
    try {
        Fiber::suspend();
    } catch (RuntimeException $error) {
        return "caught";
    }
    return "missed";
});
$fibers = [$fiber];
$fiber->start();
$injected = new RuntimeException("injected");
$fibers[0]->throw($injected);
echo $fiber->getReturn();
$fibers = [];
unset($injected);
unset($fiber);
"#,
    );
    assert_clean(out, "caught");
}

/// Retires an array-loaded receiver lease before the injected exception reaches the caller.
#[test]
fn array_stored_fiber_throw_releases_receiver_before_escaped_exception() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): void {
    Fiber::suspend();
});
$fibers = [$fiber];
$fiber->start();
$injected = new RuntimeException("injected");
try {
    $fibers[0]->throw($injected);
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
$fibers = [];
unset($injected);
unset($fiber);
"#,
    );
    assert_clean(out, "injected");
}

/// Retires array-loaded receiver and throwable leases when throw rejects Fiber state.
#[test]
fn array_stored_fiber_throw_releases_receiver_on_state_error() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$fiber = new Fiber(function (): void {});
$fibers = [$fiber];
$injected = new RuntimeException("injected");
try {
    $fibers[0]->throw($injected);
} catch (FiberError $error) {
    echo "invalid";
}
$fibers = [];
unset($injected);
unset($fiber);
"#,
    );
    assert_clean(out, "invalid");
}

/// Releases a local Fiber copy loaded from an array before exception escape.
#[test]
fn array_stored_fiber_loaded_into_local_before_exception_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$fiber = new Fiber(function (): void {
    Fiber::suspend();
    throw new CancelledException();
});
$fibers = [$fiber];
$fiber->start();
$stored = $fibers[0];
try {
    $stored->resume();
} catch (CancelledException $error) {
    echo "cancelled";
}
unset($stored);
$fibers = [];
unset($fiber);
"#,
    );
    assert_clean(out, "cancelled");
}

/// Releases a property-array Fiber copy and clears its table after a method-local catch.
#[test]
fn method_catch_releases_terminated_fiber_loaded_from_property_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
final class FiberHolder {
    public array $fibers = [];
    public function drive(): void {
        $fiber = $this->fibers[0];
        try {
            $fiber->resume();
        } catch (CancelledException $error) {
            echo "method-catch";
        }
        unset($fiber);
        $this->fibers = [];
    }
}
$fiber = new Fiber(function (): void {
    Fiber::suspend();
    throw new CancelledException();
});
$holder = new FiberHolder();
$holder->fibers = [$fiber];
$fiber->start();
$holder->drive();
unset($fiber);
unset($holder);
"#,
    );
    assert_clean(out, "method-catch");
}

/// Releases TaskGroup captures when a resumed Fiber catches a cancellation exception.
#[test]
fn fiber_catching_cancelled_exception_releases_task_group_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__Scheduler;
use Elephc\Async\__ScopeState;
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) { run(function (TaskGroup $tasks): void {}); }
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
$fiber = new Fiber(function () use ($group): void {
    Fiber::suspend();
    try { throw new CancelledException(); } catch (CancelledException $error) { echo "caught"; }
});
$fiber->start();
$fiber->resume();
$scope->detach();
unset($fiber);
unset($group);
unset($scope);
unset($scheduler);
unset($state);
"#,
    );
    assert_clean(out, "caught");
}

/// Releases the cancellation state's exception and token owners after a requested throw.
#[test]
fn cancellation_state_throw_if_requested_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) { run(function (TaskGroup $tasks): void {}); }
$state = new __CancellationState();
$state->request();
try { $state->throwIfRequested(); } catch (CancelledException $error) { echo "state"; }
unset($state);
"#,
    );
    assert_clean(out, "state");
}

/// Keeps a dynamic Fiber's caught-exception string return alive until extraction and cleanup.
#[test]
fn dynamic_fiber_descriptor_catch_preserves_string_return() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function eraseSuspendingFiberCallable(callable $callback): callable {
    return $callback;
}
$source = function (): string {
    try {
        Fiber::suspend();
        throw new RuntimeException("inside");
    } catch (RuntimeException $error) {
        return "cleaned";
    }
};
$callback = eraseSuspendingFiberCallable($source);
$fiber = new Fiber($callback);
unset($callback);
unset($source);
$fiber->start();
$fiber->resume();
echo $fiber->getReturn();
unset($fiber);
"#,
    );
    assert_clean(out, "cleaned");
}

/// Retires dynamic argument cells when a Fiber body throws during initial invocation.
#[test]
fn dynamic_fiber_descriptor_escape_releases_argument_cell() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function eraseThrowingFiberCallable(callable $callback): callable {
    return $callback;
}
$source = function (int $value): void {
    if ($value === 42) {
        throw new RuntimeException("dynamic");
    }
};
$callback = eraseThrowingFiberCallable($source);
$fiber = new Fiber($callback);
unset($callback);
unset($source);
try {
    $fiber->start(42);
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
unset($fiber);
"#,
    );
    assert_clean(out, "dynamic");
}

/// Releases the parked dynamic argument container when a suspended Fiber is destroyed.
#[test]
fn suspended_dynamic_fiber_releases_parked_argument_container_on_destroy() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function eraseParkedFiberCallable(callable $callback): callable {
    return $callback;
}
$source = function (int $value): void {
    if ($value === 42) {
        Fiber::suspend();
    }
};
$callback = eraseParkedFiberCallable($source);
$fiber = new Fiber($callback);
unset($callback);
unset($source);
$fiber->start(42);
unset($fiber);
echo "parked";
"#,
    );
    assert_clean(out, "parked");
}

/// Releases child outcome storage after its failure propagates from the scope.
#[test]
fn async_failure_releases_task_outcomes_after_propagation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $tasks->spawn(function (): void {
            throw new RuntimeException("failed");
        });
    });
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_clean(out, "failed");
}

/// Releases root task/outcome state after root failure propagation.
#[test]
fn async_root_failure_releases_root_task_state() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        throw new RuntimeException("root");
    });
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_clean(out, "root");
}

/// Keeps a recorded failure owned until explicit outcome rethrow and retirement.
#[test]
fn async_task_outcome_retains_a_failure_through_explicit_rethrow() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__TaskOutcome;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$outcome = new __TaskOutcome();
$failure = new RuntimeException("outcome");
$outcome->recordFailure($failure);
unset($failure);
try {
    $outcome->throwIfFailed();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
unset($outcome);
"#,
    );
    assert_clean(out, "outcome");
}

/// Keeps a Mixed outcome's failure live after its original table owner is cleared.
#[test]
fn async_mixed_outcome_rethrow_keeps_failure_live_after_table_owner_releases() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__TaskOutcome;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$outcome = new __TaskOutcome();
$failure = new RuntimeException("mixed-outcome");
$outcome->recordFailure($failure);
unset($failure);
$table = [$outcome];
unset($outcome);
$dynamic = $table[0];
$table[0] = null;
try {
    $dynamic->throwIfFailed();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
unset($dynamic);
unset($table);
"#,
    );
    assert_clean(out, "mixed-outcome");
}

/// Preserves an acquired failure when its property outcome table is cleared before rethrow.
#[test]
fn clearing_an_outcome_table_before_rethrow_preserves_the_acquired_failure() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__TaskOutcome;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
final class OutcomeTable {
    public array $outcomes = [];
    public function throwFirst(): void {
        $outcome = $this->outcomes[0];
        $this->outcomes = [];
        $outcome->throwIfFailed();
    }
}
$outcome = new __TaskOutcome();
$failure = new RuntimeException("table");
$outcome->recordFailure($failure);
unset($failure);
$table = new OutcomeTable();
$table->outcomes[] = $outcome;
unset($outcome);
try {
    $table->throwFirst();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
unset($table);
"#,
    );
    assert_clean(out, "table");
}

/// Keeps shared outcome/reason failure identity balanced across rethrow and reason clearing.
#[test]
fn async_outcome_and_cancellation_reason_share_failure_without_refcount_loss() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\__CancellationState;
use Elephc\Async\__TaskOutcome;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

if ($argc < 0) {
    run(function (TaskGroup $tasks): void {});
}
$state = new __CancellationState();
$outcome = new __TaskOutcome();
$failure = new RuntimeException("shared");
$outcome->recordFailure($failure);
$state->request($failure);
unset($failure);
try {
    $outcome->throwIfFailed();
} catch (RuntimeException $caught) {
    echo $caught->getMessage();
    unset($caught);
}
$state->__clearReason();
unset($outcome);
unset($state);
"#,
    );
    assert_clean(out, "shared");
}

/// Retires root and child ownership when an awaited child failure escapes.
#[test]
fn async_awaited_failure_releases_root_task_state() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $child = $tasks->spawn(function (): void {
            throw new RuntimeException("awaited");
        });
        $child->await();
    });
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_clean(out, "awaited");
}

/// Releases descriptor-registration resources after socket readiness and task completion.
#[test]
fn async_reactor_releases_registration_resources() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $pair = stream_socket_pair(1, 1, 0);
    $writer = $pair[0];
    $reader = $pair[1];
    stream_set_blocking($reader, false);
    $waiter = $tasks->spawn(function () use ($tasks, $reader): string {
        if (!$tasks->awaitReadable($reader, 1.0)) {
            return "timeout";
        }
        return fread($reader, 1);
    });
    $tasks->spawn(function () use ($writer): void {
        fwrite($writer, "x");
    });
    echo $waiter->await();
    fclose($writer);
    fclose($reader);
});
"#,
    );
    assert_clean(out, "x");
}
