//! Purpose:
//! End-to-end execution tests for the structured `Elephc\Parallel` worker surface.
//!
//! Called from:
//! - `cargo test --test codegen_tests parallel_execution`.
//!
//! Key details:
//! - Exercises actual OS worker threads and isolated runtime contexts through the native bridge.
//! - Values and failures must cross only through the PHP-wire/EPV1 and EPFL1 envelopes.
//! - Scope exit drains unjoined children; explicit joins remain repeatable from cached parent data.

use crate::support::*;

/// Named fields and callback inputs retain source order in both execution domains.
#[test]
fn task_submission_named_arguments_preserve_source_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
function input(): int { echo "I"; return 42; }
function interval(): float { echo "S"; return 0.001; }
function execution(bool $parallel): Execution {
    echo $parallel ? "P" : "A";
    return $parallel ? Execution::Parallel : Execution::Async;
}
run(static function (TaskGroup $tasks): void {
    $task = $tasks->spawn(task: static fn(int $value): int => $value,
        value: input(), execution: execution(false));
    echo $task->await(), "|";
    $task = $tasks->spawn(task: static fn(int $value): int => $value,
        value: input(), execution: execution(true));
    echo $task->await(), "|";
});
run(static function (TaskGroup $tasks): void {
    $tasks->every(task: static function (PeriodicControl $control, int $value): void {
        echo $value; $control->stop();
    }, value: input(), seconds: interval(), execution: execution(false));
});
echo "|";
run(static function (TaskGroup $tasks): void {
    $tasks->every(task: static function (PeriodicControl $control, int $value): void {
        echo $value; $control->stop();
    }, value: input(), seconds: interval(), execution: execution(true));
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "IA42|IP42|ISA42|ISP42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Static indexed/associative unpacking chooses the executor before native submission.
#[test]
fn task_submission_static_unpack_preserves_execution_and_callback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
function input(int $value): int { echo $value; return $value; }
run(static function (TaskGroup $tasks): void {
    echo $tasks->spawn(...[Execution::Parallel, static fn(int $value): int => $value, 42])->await(), "|";
    echo $tasks->spawn(...["value" => input(1), "execution" => Execution::Async,
        "task" => static fn(int $value): int => $value, "value" => input(2)])->await(), "|";
    echo $tasks->spawn(...["value" => input(3), "execution" => Execution::Parallel,
        "task" => static fn(int $value): int => $value, "value" => input(4)])->await();
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "42|122|344", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Duplicate unpack keys release the replaced object before the selected callback runs.
#[test]
fn task_submission_overwritten_unpack_value_retires_before_dispatch() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
class Replaced {
    public function __construct() { echo "O"; }
    public function __destruct() { echo "D"; }
}
function replacement(): int { echo "N"; return 42; }
run(static function (TaskGroup $tasks): void {
    echo $tasks->spawn(...["value" => new Replaced(), "value" => replacement(),
        "execution" => Execution::Async, "task" => static fn(int $value): int => $value])->await();
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "OND42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Trailing runtime indexed input is evaluated once before either executor binds the callback.
#[test]
fn task_submission_dynamic_indexed_tail_preserves_binding() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
function input(int $value): int { echo "I"; return $value; }
run(static function (TaskGroup $tasks): void {
    $inputs = [input(20), 22];
    echo $tasks->spawn(Execution::Async, static fn(int $a, int $b): int => $a + $b, ...$inputs)->await(), "|";
    $inputs = [input(20), 22];
    echo $tasks->spawn(Execution::Parallel, static fn(int $a, int $b): int => $a + $b, ...$inputs)->await();
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "I42|I42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A later named-input throw releases earlier snapshots without closing the live group.
#[test]
fn task_submission_throwing_named_input_retires_snapshots_and_preserves_scope() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
class Input {
    public function __destruct() { echo "D"; }
}
function fail(): int { throw new RuntimeException("input failure"); }
run(static function (TaskGroup $tasks): void {
    try {
        $tasks->spawn(task: static fn(Input $input, int $value): int => $value,
            input: new Input(), value: fail(), execution: Execution::Async);
    } catch (RuntimeException $error) {
        echo "C"; unset($error);
    }
    echo $tasks->spawn(Execution::Parallel, static fn(int $value): int => $value, 42)->await();
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "DC42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime-returned execution enums select both engines and are evaluated exactly once.
#[test]
fn task_spawn_runtime_execution_selects_both_engines_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
function chooseExecution(bool $parallel): Execution {
    echo $parallel ? "P" : "A";
    return $parallel ? Execution::Parallel : Execution::Async;
}
run(static function (TaskGroup $tasks): void {
    $async = $tasks->spawn(chooseExecution(false), static fn(int $value): int => $value * 2, 21);
    $parallel = $tasks->spawn(chooseExecution(true), static fn(int $value): int => $value + 1, 41);
    echo "|", $async->await(), "|", $parallel->await();
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "AP|42|42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both periodic engines accept a runtime mode without duplicating registration or selector effects.
#[test]
fn task_every_runtime_execution_selects_both_engines_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
function chooseExecution(bool $parallel): Execution {
    echo $parallel ? "P" : "A";
    return $parallel ? Execution::Parallel : Execution::Async;
}
run(static function (TaskGroup $tasks): void {
    $tasks->every(chooseExecution(false), 0.001, static function (PeriodicControl $control): void {
        echo "async"; $control->stop();
    });
});
echo "|";
run(static function (TaskGroup $tasks): void {
    $tasks->every(chooseExecution(true), 0.001, static function (PeriodicControl $control): void {
        echo "parallel"; $control->stop();
    });
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Aasync|Pparallel", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Application direct, dynamic and callable dispatch cannot invoke native cadence helpers.
#[test]
fn task_every_control_internal_helpers_are_not_application_capabilities() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 100.0, static function (PeriodicControl $control): void {
        $control->stop();
    });
    $control->stop();
    try { $control->__nativeId(); } catch (Error $error) { echo "direct|"; unset($error); }
    $method = "__claim";
    try { $control->$method(); } catch (Error $error) { echo "dynamic|"; unset($error); }
    try { call_user_func([$control, "__close"]); }
    catch (Throwable $error) { echo "callback"; unset($error); }
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "direct|dynamic|callback", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Periodic authority cannot be cloned, serialized or restored as an application data object.
#[test]
fn task_every_control_rejects_authority_copy() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 100.0, static function (PeriodicControl $control): void {
        $control->stop();
    });
    $control->stop();
    try { $copy = clone $control; } catch (Error $error) { echo "clone|"; unset($error); }
    try { serialize($control); } catch (Error $error) { echo "serialize|"; unset($error); }
    try { unserialize('O:27:"Elephc\Task\PeriodicControl":0:{}'); }
    catch (Error $error) { echo "unserialize"; unset($error); }
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "clone|serialize|unserialize", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The unified task API lets an Async sibling progress while awaiting a Parallel result.
#[test]
fn parallel_cooperative_adapter_allows_sibling_progress() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
echo run(static function (TaskGroup $tasks): int {
    $future = $tasks->spawn(Execution::Parallel, static function (): int {
        usleep(100000);
        return 42;
    });
    $tasks->spawn(Execution::Async, static function (TaskGroup $scope): void {
        $scope->sleep(0.001);
        echo "tick|";
    }, $tasks);
    return $future->await();
});
"#);
    assert_eq!(out, "tick|42");
}

/// A cooperatively awaited worker failure is observed once and keeps its parent-side failure type.
#[test]
fn parallel_cooperative_adapter_observes_failure() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
echo run(static function (TaskGroup $tasks): string {
    $future = $tasks->spawn(Execution::Parallel, static function (): int {
        usleep(1000);
        throw new RuntimeException("worker-boom");
    });
    try { $future->await(); }
    catch (\Elephc\Task\TaskFailure $failure) { return $failure->getMessage(); }
    return "missed";
});
"#);
    assert_eq!(out, "worker-boom");
}

/// A terminal unified handle remains readable without retaining an active execution scope.
#[test]
fn task_public_async_result_survives_scope_close() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Task, Execution};
use function Elephc\Task\run;
$task = run(static function (TaskGroup $tasks): Task {
    return $tasks->spawn(Execution::Async, static fn(): int => 42);
});
echo $task->isComplete() ? "done|" : "pending|";
echo $task->await(), "|", $task->await();
"#);
    assert_eq!(out, "done|42|42");
}

/// A settled worker result survives native job release and remains repeatably readable.
#[test]
fn task_public_parallel_result_survives_scope_close() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Task, Execution};
use function Elephc\Task\run;
$task = run(static function (TaskGroup $tasks): Task {
    return $tasks->spawn(Execution::Parallel, static fn(): int => 42);
});
echo $task->isComplete() ? "done|" : "pending|";
echo $task->await(), "|", $task->await();
"#);
    assert_eq!(out, "done|42|42");
}

/// Closed group aliases cannot regain task submission authority through the unified facade.
#[test]
fn task_public_group_rejects_submission_after_scope_close() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
$closed = run(static fn(TaskGroup $tasks): TaskGroup => $tasks);
try { $closed->spawn(Execution::Async, static fn(): int => 1); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Elephc TaskGroup scope has ended");
}

/// Runtime clone validation cannot duplicate scope authority or a Future's original native job.
#[test]
fn parallel_private_clone_guards_preserve_original_scope_and_job() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;
run(static function (TaskGroup $tasks): void {
    try { $groupDuplicate = clone $tasks; echo 'group-cloned|'; }
    catch (Error $error) { echo str_contains($error->getMessage(), 'private') ? 'group|' : 'wrong-group|'; }
    $future = $tasks->spawn(static fn (): int => 7);
    try { $futureDuplicate = clone $future; echo 'future-cloned|'; }
    catch (Error $error) { echo str_contains($error->getMessage(), 'private') ? 'future|' : 'wrong-future|'; }
    echo $future->join();
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "group|future|7", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A handle submitted while draining a child also has a terminal snapshot after run.
#[test]
fn task_public_descendant_result_survives_scope_close() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Task, Execution};
use function Elephc\Task\run;
$producer = run(static function (TaskGroup $tasks): Task {
    return $tasks->spawn(Execution::Async, static function (TaskGroup $scope): Task {
        return $scope->spawn(Execution::Async, static fn(): int => 42);
    }, $tasks);
});
$descendant = $producer->await();
echo $descendant->isComplete() ? "done|" : "pending|";
echo $descendant->await();
"#);
    assert_eq!(out, "done|42");
}

/// Periodic registration returns immediately and injects its explicit control into the callback.
#[test]
fn task_every_async_registration_is_nonblocking_and_scope_owned() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $started = hrtime(true);
    $tasks->every(Execution::Async, 0.001, static function ($control, int $started): void {
        echo hrtime(true) - $started >= 1000000 ? "beat" : "early";
        $control->stop();
    }, $started);
    echo "registered|";
});
"#);
    assert_eq!(out, "registered|beat");
}

/// Independent registrations keep their own deadlines instead of sharing one global beat.
#[test]
fn task_every_async_registrations_have_independent_intervals() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $tasks->every(Execution::Async, 0.02, static function ($control): void {
        echo "slow|";
        $control->stop();
    });
    $tasks->every(Execution::Async, 0.001, static function ($control): void {
        echo "fast|";
        $control->stop();
    });
});
"#);
    // A delayed process can see both deadlines expired in one FIFO dispatch step.
    // Independent stop authority must still allow each registration to execute once.
    let mut beats = out.split('|').filter(|beat| !beat.is_empty()).collect::<Vec<_>>();
    beats.sort_unstable();
    assert_eq!(beats, ["fast", "slow"]);
}

/// Parallel periodic callbacks remain isolated while sharing the public scalar control API.
#[test]
fn task_every_parallel_uses_isolated_worker_and_cooperative_parent() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
gc_enable();
run(static function (TaskGroup $tasks): void {
    $tasks->every(Execution::Parallel, 0.001, static function (PeriodicControl $control): void {
        gc_disable();
        echo "worker|";
        $control->stop();
    });
    $tasks->spawn(Execution::Async, static function (): void { echo "sibling|"; });
});
echo gc_enabled() ? "isolated" : "polluted";
"#);
    assert!(out.ends_with("isolated"), "worker changed parent GC state: {out}");
    let mut events = out.strip_suffix("isolated").unwrap().split('|')
        .filter(|event| !event.is_empty()).collect::<Vec<_>>();
    events.sort_unstable();
    assert_eq!(events, ["sibling", "worker"]);
}

/// Repeated isolated workers share only application file data and the native control identity.
#[test]
fn task_every_parallel_repeats_changes_interval_and_retires_history() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
file_put_contents("periodic-count.txt", "0");
run(static function (TaskGroup $tasks): void {
    $tasks->every(Execution::Parallel, 0.001, static function (PeriodicControl $control): void {
        $count = (int) file_get_contents("periodic-count.txt") + 1;
        file_put_contents("periodic-count.txt", (string) $count);
        if ($count === 2) { $control->setInterval(0.003); }
        if ($count === 3) { $control->stop(); }
    });
});
echo file_get_contents("periodic-count.txt");
unlink("periodic-count.txt");
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A periodic worker error stops admission and is materialized as the canonical parent failure.
#[test]
fn task_every_parallel_failure_drains_and_propagates() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, TaskFailure, Execution};
use function Elephc\Task\run;
try {
    run(static function (TaskGroup $tasks): void {
        $tasks->every(Execution::Parallel, 0.001, static function (PeriodicControl $control): void {
            throw new RuntimeException("periodic-worker-boom");
        });
    });
} catch (TaskFailure $error) { echo $error->getMessage(); unset($error); }
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "periodic-worker-boom", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Interval mutation during a callback rearms the next invocation after completion.
#[test]
fn task_every_async_set_interval_and_repetition_keep_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
class BeatState { public int $count = 0; public int $last = 0; }
run(static function (TaskGroup $tasks): void {
    $state = new BeatState();
    $tasks->every(Execution::Async, 0.001, static function ($control, BeatState $state): void {
        $now = hrtime(true);
        $state->count++;
        if ($state->count === 1) {
            $state->last = $now;
            $control->setInterval(0.005);
            echo "first|";
        } else {
            echo $now - $state->last >= 5000000 ? "changed|" : "early|";
            $control->stop();
        }
    }, $state);
});
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "first|changed|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Another task can stop a sleeping registration without waiting for its long deadline.
#[test]
fn task_every_async_external_stop_wakes_the_pump() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 10.0, static function ($control): void {
        echo "unexpected";
        $control->stop();
    });
    $tasks->spawn(Execution::Async, static function ($control): void {
        $control->stop();
        echo "stopped";
    }, $control);
});
"#);
    assert_eq!(out, "stopped");
}

/// A retained control cannot regain mutation authority after its owning scope returns.
#[test]
fn task_every_async_control_expires_after_scope_close() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
$control = run(static function (TaskGroup $tasks): PeriodicControl {
    return $tasks->every(Execution::Async, 0.001, static function ($control): void { $control->stop(); });
});
try { $control->stop(); }
catch (Error $error) { echo $error->getMessage(); unset($error); }
unset($control);
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Elephc periodic control scope has ended", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Invalid registration intervals fail before admitting work or owning a native registration.
#[test]
fn task_every_async_rejects_invalid_intervals() {
    let out = compile_and_run(r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    foreach ([0.0, -1.0, INF, NAN, 0.000000000001] as $seconds) {
        try { $tasks->every(Execution::Async, $seconds, static function ($control): void { echo "bad"; }); }
        catch (ValueError $error) { echo "rejected|"; }
    }
});
"#);
    assert_eq!(out, "rejected|rejected|rejected|rejected|rejected|");
}

/// A worker-local Fiber keeps worker GC/heap state instead of entering the main runtime context.
#[test]
fn parallel_worker_local_fiber_inherits_worker_runtime_context() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

gc_enable();
$result = run(static function (TaskGroup $tasks): int {
    return $tasks->spawn(static function (): int {
        $fiber = new Fiber(static function (): int {
            gc_disable();
            return 42;
        });
        $fiber->start();
        return (int) $fiber->getReturn();
    })->join();
});
echo $result, '|', gc_enabled() ? 'main-enabled' : 'main-disabled';
"#);
    assert!(out.success, "worker Fiber failed: {}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "42|main-enabled");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Generators share initial coroutine frames and must retain the worker's arena and request state.
#[test]
fn parallel_worker_local_generator_inherits_worker_runtime_context() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

function worker_context_values(): iterable {
    gc_disable();
    yield 40;
    yield 2;
}
gc_enable();
$result = run(static function (TaskGroup $tasks): int {
    return $tasks->spawn(static function (): int {
        $sum = 0;
        foreach (worker_context_values() as $value) {
            $sum += $value;
        }
        return $sum;
    })->join();
});
echo $result, '|', gc_enabled() ? 'main-enabled' : 'main-disabled';
"#);
    assert!(out.success, "worker Generator failed: {}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "42|main-enabled");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
/// A transferred string argument cannot invoke an unrelated parent's global Stringable object.
fn parallel_string_entry_does_not_inherit_unrelated_stringifier_globals() {
    let out = compile_and_run(r#"<?php
class GlobalText {
    public function __toString(): string {
        global $observed;
        $observed = 1;
        return 'unrelated';
    }
}
function text_identity(string $text): string { return $text; }
echo Elephc\Parallel\run(static function (Elephc\Parallel\TaskGroup $tasks): string {
    return $tasks->spawn(text_identity(...), 'safe')->join();
});
"#);
    assert_eq!(out, "safe");
}

#[test]
/// Namespace aliases and relative names resolve the same public Parallel root API.
fn parallel_run_works_through_namespace_alias_and_relative_namespace_names() {
    for source in [
        r#"<?php namespace Client; use Elephc\Parallel as P;
echo P\run(static fn (P\TaskGroup $tasks): string => "ok");"#,
        r#"<?php namespace Elephc\Parallel;
echo run(static fn (TaskGroup $tasks): string => "ok");"#,
    ] {
        assert_eq!(compile_and_run(source), "ok", "{source}");
    }
}

#[test]
/// Catching a joined worker failure still cancels its group and forbids a later spawn.
fn parallel_caught_join_failure_requests_cancellation_and_rejects_late_spawn() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $token = $tasks->cancellation();
    $failure = $tasks->spawn(static function (): int {
        throw new RuntimeException("observed");
    });
    try {
        $failure->join();
    } catch (Elephc\Parallel\TaskFailure $error) {
        echo $error->getMessage(), "|";
    }
    echo $token->isRequested() ? "requested|" : "unrequested|";
    try {
        $tasks->spawn(static fn (): int => 99);
        echo "admitted|";
    } catch (Error $error) {
        echo $error->getMessage(), "|";
    }
    return 42;
});
echo $result;
"#,
    );
    assert_eq!(out, "observed|requested|Elephc Parallel TaskGroup cannot spawn after cancellation has been requested|42");
}

#[test]
/// User PHP cannot access compiler-private scope helpers through ordinary calls.
fn parallel_private_scope_helpers_cannot_be_called_by_user_code() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

echo run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $token = $tasks->cancellation();
    $future = $tasks->spawn(static fn (): int => 42);
    try {
        $tasks->__cancelAll();
        echo "cancel-leaked|";
    } catch (Error $error) {
        echo "cancel-private|";
    }
    try {
        $future->__attachScope($tasks);
        echo "attach-leaked|";
    } catch (Error $error) {
        echo "attach-private|";
    }
    echo $token->isRequested() ? "requested|" : "unrequested|";
    return $future->join();
});
"#,
    );
    assert_eq!(out, "cancel-private|attach-private|unrequested|42");
}

#[test]
/// Normal compilation exposes the catalogued Parallel prelude types.
fn parallel_catalogued_prelude_types_are_visible_in_normal_mode() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

echo run(static function (TaskGroup $tasks): string {
    if (!class_exists("Elephc\\Parallel\\Future")) {
        return "missing:Future";
    }
    if (!class_exists("Elephc\\Parallel\\TaskFailure")) {
        return "missing:TaskFailure";
    }
    if (!class_exists("Elephc\\Parallel\\TaskFailureKind")) {
        return "missing:TaskFailureKind";
    }
    if (!class_exists("Elephc\\Parallel\\TaskGroup")) {
        return "missing:TaskGroup";
    }
    if (!class_exists("Elephc\\Parallel\\TaskGroupFailure")) {
        return "missing:TaskGroupFailure";
    }
    return "visible";
});
"#,
    );
    assert_eq!(out, "visible");
}

#[test]
/// An invalid zero job id settles as infrastructure failure rather than a pending worker.
fn parallel_zero_job_id_drains_as_an_infrastructure_failure() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\Future;
use Elephc\Parallel\TaskGroup;
use Elephc\Parallel\TaskGroupFailure;
use Elephc\Parallel\TaskFailureKind;
use function Elephc\Parallel\run;

try {
    run(static function (TaskGroup $tasks): int {
        $future = (new ReflectionClass(Future::class))->newInstanceWithoutConstructor();
        (new ReflectionProperty(Future::class, "jobId"))->setValue($future, 0);
        (new ReflectionMethod(TaskGroup::class, "__recordFuture"))->invoke($tasks, $future);
        unset($future);
        return 0;
    });
} catch (TaskGroupFailure $failure) {
    $failures = $failure->failures();
    echo count($failures), "|", $failures[0]->kind() === TaskFailureKind::Infrastructure ? "infrastructure" : "wrong";
}
"#,
    );
    assert_eq!(out, "1|infrastructure");
}

#[test]
/// Live Parallel handles cannot escape their ownership scope through PHP serialization.
fn parallel_handles_cannot_be_serialized_or_unserialized() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

echo run(static function (TaskGroup $tasks): string {
    $future = $tasks->spawn(static fn (): int => 42);
    $future->join();
    $result = "";
    try { serialize($future); } catch (Error $error) { $result .= "future|"; }
    try { serialize($tasks); } catch (Error $error) { $result .= "group|"; }
    try { unserialize('O:22:"Elephc\\Parallel\\Future":0:{}'); } catch (Error $error) { $result .= "future-unserialize|"; }
    try { unserialize('O:25:"Elephc\\Parallel\\TaskGroup":0:{}'); } catch (Error $error) { $result .= "group-unserialize"; }
    return $result;
});
"#,
    );
    assert_eq!(out, "future|group|future-unserialize|group-unserialize");
}

#[test]
/// A cycle-collector destructor's exit is contained by the worker fatal boundary.
fn parallel_worker_cycle_destructor_exit_stays_inside_the_worker() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use Elephc\Parallel\TaskGroupFailure;
use Elephc\Parallel\TaskFailureKind;
use function Elephc\Parallel\run;

final class ExitDuringWorkerRelease {
    public $cycle = null;
    public function __construct() {}
    public function __destruct() { exit(7); }
}

try {
    run(static function (TaskGroup $tasks): int {
        $tasks->spawn(static function (): int {
            $value = new ExitDuringWorkerRelease();
            $value->cycle = $value;
            unset($value);
            return 42;
        });
        return 0;
    });
} catch (TaskGroupFailure $failure) {
    $item = $failure->failures()[0];
    echo $item->kind()->value, "|", $item->getMessage();
}
"#,
    );
    assert_eq!(out, "2|Parallel worker terminated with a PHP fatal");
}

#[test]
/// Concurrent worker streams preserve unique ids in the synchronized resource registry.
fn parallel_worker_resource_ids_remain_unique_under_concurrent_stream_use() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

echo run(static function (TaskGroup $tasks): string {
    $futures = [];
    for ($worker = 0; $worker < 7; $worker++) {
        $futures[] = $tasks->spawn(static function (): array {
            $ids = [];
        for ($index = 0; $index < 16; $index++) {
            $stream = fopen("/dev/null", "r");
            usleep(1000);
            $ids[] = get_resource_id($stream);
            fclose($stream);
            }
            return $ids;
        });
    }
    $seen = [];
    foreach ($futures as $future) {
        foreach ($future->join() as $id) {
            if (isset($seen[$id])) { return "duplicate"; }
            $seen[$id] = true;
        }
    }
    return count($seen) === 112 ? "unique" : "missing";
});
"#,
    );
    assert_eq!(out, "unique");
}

#[test]
/// A static first-class function closure is accepted as a copied worker entry.
fn parallel_spawn_accepts_static_first_class_function_closure() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

extern function atoi(string $value): int;

function parallel_named_worker(int $value): int {
    return $value + 1;
}

echo run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $named = $tasks->spawn(parallel_named_worker(...), 41)->join();
    $builtin = $tasks->spawn(strlen(...), "hello")->join();
    $extern = $tasks->spawn(atoi(...), "42")->join();
    return $named + $builtin + $extern;
});
"#,
    );
    assert_eq!(out, "89");
}

#[test]
/// Worker argument transport is independent of the seven-slot raw Fiber start ABI.
fn parallel_spawn_accepts_more_than_seven_arguments() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

echo run(static function (TaskGroup $tasks): int {
    return $tasks->spawn(
        static function (
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
        },
        1, 2, 3, 4, 5, 6, 7, 8,
    )->join();
});
"#,
    );
    assert_eq!(out, "36");
}

#[test]
/// Dynamic callable strings cannot dispatch compiler-only Parallel bridge externs.
fn parallel_bridge_extern_is_not_dispatchable_through_a_runtime_string() {
    let error = compile_and_run_expect_failure(
        r#"<?php
$callback = "elephc_parallel_parent_scope_enter";
function invoke_runtime_callback(string $callback): mixed {
    return call_user_func($callback);
}
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks) use ($callback): int {
    invoke_runtime_callback($callback);
    return 0;
});
echo "completed";
"#,
    );
    assert!(
        error.contains("Call to undefined function"),
        "unexpected runtime error: {error}"
    );
}

#[test]
/// Copied arguments, closure captures and nested results round-trip through the worker wire format.
fn parallel_worker_round_trips_arguments_captures_and_nested_results() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$prefix = "worker";
$result = run(static function (Elephc\Parallel\TaskGroup $tasks) use ($prefix): array {
    $future = $tasks->spawn(
        static fn (array $values): string => $prefix . "|" . ($values[0] + 1) . "|" . $values[1][1],
        [41, [true, "ok"]],
    );
    $first = $future->join();
    $second = $future->join();
    $nested = $tasks->spawn(static fn (): array => ["A\0B", [1, true, "x"]])->join();
    $spread = [20, 22];
    $sum = $tasks->spawn(static fn (int $left, int $right): int => $left + $right, ...$spread)->join();
    return [$first, $second, $nested, $sum];
});
echo $result[0], "|", $result[1], "|", strlen($result[2][0]), "|", $result[2][1][2], "|", $result[3];
"#,
    );
    assert_eq!(out, "worker|42|ok|worker|42|ok|3|x|42");
}

#[test]
/// A remote worker Throwable settles as a parent-side TaskFailure.
fn parallel_worker_throwable_becomes_parent_task_failure() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $tasks->spawn(static function (): int {
            throw new RuntimeException("worker-boom", 17);
        });
        return 0;
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    $taskFailures = $groupFailure->failures();
    $taskFailure = $taskFailures[0];
    echo $taskFailure instanceof Elephc\Parallel\TaskFailure ? "task|" : "wrong|";
    echo $taskFailure->remoteClass(), "|", $taskFailure->getMessage(), "|", $taskFailure->getCode();
}
"#,
    );
    assert_eq!(out, "task|RuntimeException|worker-boom|17");
}

#[test]
/// Simultaneous root/child failures remain represented when scope teardown aggregates them.
fn parallel_combines_simultaneous_root_and_child_failures_without_loss() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $tasks->spawn(static function (): int {
            throw new RuntimeException("child-boom", 17);
        });
        usleep(50000);
        throw new LogicException("root-boom", 23);
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    $root = $groupFailure->rootFailure();
    $children = $groupFailure->failures();
    echo get_class($root), "|", $root->getMessage(), "|";
    echo count($children), "|", $children[0]->getMessage();
}
"#,
    );
    assert_eq!(out, "LogicException|root-boom|1|child-boom");
}

#[test]
/// A root failure without competing child failures is rethrown unchanged.
fn parallel_rethrows_a_lone_root_failure_unchanged() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        throw new LogicException("root-only", 29);
    });
} catch (LogicException $failure) {
    echo get_class($failure), "|", $failure->getMessage(), "|", $failure->getCode();
}
"#,
    );
    assert_eq!(out, "LogicException|root-only|29");
}

#[test]
/// Leaving a root drains every unjoined child before releasing group state.
fn parallel_scope_drains_multiple_unjoined_workers() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$value = run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $first = $tasks->spawn(static fn (int $value): int => $value + 1, 20);
    $second = $tasks->spawn(static fn (int $value): int => $value * 2, 21);
    return $second->join() + $first->join();
});
echo $value;
"#,
    );
    assert_eq!(out, "63");
}

#[test]
/// A scope failure cancels queued siblings without treating those cancellations as new causes.
fn parallel_scope_failure_cancels_queued_siblings_and_reports_only_the_cause() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $tasks->spawn(static function (): int {
            throw new RuntimeException("first");
        });
        for ($i = 0; $i < 9; $i++) {
            $tasks->spawn(static function (): int {
                usleep(20000);
                return 1;
            });
        }
        return 0;
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    $taskFailures = $groupFailure->failures();
    echo count($taskFailures), "|", $taskFailures[0]->getMessage();
}
"#,
    );
    assert_eq!(out, "1|first");
}

#[test]
/// A running child observes cancellation through its copied cancellation capture/proxy.
fn parallel_running_worker_observes_shared_cancellation_capture() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $cancellation = $tasks->cancellation();
        $tasks->spawn(static function () use (&$cancellation): int {
            while (!$cancellation->isRequested()) {
                usleep(1000);
            }
        try {
            $cancellation->throwIfRequested();
        } catch (Elephc\Async\CancelledException $cancelled) {
                echo $cancelled->reason() === null ? "worker-null|" : "worker-reason|";
                return 1;
            }
            return 0;
        });
        $tasks->spawn(static function (): int {
            usleep(10000);
            throw new RuntimeException("first");
        });
        return 0;
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    $taskFailures = $groupFailure->failures();
    echo $taskFailures[0]->getMessage();
}
"#,
    );
    assert_eq!(out, "worker-null|first");
}

#[test]
/// A direct cancellation argument becomes a worker-local observation proxy.
fn parallel_spawn_transfers_a_direct_cancellation_argument_as_a_worker_local_proxy() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\Cancellation;
use Elephc\Async\__CancellationState;
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

$token = new Cancellation(new __CancellationState());
echo run(static function (TaskGroup $tasks) use ($token): string {
    return $tasks->spawn(
        static fn (Cancellation $workerToken): string => $workerToken->isRequested() ? "requested" : "open",
        $token,
    )->join();
});
"#,
    );
    assert_eq!(out, "open");
}

#[test]
/// Group cancellation requests children without copying the parent's Throwable reason across arenas.
fn parallel_group_cancel_requests_workers_without_transferring_parent_reason() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $cancellation = $tasks->cancellation();
    $tasks->spawn(static function () use (&$cancellation): int {
        while (!$cancellation->isRequested()) {
            usleep(1000);
        }
        try {
            $cancellation->throwIfRequested();
        } catch (Elephc\Async\CancelledException $cancelled) {
            echo $cancelled->reason() === null ? "worker-null|" : "worker-reason|";
            return 1;
        }
        return 0;
    });
    usleep(10000);
    $tasks->cancel(new RuntimeException("parent-only"));
    $tasks->cancel(new RuntimeException("later"));
    try {
        $cancellation->throwIfRequested();
    } catch (Elephc\Async\CancelledException $parent) {
        echo $parent->reason()->getMessage(), "|";
    }
    echo $cancellation->isRequested() ? "parent-requested|" : "parent-open|";
    return 42;
});
echo $result;
"#,
    );
    assert_eq!(out, "parent-only|parent-requested|worker-null|42");
}

#[test]
/// A Fiber-owned root drains its workers before its exception escapes to the resumer.
fn parallel_scope_drains_before_a_fiber_root_exception_escapes() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$outer = new Fiber(static function (): int {
    return run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $tasks->spawn(static fn (): int => 42);
        throw new LogicException("fiber-root-failure");
    });
});
try {
    $outer->start();
} catch (LogicException $error) {
    echo $error->getMessage(), "|";
}
echo run(static fn (Elephc\Parallel\TaskGroup $tasks): int => 7);
"#,
    );
    assert_eq!(out, "fiber-root-failure|7");
}

#[test]
/// A retained TaskGroup is closed even when its root throws before ordinary completion.
fn parallel_task_group_retained_by_caller_is_closed_when_root_throws() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

$escaped = null;
try {
    run(static function (TaskGroup $tasks) use (&$escaped): int {
        $escaped = $tasks;
        $tasks->spawn(static fn (): int => 42);
        throw new LogicException("root-failure");
    });
} catch (LogicException $error) {
    echo $error->getMessage(), "|";
}
try {
    $escaped->cancel();
} catch (Error $error) {
    echo $error->getMessage(), "|";
}
echo run(static fn (TaskGroup $tasks): int => 7);
"#,
    );
    assert_eq!(
        out,
        "root-failure|Elephc Parallel TaskGroup cannot be used after its Parallel\\run() scope has closed|7"
    );
}

#[test]
/// A queued cancelled Future delivers worker-local cancellation instead of running its body.
fn parallel_group_cancel_makes_queued_future_throw_worker_local_cancellation() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): string {
    for ($i = 0; $i < 7; $i++) {
        $tasks->spawn(static function (): int {
            usleep(50000);
            return 1;
        });
    }
    $queued = $tasks->spawn(static fn (): int => 99);
    $tasks->cancel(new RuntimeException("parent-only"));
    echo $queued->isComplete() ? "complete|" : "pending|";
    try {
        $queued->join();
    } catch (Elephc\Async\CancelledException $cancelled) {
        return $cancelled->reason() === null ? "queued-null" : "queued-reason";
    }
    return "not-cancelled";
});
echo $result;
"#,
    );
    assert_eq!(out, "complete|queued-null");
}

#[test]
/// Joining a failure does not make teardown aggregate the same observed failure twice.
fn parallel_joined_failure_is_not_aggregated_again_at_scope_exit() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $future = $tasks->spawn(static function (): int {
        throw new RuntimeException("observed");
    });
    try {
        $future->join();
    } catch (Elephc\Parallel\TaskFailure $failure) {
        echo $failure->getMessage(), "|";
    }
    return 42;
});
echo $result;
"#,
    );
    assert_eq!(out, "observed|42");
}

#[test]
/// A worker exit becomes a typed fatal result while the parent process survives.
fn parallel_worker_exit_becomes_typed_php_fatal_without_exiting_parent() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

final class FatalWorkerPayload {
    public array $values;

    public function __construct() {
        $this->values = [["payload" => str_repeat("x", 64)]];
    }
}

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $tasks->spawn(static function (): int {
            $payload = new FatalWorkerPayload();
            $copy = $payload->values;
            exit(7);
        });
        return 0;
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    $failures = $groupFailure->failures();
    $failure = $failures[0];
    echo $failure->kind() === Elephc\Parallel\TaskFailureKind::PhpFatal ? "fatal|" : "wrong|";
    echo $failure->getMessage();
}
"#,
    );
    assert_eq!(out, "fatal|Parallel worker terminated with a PHP fatal");
}

#[test]
/// Fatal reentry during destructor/Buffer cleanup remains contained rather than killing the parent.
fn parallel_worker_fatal_reentry_from_destructor_with_buffer_stays_contained() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

final class ReentrantFatalOwner {
    public function __destruct() {
        buffer<int> $values = buffer_new<int>(32);
        $values[0] = 7;
        exit(9);
    }
}

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $tasks->spawn(static function (): int {
            $owner = new ReentrantFatalOwner();
            exit(7);
        });
        return 0;
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    $failure = $groupFailure->failures()[0];
    echo $failure->kind() === Elephc\Parallel\TaskFailureKind::PhpFatal ? "contained" : "wrong";
}
"#,
    );
    assert_eq!(out, "contained");
}

#[test]
/// User code cannot forge the authority used by private scope bookkeeping.
fn parallel_internal_bookkeeping_authority_is_private_to_the_scope() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $future = $tasks->spawn(static fn (): int => 42);
    try {
        $tasks->__recordFuture($future);
    } catch (Error $error) {
        echo "record|";
    }
    try {
        $tasks->__cancelAll();
    } catch (Error $error) {
        echo "cancel|";
    }
    try {
        $tasks->__drain();
    } catch (Error $error) {
        echo "drain|";
    }
    try {
        $tasks->__cleanupOnExit();
    } catch (Error $error) {
        echo "cleanup|";
    }
    try {
        $future->__scopeFailure();
    } catch (Error $error) {
        echo "future|";
    }
    return $future->join();
});
echo $result;
"#,
    );
    assert_eq!(out, "record|cancel|drain|cleanup|future|42");
}

#[test]
/// A reflection-forged group cannot acquire valid worker-side TaskGroup authority.
fn parallel_worker_cannot_use_a_reflection_forged_task_group() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

echo run(static function (TaskGroup $tasks): string {
    $future = $tasks->spawn(static function (): string {
        $reflection = new ReflectionClass(TaskGroup::class);
        $forged = $reflection->newInstanceWithoutConstructor();
        try {
            $forged->cancel();
            return "accepted";
        } catch (Error $error) {
            return $error->getMessage();
        }
    });
    return $future->join();
});
"#,
    );
    assert_eq!(
        out,
        "Elephc Parallel TaskGroup cannot be used from a Parallel worker"
    );
}

#[test]
/// A successfully settled Future retains its cached parent result after group teardown.
fn parallel_future_remains_cached_after_owning_scope_drains() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$future = run(static function (Elephc\Parallel\TaskGroup $tasks): Elephc\Parallel\Future {
    return $tasks->spawn(static fn (): int => 42);
});
echo $future->isComplete() ? "complete|" : "pending|";
echo $future->join(), "|", $future->join();
"#,
    );
    assert_eq!(out, "complete|42|42");
}

#[test]
/// An escaped failed Future preserves its parent-side failure after its scope drains.
fn parallel_escaped_failed_future_retains_its_parent_failure_after_scope_drain() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

final class FailedFutureBox {
    public Elephc\Parallel\Future $future;
}

$box = new FailedFutureBox();
try {
    run(static function (Elephc\Parallel\TaskGroup $tasks) use ($box): int {
        $box->future = $tasks->spawn(static function (): int {
            throw new LogicException("worker-boom");
        });
        return 0;
    });
} catch (Elephc\Parallel\TaskGroupFailure $groupFailure) {
    echo $box->future->isComplete() ? "complete|" : "pending|";
    try {
        $box->future->join();
    } catch (Elephc\Parallel\TaskFailure $failure) {
        echo $failure->remoteClass(), "|", $failure->getMessage();
    }
}
"#,
    );
    assert_eq!(out, "complete|LogicException|worker-boom");
}

#[test]
/// A runtime-resolved Async root cannot start inside a Parallel worker.
fn parallel_worker_dynamic_async_run_is_rejected_by_the_runtime_guard() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Async\run as async_run;
use function Elephc\Parallel\run;

if ($argc === -1) {
    async_run(static fn (Elephc\Async\TaskGroup $tasks): int => 1);
}

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $future = $tasks->spawn(static function (): int {
            $callback = "Elephc\\Async\\run";
            return call_user_func(
                $callback,
                static fn (Elephc\Async\TaskGroup $async): int => 1,
            );
        });
        return $future->join();
    });
} catch (Elephc\Parallel\TaskFailure $failure) {
    echo $failure->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Elephc\\Async\\run() cannot be called from a Parallel worker in v1"
    );
}

#[test]
/// Direct worker Fiber suspension produces the defined v1 rejection instead of stranding a child.
fn parallel_worker_fiber_suspend_fails_closed() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskFailure;
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

try {
    run(static function (TaskGroup $tasks): int {
        return $tasks->spawn(static function (): int {
            Fiber::suspend();
            return 0;
        })->join();
    });
} catch (TaskFailure $failure) {
    echo $failure->remoteClass(), "|", $failure->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Error|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1"
    );
}

#[test]
/// Runtime-resolved worker suspension is rejected before its argument side effects occur.
fn parallel_worker_runtime_resolved_fiber_suspend_fails_before_args() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskFailure;
use function Elephc\Parallel\run;

function worker_suspend_payload(): int {
    echo "argument-evaluated|";
    return 7;
}

$callbackName = $argc > 0 ? "Fiber::suspend" : "strlen";
try {
    run(static function (Elephc\Parallel\TaskGroup $tasks) use ($callbackName): int {
        return $tasks->spawn(static function () use ($callbackName): int {
            return array_map($callbackName, [worker_suspend_payload()])[0];
        })->join();
    });
} catch (TaskFailure $failure) {
    echo $failure->remoteClass(), "|", $failure->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Error|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1"
    );
}

#[test]
/// Spawning after cancellation fails before a native worker job is created.
fn parallel_spawn_after_cancellation_fails_before_creating_a_job() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): string {
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
        "Elephc Parallel TaskGroup cannot spawn after cancellation has been requested"
    );
}

#[test]
/// Uncaught child cancellation keeps cancellation identity rather than becoming TaskFailure.
fn parallel_uncaught_worker_cancellation_is_not_a_task_failure() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $tasks): int {
    $token = $tasks->cancellation();
    $tasks->spawn(static function () use (&$token): int {
        while (!$token->isRequested()) {
            usleep(1000);
        }
        $token->throwIfRequested();
        return 1;
    });
    usleep(10000);
    $tasks->cancel();
    return 42;
});
echo $result;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
/// A token retained beyond scope closure remains observation-only.
fn parallel_escaped_cancellation_token_is_observation_only_after_scope_close() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

$token = run(static function (Elephc\Parallel\TaskGroup $tasks): Elephc\Async\Cancellation {
    $token = $tasks->cancellation();
    $tasks->cancel(new RuntimeException("parent-only"));
    return $token;
});
echo $token->isRequested() ? "requested|" : "open|";
try {
    $token->throwIfRequested();
} catch (Elephc\Async\CancelledException $cancelled) {
    echo $cancelled->reason() === null ? "null" : "reason";
}
"#,
    );
    assert_eq!(out, "requested|null");
}

#[test]
/// A dynamically requested nested worker scope settles through the typed failure envelope.
fn parallel_worker_dynamic_nested_scope_becomes_a_typed_task_failure() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $tasks): int {
        $future = $tasks->spawn(static function (): int {
            $callback = "Elephc\\Parallel\\run";
            return call_user_func(
                $callback,
                static fn (Elephc\Parallel\TaskGroup $nested): int => 1,
            );
        });
        return $future->join();
    });
} catch (Elephc\Parallel\TaskFailure $failure) {
    echo $failure->remoteClass(), "|", $failure->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Error|Elephc\\Parallel\\run(): nested Parallel scopes are not supported inside a Parallel worker in v1"
    );
}

#[test]
/// A Parallel parent may run an Async root while retaining its own structured worker scope.
fn async_run_is_supported_inside_the_parallel_parent_root_body() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup as AsyncTaskGroup;
use function Elephc\Async\run as async_run;
use function Elephc\Parallel\run;

$result = run(static function (Elephc\Parallel\TaskGroup $parallel): int {
    return async_run(static function (AsyncTaskGroup $async): int {
        return 42;
    });
});
echo $result;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
/// Scheduler-managed owner-Fiber yields preserve Parallel ownership and child teardown.
fn parallel_run_allows_async_scheduler_yields_from_its_owner_fiber() {
    let out = compile_and_run(
r#"<?php
echo \Elephc\Async\run(static function (\Elephc\Async\TaskGroup $async): int {
    $parallel = \Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $parallel) use ($async): int {
        $async->reschedule();
        $async->sleep(0.001);
        $pair = stream_socket_pair(1, 1, 0);
        $reader = $pair[0];
        $writer = $pair[1];
        unset($pair);
        stream_set_blocking($reader, false);
        stream_set_blocking($writer, false);
        $writerTask = $async->spawn(static function () use ($async, $writer): int {
            $async->sleep(0.001);
            fwrite($writer, "x");
            return 1;
        });
        if (!$async->awaitReadable($reader, 1.0)) {
            throw new RuntimeException("Async descriptor wait timed out in a Parallel owner");
        }
        $ready = fread($reader, 1);
        $child = $async->spawn(static fn (): int => 1);
        $result = 40 + $writerTask->await() + $child->await();
        fclose($reader);
        fclose($writer);

        $pair = stream_socket_pair(1, 1, 0);
        $readSide = $pair[0];
        $writeSide = $pair[1];
        unset($pair);
        stream_set_blocking($readSide, false);
        stream_set_blocking($writeSide, false);
        $writeReady = $async->awaitWritable($writeSide, 1.0);
        fclose($readSide);
        fclose($writeSide);
        return $ready === "x" && $writeReady ? $result : -1;
    });
    return $parallel;
});
"#,
    );
    assert_eq!(out, "42");
}

#[test]
/// A Parallel root inside an Async task cannot recursively start a second Async root.
fn async_task_parallel_parent_root_cannot_start_another_async_root() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Async\TaskGroup as AsyncTaskGroup;

$result = \Elephc\Async\run(static function (AsyncTaskGroup $async): string {
    $child = $async->spawn(static function (): string {
        try {
            return \Elephc\Parallel\run(static function (Elephc\Parallel\TaskGroup $parallel): string {
                return \Elephc\Async\run(static function (AsyncTaskGroup $nested): string {
                    return "unreachable";
                });
            });
        } catch (Error $error) {
            return $error->getMessage();
        }
    });
    return $child->await();
});
echo $result;
"#,
    );
    assert_eq!(
        out,
        "Elephc\\Async\\run() cannot be nested or called from a Fiber"
    );
}

#[test]
/// An ordinary Fiber may synchronously own a Parallel root and retrieve its result.
fn parallel_run_is_supported_inside_an_ordinary_user_fiber() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

$fiber = new Fiber(static function (): int {
    return run(static function (TaskGroup $tasks): int {
        return $tasks->spawn(static fn (): int => 42)->join();
    });
});
$fiber->start();
echo $fiber->getReturn();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
/// An active parent scope blocks raw Fiber suspension before argument evaluation and leaks nothing.
fn parallel_parent_fiber_cannot_suspend_while_its_scope_is_active() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Parallel\Future;
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

function make_suspended_parallel_payload(TaskGroup $tasks, Future $future): array {
    echo "argument-evaluated|";
    return [$tasks, $future];
}

$outer = new Fiber(static function (): int {
    return run(static function (TaskGroup $tasks): int {
        $future = $tasks->spawn(static fn (): int => 42);
        try {
            Fiber::suspend(make_suspended_parallel_payload($tasks, $future));
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        $suspend = Fiber::suspend(...);
        try {
            $suspend();
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        return $future->join();
    });
});
$signal = $outer->start();
$result = $outer->getReturn();
unset($outer);
echo $signal ?? "completed", "|", $result, "|";
echo run(static fn (TaskGroup $tasks): int => 7);
"#,
    );
    assert!(
        out.success,
        "program failed: {}; stdout={:?}",
        out.stderr,
        out.stdout
    );
    assert_eq!(
        out.stdout,
        "Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|completed|42|7"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected rejected suspend argument ownership to be clean, got: {}",
        out.stderr
    );
}

#[test]
/// Runtime-resolved callable forms of parent suspension reach the same v1 guard.
fn parallel_parent_scope_rejects_runtime_resolved_fiber_suspend_callables() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

class OrdinaryCallable {
    public static function suspend(int $value): int {
        return $value;
    }
}

function make_runtime_suspend_payload(): int {
    echo "argument-evaluated|";
    return 7;
}

function make_runtime_suspend_array(bool $useFiber): array {
    return $useFiber
        ? [Fiber::class, "suspend"]
        : [OrdinaryCallable::class, "suspend"];
}

function make_runtime_suspend_mixed_array(bool $useFiber): mixed {
    return make_runtime_suspend_array($useFiber);
}

function make_runtime_mixed_suspend_name(): mixed {
    return "\\Fiber::suspend";
}

$callbackName = $argc > 0 ? "Fiber::suspend" : "strlen";
$callbackArray = make_runtime_suspend_array($argc > 0);
$callbackMixed = make_runtime_suspend_mixed_array($argc > 0);
$callbackMixedName = make_runtime_mixed_suspend_name();
$outer = new Fiber(static function () use ($callbackName, $callbackArray, $callbackMixed, $callbackMixedName): int {
    return run(static function (TaskGroup $tasks) use ($callbackName, $callbackArray, $callbackMixed, $callbackMixedName): int {
        $future = $tasks->spawn(static fn (): int => 42);
        try {
            call_user_func($callbackName, make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            call_user_func_array($callbackName, [make_runtime_suspend_payload()]);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            call_user_func($callbackArray, make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            call_user_func($callbackMixed, make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            call_user_func($callbackMixedName, make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        return $future->join();
    });
});
$outer->start();
$result = $outer->getReturn();
unset($outer);
unset($callbackArray);
unset($callbackMixed);
unset($callbackName);
unset($callbackMixedName);
echo $result;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|42"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected rejected dynamic callback ownership to be clean, got: {}",
        out.stderr
    );
}

#[test]
/// Builtin callback dispatch cannot bypass active-scope Fiber suspension checks.
fn parallel_parent_scope_rejects_fiber_suspend_through_builtin_callbacks() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

function runtime_fiber_suspend_name(): string {
    return "Fiber::suspend";
}

function make_callback_filter_input(): array {
    echo "filter-input-evaluated|";
    return [2];
}

function make_callback_regex_pattern(string $middle): string {
    echo "pattern-evaluated|";
    return "/" . $middle . "/";
}

function make_callback_regex_subject(): string {
    echo "subject-evaluated|";
    return "x";
}

$callback = runtime_fiber_suspend_name();
$guardMessage = "Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1";
$outer = new Fiber(static function () use ($callback, $guardMessage): int {
    return run(static function (TaskGroup $tasks) use ($callback, $guardMessage): int {
        $future = $tasks->spawn(static fn (): int => 42);
        try {
            array_map($callback, [1]);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            array_map(runtime_fiber_suspend_name(), [1]);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            array_map(callback: $callback, array: [1]);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            array_map(array: make_callback_filter_input(), callback: $callback);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            array_map($callback, ...[[1]]);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            array_filter([1], $callback);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            array_filter(make_callback_filter_input(), $callback);
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            preg_replace_callback("/./", $callback, "x");
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            preg_replace_callback(
                make_callback_regex_pattern("."),
                $callback,
                make_callback_regex_subject(),
            );
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        $extraRejected = 0;
        try {
            array_reduce([1], $callback, 0);
        } catch (Error $error) {
            if ($error->getMessage() === $guardMessage) {
                $extraRejected++;
            } else {
                echo "unexpected-error|";
            }
            unset($error);
        }
        $walkInput = [2];
        try {
            array_walk($walkInput, $callback);
        } catch (Error $error) {
            if ($error->getMessage() === $guardMessage) {
                $extraRejected++;
            } else {
                echo "unexpected-error|";
            }
            unset($error);
        }
        $walkRecursiveInput = ["key" => [3]];
        try {
            array_walk_recursive($walkRecursiveInput, Fiber::suspend(...));
        } catch (Error $error) {
            if ($error->getMessage() === $guardMessage) {
                $extraRejected++;
            } else {
                echo "unexpected-error|";
            }
            unset($error);
        }
        $sortInput = [3, 1, 2];
        try {
            usort($sortInput, $callback);
        } catch (Error $error) {
            if ($error->getMessage() === $guardMessage) {
                $extraRejected++;
            } else {
                echo "unexpected-error|";
            }
            unset($error);
        }
        unset($walkInput);
        unset($walkRecursiveInput);
        unset($sortInput);
        echo $extraRejected, "|";
        return $future->join();
    });
});
$outer->start();
$result = $outer->getReturn();
unset($outer);
unset($callback);
echo $result;
"#,
    );
    assert!(
        out.success,
        "program failed: {}; stdout={:?}",
        out.stderr,
        out.stdout
    );
    let guard_message =
        "Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1";
    let expected = [
        guard_message,
        guard_message,
        guard_message,
        "filter-input-evaluated",
        guard_message,
        guard_message,
        guard_message,
        "filter-input-evaluated",
        guard_message,
        guard_message,
        "pattern-evaluated",
        guard_message,
        "4",
        "42",
    ]
    .join("|");
    assert_eq!(out.stdout, expected);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected rejected higher-order callback ownership to be clean, got: {}",
        out.stderr
    );
}

#[test]
/// A dynamic worker callable's string return crosses arenas with balanced heap ownership.
fn parallel_dynamic_callable_string_return_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function append_runtime_suffix(string $value): string {
    return $value . "!";
}

$callback = "append_runtime_suffix";
echo call_user_func($callback, "value");
"#,
    );
    assert!(out.success, "program failed: {}; stdout={:?}", out.stderr, out.stdout);
    assert_eq!(out.stdout, "value!");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected nested callable string ownership to be clean, got: {}",
        out.stderr
    );
}

#[test]
/// An instance callable array targeting Fiber suspension is rejected by the parent guard.
fn parallel_parent_scope_rejects_fiber_instance_suspend_callable_array() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

function runtime_fiber_instance_suspend_callable(): mixed {
    $fiber = new Fiber(static function (): void {});
    return [$fiber, "suspend"];
}

$callback = runtime_fiber_instance_suspend_callable();
$outer = new Fiber(static function () use ($callback): int {
    return run(static function (TaskGroup $tasks) use ($callback): int {
        try {
            call_user_func($callback);
        } catch (Error $error) {
            echo $error->getMessage();
            return 0;
        }
        return 1;
    });
});
$outer->start();
$outer->getReturn();
"#,
    );
    assert_eq!(
        out,
        "Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1"
    );
}

#[test]
/// Suspension checks preserve ordinary callback descriptor dispatch and ownership.
fn parallel_parent_scope_preserves_non_suspend_callback_descriptors() {
    let out = compile_and_run(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

class OrdinarySuspendCallback {
    public static function suspend(int $value): int {
        return $value;
    }
}

function runtime_ordinary_callback(): callable {
    return OrdinarySuspendCallback::suspend(...);
}

function runtime_ordinary_callback_array(): array {
    return [OrdinarySuspendCallback::class, "suspend"];
}

function runtime_ordinary_mixed_callback(): mixed {
    return runtime_ordinary_callback();
}

function runtime_ordinary_mixed_callback_array(): mixed {
    return runtime_ordinary_callback_array();
}

$callable = runtime_ordinary_callback();
$callbackArray = runtime_ordinary_callback_array();
$mixedCallable = runtime_ordinary_mixed_callback();
$mixedArray = runtime_ordinary_mixed_callback_array();
$ordinaryClosure = static fn (int $value): int => $value;
$outer = new Fiber(static function () use ($callable, $callbackArray, $mixedCallable, $mixedArray, $ordinaryClosure): int {
    return run(static function (TaskGroup $tasks) use ($callable, $callbackArray, $mixedCallable, $mixedArray, $ordinaryClosure): int {
        $arrayResult = call_user_func($callbackArray, 11);
        $mixedArrayResult = call_user_func($mixedArray, 12);
        $descriptorResult = $callable(13);
        $mixedDescriptorResult = call_user_func($mixedCallable, 14);
        $closureResult = call_user_func($ordinaryClosure, 15);
        return $arrayResult
            + $mixedArrayResult
            + $descriptorResult
            + $mixedDescriptorResult
            + $closureResult;
    });
});
$outer->start();
$result = $outer->getReturn();
unset($outer);
unset($callable);
unset($callbackArray);
unset($mixedCallable);
unset($mixedArray);
unset($ordinaryClosure);
echo $result;
"#,
    );
    assert_eq!(out, "65");
}

#[test]
/// The bridge guard also rejects dynamically resolved raw suspension inside an Async task.
fn parallel_bridge_also_guards_dynamic_fiber_suspend_inside_async_tasks() {
    let out = compile_and_run(
        r#"<?php

function parallel_async_fiber_suspend_name(): string {
    return "Fiber::suspend";
}

\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int { return 1; });
$callback = parallel_async_fiber_suspend_name();
try {
    \Elephc\Async\run(static function (\Elephc\Async\TaskGroup $tasks) use ($callback): int {
        $tasks->spawn(static function () use ($callback): void {
            call_user_func($callback);
        });
        return 1;
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

#[test]
/// Both scheduler preludes compose without declaration or namespace conflicts.
fn parallel_and_async_scheduler_preludes_compile_together() {
    let out = compile_and_run(
        r#"<?php
$parallelResult = \Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int { return 1; });
$result = \Elephc\Async\run(static function (\Elephc\Async\TaskGroup $tasks): int { return 42; });
echo $parallelResult . $result;
"#,
    );
    assert_eq!(out, "142");
}

#[test]
/// Escaped first-class suspension callables fail before argument evaluation in the parent scope.
fn parallel_parent_scope_rejects_escaped_first_class_fiber_suspend_before_args() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

function make_runtime_suspend_payload(): int {
    echo "argument-evaluated|";
    return 7;
}

function make_runtime_suspend_callable(): callable {
    return Fiber::suspend(...);
}

function make_runtime_suspend_mixed_callable(): mixed {
    return Fiber::suspend(...);
}

$firstClass = make_runtime_suspend_callable();
$mixedFirstClass = make_runtime_suspend_mixed_callable();
$outer = new Fiber(static function () use ($firstClass, $mixedFirstClass): int {
    return run(static function (TaskGroup $tasks) use ($firstClass, $mixedFirstClass): int {
        $future = $tasks->spawn(static fn (): int => 42);
        try {
            call_user_func(Fiber::suspend(...), make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            $firstClass(make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        try {
            call_user_func($mixedFirstClass, make_runtime_suspend_payload());
        } catch (Error $error) {
            echo $error->getMessage(), "|";
            unset($error);
        }
        return $future->join();
    });
});
$outer->start();
$result = $outer->getReturn();
unset($outer);
unset($firstClass);
unset($mixedFirstClass);
echo $result;
"#,
    );
    assert!(
        out.success,
        "program failed: {}; stdout={:?}",
        out.stderr,
        out.stdout
    );
    assert_eq!(
        out.stdout,
        "Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1|42"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected first-class callback ownership to be clean, got: {}",
        out.stderr
    );
}

#[test]
/// The active parent root rejects a dynamically resolved nested Parallel root.
fn parallel_parent_root_rejects_dynamic_nested_scope() {
    let out = compile_and_run(
        r#"<?php
use function Elephc\Parallel\run;

try {
    run(static function (Elephc\Parallel\TaskGroup $outer): int {
        $callback = "Elephc\\Parallel\\run";
        return call_user_func(
            $callback,
            static fn (Elephc\Parallel\TaskGroup $inner): int => 1,
        );
    });
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Elephc\\Parallel\\run(): nested Parallel scopes are not supported in a parent root in v1"
    );
}
