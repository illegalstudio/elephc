//! Purpose:
//! End-to-end tests for Elephc's structured cooperative Async scheduler.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::async_scheduler`.
//!
//! Key details:
//! - The scheduler owns FIFO readiness, monotonic timers, task awaiting,
//!   cooperative cancellation, and root-scope failure propagation.

use crate::support::*;

/// Drains spawned children before returning the root's result.
#[test]
fn async_run_waits_for_children_and_returns_the_root_value() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use Elephc\Async\CancelledException;
use function Elephc\Async\run;

$events = [];
$value = run(function (TaskGroup $tasks) use (&$events): string {
    $tasks->spawn(function () use (&$events): void {
        $events[] = "child";
    });
    $events[] = "root";
    return "done";
});
echo implode(",", $events) . "|" . $value;
"#,
    );
    assert_eq!(out, "root,child|done");
}

/// Exercises the Async root through an aliased function import.
#[test]
fn async_run_works_through_an_aliased_function_import() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Async\run as asyncRun;

echo asyncRun(static fn (\Elephc\Async\TaskGroup $tasks): string => "alias");
"#,
    );
    assert_eq!(out, "alias");
}

/// Rejects cancellation authority and token access through an escaped closed TaskGroup.
#[test]
fn async_task_group_has_no_cancellation_authority_after_its_scope_ends() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$escaped = run(static function (TaskGroup $tasks): TaskGroup {
    return $tasks;
});

try {
    $escaped->cancel();
} catch (Error $error) {
    echo $error->getMessage(), "|";
}
try {
    $escaped->cancellation();
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Elephc Async TaskGroup scope has ended|Elephc Async TaskGroup scope has ended"
    );
}

/// Makes a cloned TaskGroup observe the same closed-scope authority boundary.
#[test]
fn async_cloned_task_group_has_no_cancellation_authority_after_scope_exit() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$escaped = run(static function (TaskGroup $tasks): TaskGroup {
    return clone $tasks;
});
try {
    $escaped->cancel(new RuntimeException("after scope"));
    echo "authority-leaked|";
} catch (Error $error) {
    echo $error->getMessage(), "|";
}
try {
    $escaped->cancellation();
    echo "token-leaked";
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(out, "Elephc Async TaskGroup scope has ended|Elephc Async TaskGroup scope has ended");
}

/// Resolves Async root calls through namespace aliases and relative namespace names.
#[test]
fn async_run_works_through_namespace_alias_and_relative_namespace_names() {
    for source in [
        r#"<?php namespace Client; use Elephc\Async as A;
echo A\run(static fn (A\TaskGroup $tasks): string => "ok");"#,
        r#"<?php namespace Client; use Elephc as E;
echo E\Async\run(static fn (E\Async\TaskGroup $tasks): string => "ok");"#,
        r#"<?php namespace Elephc;
echo Async\run(static fn (Async\TaskGroup $tasks): string => "ok");"#,
        r#"<?php namespace Elephc\Async;
echo run(static fn (TaskGroup $tasks): string => "ok");"#,
    ] {
        assert_eq!(compile_and_run(source), "ok", "{source}");
    }
}

/// Rejects completion queries and awaiting after the handle's owning scope closes.
#[test]
fn async_completed_awaitable_expires_after_scope_close() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\Awaitable;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$awaitable = run(static function (TaskGroup $tasks): Awaitable {
    return $tasks->spawn(static fn (): int => 42);
});
try {
    $awaitable->isComplete();
} catch (Error $error) {
    echo $error->getMessage(), "|";
}
try {
    $awaitable->await();
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Elephc Async Awaitable scope has ended|Elephc Async Awaitable scope has ended"
    );
}

/// Lets sibling tasks run while a consumer awaits its dependency.
#[test]
fn async_await_suspends_until_the_dependency_completes() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

final class AwaitableSlot { public $task = null; }

$events = [];
$value = run(function (TaskGroup $tasks) use (&$events): int {
    $slot = new AwaitableSlot();
    $consumer = $tasks->spawn(function () use (&$events, $slot): int {
        $events[] = "consumer-before";
        $value = $slot->task->await();
        $events[] = "consumer-after";
        return $value + 1;
    });
    $slot->task = $tasks->spawn(function () use (&$events): int {
        $events[] = "dependency";
        return 20;
    });
    $sibling = $tasks->spawn(function () use (&$events): int {
        $events[] = "sibling";
        return 21;
    });
    return $consumer->await() + $sibling->await();
});
echo implode(",", $events) . "|" . $value;
"#,
    );
    assert_eq!(
        out,
        "consumer-before,dependency,sibling,consumer-after|42"
    );
}

/// Reports deadlock when bare suspension leaves live tasks without a managed wakeup.
#[test]
fn async_raw_fiber_suspend_without_scheduler_wakeup_fails_closed() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(static function (TaskGroup $tasks): void {
        $tasks->spawn(static function (): void {
            Fiber::suspend();
        });
    });
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Elephc Async deadlock: tasks are live but none are runnable"
    );
}

/// Keeps ordinary callback dispatch valid inside and outside an Async task.
#[test]
fn async_callable_guard_allows_ordinary_callbacks() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_ordinary_callback(int $value): int {
    return $value + 1;
}

$events = [];
run(static function (TaskGroup $tasks) use (&$events): void {
    $tasks->spawn(static function () use (&$events): void {
        $events[] = array_map("async_ordinary_callback", [6])[0];
    });
});
echo implode(",", $events) . "|" . array_map("async_ordinary_callback", [6])[0];
"#,
    );
    assert_eq!(out, "7|7");
}

/// Rejects runtime string callbacks to unmanaged Fiber suspension.
#[test]
fn async_dynamic_callback_fiber_suspend_without_scheduler_wakeup_fails_closed() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_runtime_fiber_suspend_name(): string {
    return "Fiber::suspend";
}

$callback = async_runtime_fiber_suspend_name();
try {
    run(static function (TaskGroup $tasks) use ($callback): void {
        $tasks->spawn(static function () use ($callback): void {
            array_map($callback, [1]);
        });
    });
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task"
    );
}

/// Rejects Fiber::suspend callable arrays before callback dispatch.
#[test]
fn async_array_callable_fiber_suspend_is_rejected_before_dispatch() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_runtime_fiber_suspend_array(): array {
    return [Fiber::class, "suspend"];
}

$callback = async_runtime_fiber_suspend_array();
try {
    run(static function (TaskGroup $tasks) use ($callback): void {
        $tasks->spawn(static function () use ($callback): void {
            array_map($callback, [1]);
        });
    });
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task"
    );
}

/// Uses internal reflection to reject handles from an earlier task-slot generation.
#[test]
fn async_task_generation_rejects_a_stale_handle_after_slot_reuse() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\Awaitable;
use Elephc\Async\TaskGroup;
use Elephc\Async\__CancellationState;
use Elephc\Async\__Scheduler;
use Elephc\Async\__ScopeState;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {});

$state = new __CancellationState();
$scheduler = new __Scheduler($state);
// Reflection is used only by this internal generation probe; direct user calls are rejected.
$runRoot = new ReflectionMethod(__Scheduler::class, "runRoot");

$firstScope = new __ScopeState($scheduler);
$firstGroup = new TaskGroup($firstScope, $state);
$runRoot->invoke($scheduler, function (TaskGroup $tasks): void {}, $firstGroup);

// The first slot-zero task has generation one. Reusing the same terminal scheduler creates
// slot zero with generation two; the old numeric handle must not resolve to that new task.
$stale = new Awaitable($scheduler, 1);
$secondScope = new __ScopeState($scheduler);
$secondGroup = new TaskGroup($secondScope, $state);
$result = $runRoot->invoke($scheduler, function (TaskGroup $tasks) use ($stale): string {
    try {
        $stale->isComplete();
    } catch (\Error $error) {
        return $error->getMessage();
    }
    return "accepted stale handle";
}, $secondGroup);

echo $result;
"#,
    );
    assert_eq!(out, "Stale Elephc Async task handle");
}

/// Keeps internal runRoot inaccessible to ordinary user calls.
#[test]
fn async_internal_runroot_cannot_be_called_directly() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use Elephc\Async\__CancellationState;
use Elephc\Async\__Scheduler;
use Elephc\Async\__ScopeState;
use function Elephc\Async\run;

run(static function (TaskGroup $tasks): void {});
$state = new __CancellationState();
$scheduler = new __Scheduler($state);
$scope = new __ScopeState($scheduler);
$group = new TaskGroup($scope, $state);
try {
    $scheduler->runRoot(static function (TaskGroup $tasks): void {
        echo "entered";
    }, $group);
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Call to private method Elephc\\Async\\__Scheduler::runRoot() from global scope"
    );
}

/// Forwards source arguments to a statically resolved spawned callable.
#[test]
fn async_spawn_forwards_arguments() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_add(int $left, int $right): int {
    return $left + $right;
}

$value = run(function (TaskGroup $tasks): int {
    $sum = $tasks->spawn(async_add(...), 20, 22);
    return $sum->await();
});
echo $value;
"#,
    );
    assert_eq!(out, "42");
}

/// Forwards static task arguments beyond the historical seven-slot cap.
#[test]
fn async_spawn_accepts_more_than_seven_statically_known_arguments() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_sum_eight(
    int $first,
    int $second,
    int $third,
    int $fourth,
    int $fifth,
    int $sixth,
    int $seventh,
    int $eighth
): int {
    return $first + $second + $third + $fourth + $fifth + $sixth + $seventh + $eighth;
}

echo run(static function (TaskGroup $tasks): int {
    return $tasks->spawn(async_sum_eight(...), 1, 2, 3, 4, 5, 6, 7, 8)->await();
});
"#,
    );
    assert_eq!(out, "36");
}

/// Keeps source name order when forwarding more than seven task arguments.
#[test]
fn async_spawn_preserves_named_arguments_above_seven() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_pack_eight(mixed ...$values): string {
    $result = "";
    foreach ($values as $key => $value) {
        $result = $result . $key . "=" . $value . ";";
    }
    return $result;
}

echo run(static function (TaskGroup $tasks): string {
    return $tasks->spawn(
        async_pack_eight(...),
        eighth: 8,
        first: 1,
        sixth: 6,
        second: 2,
        fifth: 5,
        third: 3,
        seventh: 7,
        fourth: 4,
    )->await();
});
"#,
    );
    assert_eq!(
        out,
        "eighth=8;first=1;sixth=6;second=2;fifth=5;third=3;seventh=7;fourth=4;"
    );
}

/// Applies string variadic binding to positional and named task arguments.
#[test]
fn async_spawn_applies_typed_variadic_callback_binding() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

function async_typed_variadic_type(string ...$values): string {
    foreach ($values as $value) {
        return gettype($value);
    }
    return "empty";
}

echo run(static function (TaskGroup $tasks): string {
    $positional = $tasks->spawn(async_typed_variadic_type(...), 42)->await();
    $named = $tasks->spawn(async_typed_variadic_type(...), value: 42)->await();
    return $positional . "|" . $named;
});
"#,
    );
    assert_eq!(out, "string|string");
}

/// Preserves keys when an associative PHP array is passed as one task argument.
#[test]
fn async_spawn_preserves_an_associative_array_argument() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$payload = ["first" => 1, "eighth" => 8];
echo run(static function (TaskGroup $tasks) use ($payload): string {
    return $tasks->spawn(
        static function (mixed $values): string {
            return json_encode($values);
        },
        $payload,
    )->await();
});
"#,
    );
    assert_eq!(out, "{\"first\":1,\"eighth\":8}");
}

#[test]
/// Relays insertion-ordered named variadics as a PHP array accepted by callback dispatch.
fn user_function_relays_named_variadic_arguments() {
    let out = compile_and_run(
        r#"<?php
function gather_named_values(mixed ...$values): string {
    $result = "";
    foreach ($values as $key => $value) {
        $result = $result . $key . "=" . $value . ";";
    }
    return $result;
}

function async_relay_named_args(callable $callback, mixed ...$args): string {
    return call_user_func_array($callback, $args);
}

echo async_relay_named_args(
    gather_named_values(...),
    eighth: 8,
    first: 1,
    sixth: 6,
    second: 2,
    fifth: 5,
    third: 3,
    seventh: 7,
    fourth: 4,
);
"#,
    );
    assert_eq!(
        out,
        "eighth=8;first=1;sixth=6;second=2;fifth=5;third=3;seventh=7;fourth=4;"
    );
}

/// Keeps named variadic entries when a user method collects task-style arguments.
#[test]
fn user_method_captures_named_variadic_arguments() {
    let out = compile_and_run(
        r#"<?php
final class NamedVariadicCounter
{
    public function count(mixed ...$values): int {
        return count($values);
    }
}

echo (new NamedVariadicCounter())->count(first: 1, eighth: 8);
"#,
    );
    assert_eq!(out, "2");
}

/// Preserves variadic key order after a regular callable method parameter.
#[test]
fn user_method_with_regular_parameter_preserves_named_variadic_arguments() {
    let out = compile_and_run(
        r#"<?php
final class NamedVariadicCounter
{
    public function capture(callable $callback, mixed ...$values): string {
        return json_encode($values);
    }
}

echo (new NamedVariadicCounter())->capture(
    static fn (): int => 0,
    eighth: 8,
    first: 1,
    sixth: 6,
    second: 2,
    fifth: 5,
    third: 3,
    seventh: 7,
    fourth: 4,
);
"#,
    );
    assert_eq!(
        out,
        r#"{"eighth":8,"first":1,"sixth":6,"second":2,"fifth":5,"third":3,"seventh":7,"fourth":4}"#
    );
}

/// Places a yielding task behind already-runnable siblings in FIFO order.
#[test]
fn async_reschedule_moves_the_current_task_to_the_fifo_tail() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $left = $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "left-1";
        $tasks->reschedule();
        $events[] = "left-2";
    });
    $right = $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "right-1";
        $tasks->reschedule();
        $events[] = "right-2";
    });
    $left->await();
    $right->await();
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "left-1,right-1,left-2,right-2");
}

/// Orders timer wakeups while other runnable tasks continue executing.
#[test]
fn async_sleep_wakes_by_monotonic_deadline_without_blocking_runnable_tasks() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $slow = $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "slow-start";
        $tasks->sleep(0.02);
        $events[] = "slow-end";
    });
    $fast = $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "fast-start";
        $tasks->sleep(0.001);
        $events[] = "fast-end";
    });
    $slow->await();
    $fast->await();
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "slow-start,fast-start,fast-end,slow-end");
}

/// Rejects negative sleep duration before registering a timer.
#[test]
fn async_sleep_rejects_a_negative_duration() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $tasks->sleep(-0.1);
    });
} catch (ValueError $error) {
    echo $error->getMessage();
}

"#,
    );
    assert_eq!(
        out,
        "TaskGroup::sleep(): Argument #1 ($seconds) must be greater than or equal to 0"
    );
}

/// Rejects NaN, infinity and durations outside the deadline range.
#[test]
fn async_sleep_rejects_non_finite_and_unrepresentable_durations() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    foreach ([NAN, INF] as $seconds) {
        try {
            $tasks->sleep($seconds);
        } catch (ValueError $error) {
            echo $error->getMessage() . "|";
        }
    }
    try {
        $tasks->sleep(1e300);
    } catch (ValueError $error) {
        echo $error->getMessage();
    }
});
"#,
    );
    assert_eq!(
        out,
        "TaskGroup::sleep(): Argument #1 ($seconds) must be finite|TaskGroup::sleep(): Argument #1 ($seconds) must be finite|TaskGroup::sleep(): seconds exceed the monotonic deadline range"
    );
}

/// Propagates an unobserved child failure from the drained root scope.
#[test]
fn async_child_failure_fails_the_root_scope() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $tasks->spawn(function (): void {
            throw new RuntimeException("child failed");
        });
    });
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(out, "child failed");
}

/// Finishes started children's cancellation cleanup before exposing root failure.
#[test]
fn async_root_failure_cancels_started_children_before_propagating() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
try {
    run(function (TaskGroup $tasks) use (&$events): void {
        $tasks->spawn(function () use (&$events, $tasks): void {
            $events[] = "child-start";
            try {
                $tasks->sleep(60.0);
            } catch (CancelledException $error) {
                $events[] = "cleanup:" . $error->reason()->getMessage();
            }
        });
        $tasks->reschedule();
        throw new RuntimeException("root failed");
    });
} catch (RuntimeException $error) {
    $events[] = "scope:" . $error->getMessage();
}
echo implode(",", $events);
"#,
    );
    assert_eq!(
        out,
        "child-start,cleanup:root failed,scope:root failed"
    );
}

/// Delivers one dependency result to multiple suspended task waiters.
#[test]
fn async_multiple_waiters_receive_the_same_result() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

final class SharedAwaitableSlot { public $task = null; }

$value = run(function (TaskGroup $tasks): int {
    $slot = new SharedAwaitableSlot();
    $left = $tasks->spawn(function () use ($slot): int {
        return $slot->task->await();
    });
    $right = $tasks->spawn(function () use ($slot): int {
        return $slot->task->await();
    });
    $slot->task = $tasks->spawn(fn (): int => 21);
    return $left->await() + $right->await();
});
echo $value;
"#,
    );
    assert_eq!(out, "42");
}

/// Rejects self-await instead of stranding the active task.
#[test]
fn async_task_cannot_await_itself() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $tasks): string {
    $self = null;
    $self = $tasks->spawn(static function () use (&$self): int {
        $self->await();
        return 1;
    });

    try {
        $self->await();
    } catch (Error $error) {
        return $error->getMessage();
    }
    return "missed";
});
echo $result;
"#,
    );
    assert_eq!(out, "An Elephc Async task cannot await itself");
}

/// Chooses the earliest unobserved failure rather than always preferring root failure.
#[test]
fn async_scope_reports_the_first_unobserved_failure_in_occurrence_order() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(static function (TaskGroup $tasks): void {
        $tasks->spawn(static function (): void {
            throw new LogicException("child-first");
        });
        $tasks->reschedule();
        throw new RuntimeException("root-second");
    });
} catch (Throwable $failure) {
    echo get_class($failure), "|", $failure->getMessage();
}
"#,
    );
    assert_eq!(out, "LogicException|child-first");
}

/// Preserves a normal root result after deliberate child cancellation.
#[test]
fn async_deliberate_group_cancellation_allows_a_normal_root_return() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $tasks): string {
    $tasks->spawn(static function () use ($tasks): void {
        $tasks->sleep(60.0);
    });
    $tasks->reschedule();
    $tasks->cancel();
    return "root-return";
});
echo $result;
"#,
    );
    assert_eq!(out, "root-return");
}

/// Drains grandchildren even after their parent and root bodies finish.
#[test]
fn async_scope_waits_for_grandchildren_spawned_by_child_tasks() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(static function (TaskGroup $tasks) use (&$events): void {
    $parent = $tasks->spawn(static function () use (&$events, $tasks): void {
        $tasks->spawn(static function () use (&$events, $tasks): void {
            $tasks->reschedule();
            $events[] = "grandchild";
        });
        $events[] = "parent";
    });
    $parent->await();
    $events[] = "root";
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "parent,root,grandchild");
}

/// Keeps later cleanup failure visible after an earlier failure was observed.
#[test]
fn async_observed_failure_does_not_hide_a_later_cleanup_failure() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $tasks->spawn(function () use ($tasks): void {
            try {
                $tasks->sleep(60.0);
            } catch (CancelledException $error) {
                throw new RuntimeException("cleanup");
            }
        });
        $first = $tasks->spawn(function (): void {
            throw new RuntimeException("first");
        });
        try {
            $first->await();
        } catch (RuntimeException $error) {
            echo $error->getMessage() . "|";
        }
    });
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(out, "first|cleanup");
}

/// Does not rethrow a child failure consumed through its Awaitable.
#[test]
fn async_observed_failure_is_not_rethrown_by_the_scope() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $child = $tasks->spawn(function (): void {
            throw new RuntimeException("failed");
        });
        try {
            $child->await();
        } catch (RuntimeException $error) {
            echo "caught";
        }
    });
} catch (RuntimeException $error) {
    echo "|scope";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Requests sibling cancellation after failure and waits for cleanup.
#[test]
fn async_fail_fast_cancels_siblings_and_waits_for_cleanup() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "sibling-start";
        try {
            $tasks->sleep(60.0);
        } catch (CancelledException $error) {
            $events[] = "cleanup:" . $error->reason()->getMessage();
        }
    });
    $failure = $tasks->spawn(function (): void {
        throw new RuntimeException("failed");
    });
    try {
        $failure->await();
    } catch (RuntimeException $error) {
        $events[] = "caught";
    }
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "sibling-start,caught,cleanup:failed");
}

/// Cancels a queued child before its body starts.
#[test]
fn async_cancel_prevents_an_unstarted_child_from_running() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
$value = run(function (TaskGroup $tasks) use (&$events): string {
    $tasks->spawn(function () use (&$events): void {
        $events[] = "child-ran";
    });
    $tasks->cancel();
    return "root-finished";
});
echo implode(",", $events) . "|" . $value;
"#,
    );
    assert_eq!(out, "|root-finished");
}

/// Wakes a sleeping child for cleanup and preserves its completed result.
#[test]
fn async_cancel_wakes_a_sleeping_child_and_allows_cleanup() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
$value = run(function (TaskGroup $tasks) use (&$events): string {
    $child = $tasks->spawn(function () use (&$events, $tasks): string {
        $events[] = "child-start";
        try {
            $tasks->sleep(60.0);
        } catch (CancelledException $error) {
            $events[] = "child-cleanup";
        }
        return "clean";
    });
    $tasks->reschedule();
    $tasks->cancel();
    return $child->await();
});
echo implode(",", $events) . "|" . $value;
"#,
    );
    assert_eq!(out, "child-start,child-cleanup|clean");
}

/// Reports uncaught target cancellation through Awaitable::await.
#[test]
fn async_await_throws_when_the_target_accepts_cancellation() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $child = $tasks->spawn(function () use ($tasks): void {
        $tasks->sleep(60.0);
    });
    $tasks->reschedule();
    $tasks->cancel();
    try {
        $child->await();
    } catch (CancelledException $error) {
        echo $error->getMessage();
    }
});
"#,
    );
    assert_eq!(out, "Elephc Async task cancelled");
}

/// Distinguishes cleanup exceptions from successful requested cancellation.
#[test]
fn async_cleanup_failure_is_not_swallowed_as_cancellation() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(function (TaskGroup $tasks): void {
        $tasks->spawn(function () use ($tasks): void {
            try {
                $tasks->sleep(60.0);
            } catch (CancelledException $error) {
                throw new RuntimeException("cleanup failed");
            }
        });
        $tasks->reschedule();
        $tasks->cancel();
    });
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(out, "cleanup failed");
}

/// Observes request state and reason through the read-only cancellation token.
#[test]
fn async_cancellation_is_a_read_only_observation_token() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $cancellation = $tasks->cancellation();
    echo $cancellation->isRequested() ? "requested" : "open";
    $tasks->cancel(new RuntimeException("stop requested"));
    echo $cancellation->isRequested() ? "|requested" : "|open";
    try {
        $cancellation->throwIfRequested();
    } catch (CancelledException $error) {
        echo "|" . $error->reason()->getMessage();
    }
});
"#,
    );
    assert_eq!(out, "open|requested|stop requested");
}

/// Probes immediate socket readability and writability before and after a write.
#[test]
fn async_descriptor_zero_timeout_probes_readable_and_writable_state() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $pair = stream_socket_pair(1, 1, 0);
    $left = $pair[0];
    $right = $pair[1];
    stream_set_blocking($left, false);
    stream_set_blocking($right, false);

    echo $tasks->awaitReadable($right, 0.0) ? "ready" : "idle";
    echo $tasks->awaitWritable($left, 0.0) ? "|writable" : "|blocked";
    fwrite($left, "x");
    echo $tasks->awaitReadable($right, 0.0) ? "|ready" : "|idle";
    echo "|" . fread($right, 1);
    fclose($left);
    fclose($right);
});
"#,
    );
    assert_eq!(out, "idle|writable|ready|x");
}

/// Runs a writer while its reader suspends on descriptor readiness.
#[test]
fn async_descriptor_readiness_interleaves_socket_tasks() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $pair = stream_socket_pair(1, 1, 0);
    $left = $pair[0];
    $right = $pair[1];
    stream_set_blocking($left, false);
    stream_set_blocking($right, false);

    $reader = $tasks->spawn(function () use (&$events, $tasks, $right): void {
        $events[] = "reader-wait";
        if ($tasks->awaitReadable($right, 1.0)) {
            $events[] = "reader-ready:" . fread($right, 1);
        }
    });
    $writer = $tasks->spawn(function () use (&$events, $left): void {
        $events[] = "writer";
        fwrite($left, "x");
    });
    $reader->await();
    $writer->await();
    fclose($left);
    fclose($right);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "reader-wait,writer,reader-ready:x");
}

/// Runs siblings while a descriptor waiter reaches its timeout.
#[test]
fn async_descriptor_timeout_does_not_block_runnable_tasks() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $pair = stream_socket_pair(1, 1, 0);
    $left = $pair[0];
    $right = $pair[1];
    stream_set_blocking($right, false);

    $waiter = $tasks->spawn(function () use (&$events, $tasks, $right): void {
        $events[] = "wait";
        $ready = $tasks->awaitReadable($right, 0.02);
        $events[] = $ready ? "ready" : "timeout";
    });
    $sibling = $tasks->spawn(function () use (&$events): void {
        $events[] = "sibling";
    });
    $waiter->await();
    $sibling->await();
    fclose($left);
    fclose($right);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "wait,sibling,timeout");
}

/// Wakes descriptor waits on cancellation so task cleanup can execute.
#[test]
fn async_descriptor_wait_is_a_cancellation_point() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $pair = stream_socket_pair(1, 1, 0);
    $left = $pair[0];
    $right = $pair[1];
    stream_set_blocking($right, false);

    $waiter = $tasks->spawn(function () use (&$events, $tasks, $right): void {
        $events[] = "wait";
        try {
            $tasks->awaitReadable($right);
        } catch (CancelledException $error) {
            $events[] = "cleanup";
        }
    });
    $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "cancel";
        $tasks->cancel();
    });
    $waiter->await();
    fclose($left);
    fclose($right);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "wait,cancel,cleanup");
}

/// Exercises poll readiness beyond a small descriptor-bitset range.
#[test]
fn async_descriptor_poll_supports_descriptors_above_sixty_three() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

extern function socketpair(int $domain, int $type, int $protocol, ptr $descriptors): int;
extern function write(int $fd, string $bytes, int $length): int;
extern function close(int $fd): int;
extern function malloc(int $size): ptr;
extern function free(ptr $pointer): void;

$writers = [];
$readers = [];
$pair = malloc(8);
$i = 0;
while ($i < 40) {
    socketpair(1, 1, 0, $pair);
    $writers[] = ptr_read32($pair);
    $readers[] = ptr_read32(ptr_offset($pair, 4));
    $i = $i + 1;
}
$writer = (int) $writers[39];
$reader = (int) $readers[39];
write($writer, "x", 1);

$ready = run(function (TaskGroup $tasks) use ($reader): bool {
    return $tasks->awaitReadable($reader, 0.0);
});
echo $reader > 63 && $ready ? "high-ready" : "bad";

$i = 0;
while ($i < 40) {
    close($writers[$i]);
    close($readers[$i]);
    $i = $i + 1;
}
free($pair);
"#,
    );
    assert_eq!(out, "high-ready");
}

/// Delivers simultaneous readiness in waiter registration order, not writer order.
#[test]
fn async_descriptor_ready_events_preserve_registration_order() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $first = stream_socket_pair(1, 1, 0);
    $second = stream_socket_pair(1, 1, 0);
    stream_set_blocking($first[1], false);
    stream_set_blocking($second[1], false);

    $left = $tasks->spawn(function () use (&$events, $tasks, $first): void {
        $tasks->awaitReadable($first[1], 1.0);
        $events[] = "first";
    });
    $right = $tasks->spawn(function () use (&$events, $tasks, $second): void {
        $tasks->awaitReadable($second[1], 1.0);
        $events[] = "second";
    });
    $writer = $tasks->spawn(function () use ($first, $second): void {
        fwrite($second[0], "2");
        fwrite($first[0], "1");
    });
    $left->await();
    $right->await();
    $writer->await();
    fclose($first[0]);
    fclose($first[1]);
    fclose($second[0]);
    fclose($second[1]);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "first,second");
}

/// Rejects invalid stream sources and negative readiness timeouts.
#[test]
fn async_descriptor_wait_validates_source_and_timeout() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    try {
        $tasks->awaitReadable("not-a-stream", 0.0);
    } catch (TypeError $error) {
        echo $error->getMessage() . "|";
    }

    $pair = stream_socket_pair(1, 1, 0);
    try {
        $tasks->awaitWritable($pair[0], -0.1);
    } catch (ValueError $error) {
        echo $error->getMessage();
    }
    fclose($pair[0]);
    fclose($pair[1]);
});
"#,
    );
    assert_eq!(
        out,
        "TaskGroup::awaitReadable() expects a native stream resource or descriptor|TaskGroup::awaitWritable(): Argument #2 ($timeout) must be greater than or equal to 0"
    );
}

/// Checks descriptor C-int bounds and finite representable timeouts.
#[test]
fn async_descriptor_wait_rejects_negative_non_finite_and_unrepresentable_inputs() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    try {
        $tasks->awaitReadable(-1);
    } catch (ValueError $error) {
        echo $error->getMessage() . "|";
    }
    try {
        $tasks->awaitReadable(2147483648);
    } catch (ValueError $error) {
        echo $error->getMessage() . "|";
    }
    foreach ([NAN, INF] as $timeout) {
        try {
            $tasks->awaitReadable(0, $timeout);
        } catch (ValueError $error) {
            echo $error->getMessage() . "|";
        }
    }
    try {
        $tasks->awaitWritable(0, 1e300);
    } catch (ValueError $error) {
        echo $error->getMessage();
    }
});
"#,
    );
    assert_eq!(
        out,
        "TaskGroup::awaitReadable() expects a descriptor within the non-negative C int range|TaskGroup::awaitReadable() expects a descriptor within the non-negative C int range|TaskGroup::awaitReadable(): Argument #2 ($timeout) must be finite|TaskGroup::awaitReadable(): Argument #2 ($timeout) must be finite|TaskGroup::awaitWritable(): timeout exceeds the monotonic deadline range"
    );
}

/// Refuses nested Async roots without replacing the active scheduler.
#[test]
fn async_run_rejects_a_nested_scheduler() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    try {
        run(function (TaskGroup $nested): void {});
    } catch (Error $error) {
        echo $error->getMessage();
    }
});
"#,
    );
    assert_eq!(
        out,
        "Elephc\\Async\\run() cannot be nested or called from a Fiber"
    );
}

/// Refuses root entry from a Fiber outside scheduler ownership.
#[test]
fn async_run_rejects_entry_from_an_unrelated_fiber() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$fiber = new Fiber(function (): void {
    try {
        run(function (TaskGroup $tasks): void {});
    } catch (Error $error) {
        echo $error->getMessage();
    }
});
$fiber->start();
"#,
    );
    assert_eq!(
        out,
        "Elephc\\Async\\run() cannot be nested or called from a Fiber"
    );
}

/// Keeps expired registrations inactive when readiness arrives later.
#[test]
fn async_descriptor_registration_stays_inactive_after_timeout() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $pair = stream_socket_pair(1, 1, 0);
    $writer = $pair[0];
    $reader = $pair[1];
    stream_set_blocking($reader, false);

    $waiter = $tasks->spawn(function () use (&$events, $tasks, $reader): void {
        $ready = $tasks->awaitReadable($reader, 0.001);
        $events[] = $ready ? "ready" : "timeout";
    });
    $lateWriter = $tasks->spawn(function () use (&$events, $tasks, $writer): void {
        $tasks->sleep(0.05);
        fwrite($writer, "late");
        $events[] = "write";
    });
    $waiter->await();
    $lateWriter->await();
    fclose($writer);
    fclose($reader);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "timeout,write");
}

/// Preserves cancellation-first precedence when readiness coincides.
#[test]
fn async_cancellation_wins_over_simultaneous_descriptor_readiness() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $pair = stream_socket_pair(1, 1, 0);
    $writer = $pair[0];
    $reader = $pair[1];
    stream_set_blocking($reader, false);

    $waiter = $tasks->spawn(function () use (&$events, $tasks, $reader): void {
        $events[] = "wait";
        try {
            $tasks->awaitReadable($reader);
            $events[] = "ready";
        } catch (CancelledException $error) {
            $events[] = "cancelled";
        }
    });
    $tasks->spawn(function () use (&$events, $tasks, $writer): void {
        fwrite($writer, "x");
        $events[] = "write-cancel";
        $tasks->cancel();
    });
    $waiter->await();
    fclose($writer);
    fclose($reader);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "wait,write-cancel,cancelled");
}

/// The descriptor becomes readable while its deadline is already expired. The waiter must be
/// queued once, resume once, and observe the locked timeout-first precedence.
#[test]
fn async_expired_deadline_wins_over_simultaneous_descriptor_readiness() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $pair = stream_socket_pair(1, 1, 0);
    $writer = $pair[0];
    $reader = $pair[1];
    stream_set_blocking($reader, false);

    $waiter = $tasks->spawn(function () use (&$events, $tasks, $reader): void {
        $ready = $tasks->awaitReadable($reader, 0.001);
        $events[] = $ready ? "ready" : "timeout";
    });
    $blocker = $tasks->spawn(function () use (&$events, $writer): void {
        usleep(5000);
        fwrite($writer, "x");
        $events[] = "blocking-write";
    });
    $waiter->await();
    $blocker->await();
    fclose($writer);
    fclose($reader);
});
echo implode(",", $events);
"#,
    );
    assert_eq!(out, "blocking-write,timeout");
}

/// Does not redirect an old wait to a reused native descriptor number.
#[test]
fn async_descriptor_reuse_does_not_deliver_a_stale_wakeup() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

extern function socketpair(int $domain, int $type, int $protocol, ptr $descriptors): int;
extern function write(int $fd, string $bytes, int $length): int;
extern function close(int $fd): int;
extern function malloc(int $size): ptr;
extern function free(ptr $pointer): void;

final class ReusedDescriptorSlot {
    public int $first = -1;
    public int $second = -1;
    public bool $reused = false;
}

$pair = malloc(8);
socketpair(1, 1, 0, $pair);
$oldWriter = ptr_read32($pair);
$oldReader = ptr_read32(ptr_offset($pair, 4));
$slot = new ReusedDescriptorSlot();

$ready = run(function (TaskGroup $tasks) use ($pair, $oldReader, $slot): bool {
    $waiter = $tasks->spawn(function () use ($tasks, $oldReader): bool {
        return $tasks->awaitReadable($oldReader, 0.01);
    });
    $recycler = $tasks->spawn(function () use ($pair, $oldReader, $slot): void {
        close($oldReader);
        socketpair(1, 1, 0, $pair);
        $slot->first = ptr_read32($pair);
        $slot->second = ptr_read32(ptr_offset($pair, 4));
        $slot->reused = $slot->first === $oldReader;
        write($slot->second, "new", 3);
    });
    $result = $waiter->await();
    $recycler->await();
    return $result;
});

echo $slot->reused ? "reused|" : "not-reused|";
echo $ready ? "ready" : "timeout";
close($oldWriter);
close($slot->first);
close($slot->second);
free($pair);
"#,
    );
    assert_eq!(out, "reused|timeout");
}

/// Retires owned duplicates across repeated zero-timeout probes.
#[test]
fn async_zero_timeout_probes_release_owned_descriptor_duplicates() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

run(function (TaskGroup $tasks): void {
    $pair = stream_socket_pair(1, 1, 0);
    $writer = $pair[0];
    $reader = $pair[1];
    stream_set_blocking($reader, false);

    $i = 0;
    while ($i < 512) {
        if ($tasks->awaitReadable($reader, 0.0)) {
            echo "unexpected-ready";
            return;
        }
        $i = $i + 1;
    }
    fwrite($writer, "x");
    echo $tasks->awaitReadable($reader, 0.0) ? "released" : "not-ready";
    fclose($writer);
    fclose($reader);
});
"#,
    );
    assert_eq!(out, "released");
}

/// Rejects admission after cancellation without constructing another child.
#[test]
fn async_spawn_after_cancellation_fails_before_creating_a_child() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $tasks): string {
    $tasks->cancel();
    try {
        $tasks->spawn(static fn (): int => 42);
    } catch (Error $error) {
        return $error->getMessage();
    }
    return "admitted";
});
echo $result;
"#,
    );
    assert_eq!(
        out,
        "Elephc Async TaskGroup cannot spawn after cancellation has been requested"
    );
}

/// Keeps the first reason available through an escaped observation token.
#[test]
fn async_escaped_cancellation_token_preserves_its_reason_after_scope_close() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$token = run(static function (TaskGroup $tasks): Elephc\Async\Cancellation {
    $token = $tasks->cancellation();
    $tasks->cancel(new RuntimeException("async-reason"));
    return $token;
});
echo $token->isRequested() ? "requested|" : "open|";
try {
    $token->throwIfRequested();
} catch (Elephc\Async\CancelledException $cancelled) {
    echo $cancelled->reason()->getMessage();
}
"#,
    );
    assert_eq!(out, "requested|async-reason");
}

/// Does not replace the first cancellation reason with later requests.
#[test]
fn async_awaiting_a_cancelled_child_preserves_the_first_request_reason() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $tasks): string {
    $child = $tasks->spawn(static fn (): int => 42);
    $tasks->cancel(new RuntimeException("first-reason"));
    $tasks->cancel(new RuntimeException("later-reason"));
    try {
        $child->await();
    } catch (Elephc\Async\CancelledException $cancelled) {
        return $cancelled->reason()->getMessage();
    }
    return "not-cancelled";
});
echo $result;
"#,
    );
    assert_eq!(out, "first-reason");
}

/// Preserves an initial null reason while propagating later cleanup failure.
#[test]
fn async_first_null_cancellation_reason_is_not_superseded_by_cleanup_failure() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(static function (TaskGroup $tasks): int {
        $tasks->spawn(static function () use ($tasks): int {
            try {
                $tasks->reschedule();
                $tasks->sleep(1.0);
            } catch (Elephc\Async\CancelledException $cancelled) {
                throw new LogicException("cleanup");
            }
            return 1;
        });
        $tasks->spawn(static function () use ($tasks): int {
            try {
                $tasks->reschedule();
                $tasks->sleep(1.0);
            } catch (Elephc\Async\CancelledException $cancelled) {
                echo $cancelled->reason() === null ? "null|" : "reason|";
            }
            return 1;
        });
        $tasks->reschedule();
        $tasks->cancel();
        return 0;
    });
} catch (LogicException $error) {
    echo $error->getMessage();
}

"#,
    );
    assert_eq!(out, "null|cleanup");
}

/// Treats user-created CancelledException as failure rather than request-paired cancellation.
#[test]
fn async_user_cancelled_exception_after_a_request_remains_a_task_failure() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use Elephc\Async\CancelledException;
use function Elephc\Async\run;

try {
    run(static function (TaskGroup $tasks): int {
        $tasks->spawn(static function () use ($tasks): int {
            $tasks->cancel();
            throw new CancelledException("user failure");
        });
        return 7;
    });
    echo "swallowed";
} catch (CancelledException $failure) {
    echo $failure->getMessage() === "user failure" ? "failed" : "wrong";
}
"#,
    );
    assert_eq!(out, "failed");
}

/// Classifies requested child cancellation without failing a normal root result.
#[test]
fn async_uncaught_requested_cancellation_is_terminal_cancelled_not_failed() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $tasks): int {
    $tasks->spawn(static function () use ($tasks): int {
        $tasks->reschedule();
        return 1;
    });
    $tasks->reschedule();
    $tasks->cancel();
    return 42;
});
echo $result;
"#,
    );
    assert_eq!(out, "42");
}

/// Lets a child catching requested cancellation complete with a normal value.
#[test]
fn async_caught_cancellation_can_return_a_completed_value() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $tasks): int {
    $child = $tasks->spawn(static function () use ($tasks): int {
        try {
            $tasks->reschedule();
        } catch (Elephc\Async\CancelledException $cancelled) {
            return 7;
        }
        return 1;
    });
    $tasks->reschedule();
    $tasks->cancel();
    return $child->await();
});
echo $result;
"#,
    );
    assert_eq!(out, "7");
}

/// Prevents separately constructed group state from cancelling the active root.
#[test]
fn async_forged_group_is_isolated_from_the_active_scope() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(static function (TaskGroup $live): string {
    $privateState = new Elephc\Async\__CancellationState();
    $privateScheduler = new Elephc\Async\__Scheduler($privateState);
    $privateScope = new Elephc\Async\__ScopeState($privateScheduler);
    $forged = new TaskGroup($privateScope, $privateState);
    $forged->cancel(new RuntimeException("forged"));

    return $live->cancellation()->isRequested() ? "leaked" : "isolated";
});
echo $result;
"#,
    );
    assert_eq!(out, "isolated");
}

/// Preserves the exact cancelled-child exception and reason when the root rethrows it.
#[test]
fn async_root_awaiting_a_child_cancelled_by_its_own_request_reports_terminal_outcome() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$observed = null;
try {
    $value = run(static function (TaskGroup $tasks) use (&$observed): string {
        $child = $tasks->spawn(static fn (): int => 42);
        $tasks->cancel(new RuntimeException("root-request"));
        try {
            return (string) $child->await();
        } catch (CancelledException $cancelled) {
            $observed = $cancelled;
            throw $cancelled;
        }
    });
    echo "return|", $value;
} catch (CancelledException $error) {
    $reason = $error->reason();
    echo ($error === $observed ? "same|" : "different|"), get_class($error), "|", $error->getMessage(), "|";
    echo $reason instanceof Throwable ? $reason->getMessage() : "no-reason";
} catch (Throwable $error) {
    echo get_class($error), "|", $error->getMessage(), "|not-cancelled";
}
"#,
    );
    assert_eq!(
        out,
        "same|Elephc\\Async\\CancelledException|Elephc Async task cancelled|root-request"
    );
}

/// Does not cancel the root implicitly at its own reschedule or sleep checkpoints.
#[test]
fn async_root_is_not_implicitly_cancelled_at_its_own_yield_points() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$value = run(static function (TaskGroup $tasks): int {
    $tasks->spawn(static fn (): int => 1);
    $tasks->cancel(new RuntimeException("deliberate"));
    $tasks->cancel(new RuntimeException("later"));
    $tasks->reschedule();
    $tasks->sleep(-0.0);
    return 42;
});
echo $value;
"#,
    );
    assert_eq!(out, "42");
}

/// Keeps an earlier child failure ahead of a later cancelled-child observation.
#[test]
fn async_root_cancelled_child_does_not_override_an_earlier_child_failure() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

try {
    run(static function (TaskGroup $tasks): int {
        $slow = $tasks->spawn(static function () use ($tasks): int {
            $tasks->sleep(60.0);
            return 1;
        });
        $tasks->spawn(static function (): int {
            throw new LogicException("earlier-child");
        });
        return $slow->await();
    });
} catch (LogicException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(out, "earlier-child");
}

/// Forwards runtime spreads beyond seven slots without starting the child eagerly.
#[test]
fn async_spawn_accepts_more_than_seven_dynamic_spread_arguments() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$ran = false;
$args = [];
$limit = $argc + 7;
for ($index = 0; $index < $limit; $index++) {
    $args[] = $index;
}
$count = run(static function (TaskGroup $tasks) use ($args, &$ran): int {
    $child = $tasks->spawn(static function (mixed ...$values) use (&$ran): int {
        $ran = true;
        return count($values);
    }, ...$args);
    echo $ran ? "ran" : "not-run", "|";
    return $child->await();
});
echo $count, "|", $ran ? "ran" : "not-run";
"#,
    );
    assert_eq!(out, "not-run|8|ran");
}
