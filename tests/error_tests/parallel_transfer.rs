//! Purpose:
//! Compile-time diagnostics for values attempting to cross a Parallel context boundary.
//!
//! Called from:
//! - `cargo test --test error_tests parallel_transfer`.
//!
//! Key details:
//! - A minimal local declaration isolates checker behavior before the runtime surface lands.
//! - Objects, resources, mixed shapes, receiver closures, and ordinary references fail closed.

use super::*;

const PARALLEL_DECLARATIONS: &str = r#"
namespace Elephc\Parallel {
    final class Future {}
    final class TaskGroup {
        public function spawn(\Closure $task, mixed ...$args): Future {
            return new Future();
        }
    }
}
namespace {
"#;

/// Checks isolated worker diagnostics using a minimal local TaskGroup declaration.
fn expect_parallel_error(body: &str, expected: &str) {
    let source = format!("<?php\n{PARALLEL_DECLARATIONS}{body}\n}}\n");
    expect_error(&source, expected);
}

/// Checks diagnostics through the actual injected Parallel root and scope surface.
fn expect_parallel_surface_error(body: &str, expected: &str) {
    let source = format!("<?php\n{body}\n");
    expect_error(&source, expected);
}

/// Rejects process-global access reached through string conversion inside a worker body.
#[test]
fn parallel_string_entry_still_rejects_stringifier_globals_inside_worker_body() {
    expect_parallel_error(r#"
class GlobalText {
    public function __toString(): string {
        global $observed;
        $observed = 1;
        return 'unsafe';
    }
}
function consume_text(string $text): string { return $text; }
function worker_body(): string { return consume_text(new GlobalText()); }
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(worker_body(...));
"#, "task may access process-global PHP storage");
}

/// Inserts one worker path operation and requires refusal of shared PHP wrapper dispatch.
fn expect_user_wrapper_path_error(operation: &str) {
    let body = r#"
final class WorkerWrapper {}
stream_wrapper_register("parallel.wrapper", "WorkerWrapper");
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    __PATH_OPERATION__
    return 0;
});
"#
    .replace("__PATH_OPERATION__", operation);
    expect_parallel_error(&body, "task may access process-global PHP storage");
}

/// Uses the image checker to verify wrapper-aware worker inputs fail closed.
fn expect_parallel_image_path_error(operation: &str) {
    let body = r#"
final class WorkerWrapper {}
stream_wrapper_register("parallel.wrapper", "WorkerWrapper");
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    __PATH_OPERATION__
    return 0;
});
"#
    .replace("__PATH_OPERATION__", operation);
    let source = format!("<?php\n{PARALLEL_DECLARATIONS}{body}\n}}\n");
    let error = crate::image::check_image(&source)
        .err()
        .expect("image wrapper path in a Parallel worker must fail closed");
    assert!(
        error.contains("task may access process-global PHP storage"),
        "unexpected error: {error}"
    );
}

// Private TaskGroup/Future clone refusal is catchable at runtime, as in PHP. Its authority
// and original-job lifetime gate lives in parallel_execution's heap-debug clone regression.

/// Rejects object arguments that would retain pointers into the parent's PHP heap.
#[test]
fn parallel_rejects_an_object_argument() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$object = new \stdClass();
$tasks->spawn(static fn (\stdClass $value): int => 1, $object);
"#,
        "argument #1 cannot cross an isolated runtime context: object of class stdClass",
    );
}

/// Rejects object captures across the isolated worker context boundary.
#[test]
fn parallel_rejects_an_object_capture() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$object = new \stdClass();
$tasks->spawn(static function () use ($object): int { return 1; });
"#,
        "capture $object cannot cross an isolated runtime context: object of class stdClass",
    );
}

/// Limits shared by-reference captures to the dedicated Cancellation capability.
#[test]
fn parallel_rejects_a_scalar_capture_by_reference() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$value = 1;
$tasks->spawn(static function () use (&$value): int { return $value; });
"#,
        "capture &$value cannot cross an isolated runtime context; only Elephc\\Async\\Cancellation may be shared by reference",
    );
}

/// Fails closed when an opaque Mixed argument has no proven transferable runtime shape.
#[test]
fn parallel_rejects_an_unresolved_mixed_argument() {
    expect_parallel_error(
        r#"
function submit(\Elephc\Parallel\TaskGroup $tasks, mixed $value): void {
    $tasks->spawn(static fn (mixed $copy): int => 1, $value);
}
"#,
        "argument #1 cannot cross an isolated runtime context: mixed value whose runtime shape is not proven",
    );
}

/// Rejects a worker result whose declared object type cannot cross isolated heaps.
#[test]
fn parallel_rejects_an_object_return_value() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): \stdClass => new \stdClass());
"#,
        "task return value cannot cross an isolated runtime context: object of class stdClass",
    );
}

/// Typed worker-local arrays retain their reference-alias refusal at the exact return site.
#[test]
fn parallel_rejects_reference_aliased_worker_local_array_return() {
    expect_parallel_surface_error(
        r#"
use function Elephc\Parallel\run;
run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): array {
        $values = [1];
        $alias =& $values;
        return $values;
    });
    return 0;
});
"#,
        "task return value cannot cross an isolated runtime context: reference-aliased arrays may contain cycles",
    );
}

/// A closure variable cannot bypass a failed local alias proof through its declared array type.
#[test]
fn parallel_rejects_reference_aliased_worker_return_through_a_closure_variable() {
    expect_parallel_surface_error(
        r#"
use function Elephc\Parallel\run;
run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $worker = static function (): array {
        $values = [1];
        $alias =& $values;
        return $values;
    };
    $tasks->spawn($worker);
    return 0;
});
"#,
        "task return value cannot cross an isolated runtime context: mixed value whose runtime shape is not proven",
    );
}

/// Rejects first-class method closures carrying a parent-heap receiver object.
#[test]
fn parallel_rejects_a_receiver_bound_first_class_callable() {
    expect_parallel_error(
        r#"
final class Worker { public function task(): int { return 1; } }
$tasks = new \Elephc\Parallel\TaskGroup();
$worker = new Worker();
$tasks->spawn($worker->task(...));
"#,
        "receiver-bound Closure captures an object and cannot cross an isolated runtime context",
    );
}

/// Checks nested literal array elements rather than accepting only the outer array type.
#[test]
fn parallel_rejects_an_object_hidden_in_a_literal_array_argument() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (array $values): int => 1, [1, [new \stdClass()]]);
"#,
        "argument #1 array element cannot cross an isolated runtime context: object of class stdClass",
    );
}

/// Keeps unknown result-shape rejection through assigned and aliased task closures.
#[test]
fn parallel_rejects_an_object_hidden_in_an_assigned_closure_array_return() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$task = static fn (): array => [1, new \stdClass()];
$copy = $task;
$tasks->spawn($copy);
"#,
        "task return value cannot cross an isolated runtime context: mixed value whose runtime shape is not proven",
    );
}

/// Rejects reference-aliased cyclic arrays before a serializer can traverse them.
#[test]
fn parallel_rejects_a_cyclic_array_before_runtime_serialization() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$cycle = [];
$alias =& $cycle;
$cycle[] = $alias;
$tasks->spawn(static fn (array $value): int => 1, $cycle);
"#,
        "argument #1 cannot cross an isolated runtime context: reference-aliased arrays may contain cycles",
    );
}

/// Applies cyclic/reference-array refusal to captures on the injected public surface.
#[test]
fn parallel_rejects_a_reference_aliased_array_capture_before_runtime_serialization() {
    expect_parallel_surface_error(
        r#"
use function Elephc\Parallel\run;
run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $cycle = [];
    $alias =& $cycle;
    $cycle[] = $alias;
    $tasks->spawn(static function () use ($cycle): int { return 1; });
    return 0;
});
"#,
        "capture $cycle cannot cross an isolated runtime context: reference-aliased arrays may contain cycles",
    );
}

/// Pins the explicit diagnostic for variadic task targets unsupported by the v1 worker ABI.
#[test]
fn parallel_rejects_variadic_task_outside_the_v1_worker_abi() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (int ...$values): int => count($values), 1, 2);
"#,
        "variadic task Closures are not supported by the v1 worker ABI",
    );
}

/// Pins the v1 worker ABI refusal for first-class static-method task targets.
#[test]
fn parallel_rejects_static_method_task_outside_the_v1_worker_abi() {
    expect_parallel_error(
        r#"
final class Worker { public static function task(): int { return 1; } }
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(Worker::task(...));
"#,
        "static-method Closure targets are not supported by the v1 worker ABI",
    );
}

/// Rejects direct entry into a second Parallel root from a worker.
#[test]
fn parallel_rejects_a_direct_nested_parallel_scope() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        return \Elephc\Parallel\run(static fn (\Elephc\Parallel\TaskGroup $nested): int => 1);
    });
    return 0;
});
"#,
        "task may enter Elephc\\Parallel\\run(); nested Parallel scopes are not supported inside a Parallel worker in v1",
    );
}

/// Rejects an Async root directly entered from a Parallel worker.
#[test]
fn parallel_rejects_an_async_root_scope_inside_a_worker() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        return \Elephc\Async\run(static fn (\Elephc\Async\TaskGroup $async): int => 1);
    });
    return 0;
});
"#,
        "task may enter Elephc\\Async\\run(); Async root scopes inside Parallel workers are not supported in v1",
    );
}

/// Propagates nested Parallel-root effects through ordinary worker helper calls.
#[test]
fn parallel_rejects_a_transitively_nested_parallel_scope() {
    expect_parallel_surface_error(
        r#"
function nested_scope(): int {
    return \Elephc\Parallel\run(static fn (\Elephc\Parallel\TaskGroup $nested): int => 1);
}
function outer_worker(): int { return nested_scope(); }
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(outer_worker(...));
    return 0;
});
"#,
        "task may enter Elephc\\Parallel\\run(); nested Parallel scopes are not supported inside a Parallel worker in v1",
    );
}

/// Propagates forbidden Async-root entry through the worker's transitive call graph.
#[test]
fn parallel_rejects_a_transitively_async_root_scope_inside_a_worker() {
    expect_parallel_surface_error(
        r#"
function nested_async_scope(): int {
    return \Elephc\Async\run(static fn (\Elephc\Async\TaskGroup $async): int => 1);
}
function outer_worker(): int { return nested_async_scope(); }
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(outer_worker(...));
    return 0;
});
"#,
        "task may enter Elephc\\Async\\run(); Async root scopes inside Parallel workers are not supported in v1",
    );
}

/// Keeps nested-root refusal through local closure assignment and aliasing.
#[test]
fn parallel_rejects_a_nested_scope_in_an_assigned_closure() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $worker = static function (): int {
        return \Elephc\Parallel\run(static fn (\Elephc\Parallel\TaskGroup $nested): int => 1);
    };
    $copy = $worker;
    $tasks->spawn($copy);
    return 0;
});
"#,
        "task may enter Elephc\\Parallel\\run(); nested Parallel scopes are not supported inside a Parallel worker in v1",
    );
}

/// Propagates PHP global-variable access through user-function task targets.
#[test]
fn parallel_rejects_transitive_global_variable_access() {
    expect_parallel_error(
        r#"
$shared = 41;
function read_shared(): int { global $shared; return $shared; }
function global_worker(): int { return read_shared() + 1; }
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(global_worker(...));
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Rejects worker reads of mutable process-wide environment state.
#[test]
fn parallel_rejects_process_global_environment_reads() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static fn (): string => (string) getenv("PATH"));
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Rejects worker reads of the process-wide default timezone.
#[test]
fn parallel_rejects_process_global_timezone_reads() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static fn (): string => date_default_timezone_get());
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Rejects JSON-error and time-parser state that is not isolated per worker.
#[test]
fn parallel_rejects_process_global_json_and_time_scratch() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static fn (): int => json_last_error());
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static fn (): int => (int) strtotime("tomorrow"));
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Rejects stream-context runtime scratch that remains process-global.
#[test]
fn parallel_rejects_process_global_stream_context_scratch() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        $context = stream_context_create();
        return 1;
    });
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Rejects enum singleton reads backed by process-global cached object slots.
#[test]
fn parallel_rejects_enum_case_singletons_cached_outside_the_worker_context() {
    expect_parallel_surface_error(
        r#"
enum WorkerColor { case Red; case Blue; }
function read_worker_enum_case(): string { return WorkerColor::Red->name; }
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(read_worker_enum_case(...));
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Rejects enum cases() factories that populate shared singleton storage.
#[test]
fn parallel_rejects_enum_case_factories_that_fill_global_singleton_slots() {
    expect_parallel_surface_error(
        r#"
enum WorkerTone { case Warm; case Cool; }
function count_worker_enum_cases(): int { return count(WorkerTone::cases()); }
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(count_worker_enum_cases(...));
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Rejects opaque external task targets without a reviewed worker-safety contract.
#[test]
fn parallel_rejects_first_class_unclassified_external_functions() {
    expect_parallel_surface_error(
        r#"
extern function native_set_state(string $name): int;
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(native_set_state(...), "worker-state");
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Propagates unclassified native-call effects through a user-function worker target.
#[test]
fn parallel_rejects_transitive_unclassified_external_calls() {
    expect_parallel_surface_error(
        r#"
extern function native_set_state(string $name): int;
function mutate_native_state(): int { return native_set_state("worker-state"); }
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(mutate_native_state(...));
    return 0;
});
"#,
        "task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
    );
}

/// Conservatively rejects captured string-callable dispatch that can reach PHP globals.
#[test]
fn parallel_rejects_an_unknown_string_callable_that_may_read_globals() {
    expect_parallel_surface_error(
        r#"
$shared = 41;
function read_global_from_string_callback(): int { global $shared; return $shared; }
$callback = "read_global_from_string_callback";
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks) use ($callback): int {
    $tasks->spawn(static function () use ($callback): int { return (int) $callback(); });
    return 0;
});
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Preserves global-access refusal when a string callable uses different letter casing.
#[test]
fn parallel_rejects_case_insensitive_string_callable_that_may_read_globals() {
    expect_parallel_surface_error(
        r#"
$shared = 41;
function read_global_from_string_callback(): int { global $shared; return $shared; }
$callback = "READ_GLOBAL_FROM_STRING_CALLBACK";
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks) use ($callback): int {
    $tasks->spawn(static function () use ($callback): int { return (int) $callback(); });
    return 0;
});
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Prevents user PHP from calling compiler-only native parent-scope helpers.
#[test]
fn parallel_bridge_externs_are_not_callable_from_user_php() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    \elephc_parallel_parent_scope_leave();
    return 0;
});
"#,
        "Extern function 'elephc_parallel_parent_scope_leave' is an internal Elephc Parallel bridge function and cannot be called from user PHP",
    );
}

/// Keeps compiler-only bridge helpers private through call_user_func string dispatch.
#[test]
fn parallel_bridge_externs_are_not_callable_through_a_string_callback() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    return call_user_func("elephc_parallel_parent_scope_leave");
});
"#,
        "Extern function 'elephc_parallel_parent_scope_leave' is an internal Elephc Parallel bridge function and cannot be called from user PHP",
    );
}

/// Prevents user code from invoking private native-job completion serialization helpers.
#[test]
fn parallel_serialization_helpers_are_not_callable_from_user_php() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    \Elephc\Parallel\TaskGroup::__completeJobValue(1, "forged");
    return 0;
});
"#,
        "Cannot access private method: Elephc\\Parallel\\TaskGroup::__completeJobValue",
    );
}

/// Rejects task bodies that mutate process-global static local storage.
#[test]
fn parallel_rejects_static_local_access() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    static $calls = 0;
    return ++$calls;
});
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Rejects task reads from class static properties shared across worker heaps.
#[test]
fn parallel_rejects_static_property_access() {
    expect_parallel_error(
        r#"
final class SharedState { public static int $value = 42; }
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): int => SharedState::$value);
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Checks transitive PHP globals through the actual injected Parallel public API.
#[test]
fn parallel_public_surface_rejects_transitive_global_variable_access() {
    expect_parallel_surface_error(
        r#"
$shared = 41;
function read_shared_public(): int { global $shared; return $shared; }
function global_worker_public(): int { return read_shared_public() + 1; }

\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(global_worker_public(...));
    return 0;
});
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Checks static-local refusal through the injected run/TaskGroup surface.
#[test]
fn parallel_public_surface_rejects_static_local_access() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        static $calls = 0;
        return ++$calls;
    });
    return 0;
});
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Checks shared class-property refusal through the injected Parallel root.
#[test]
fn parallel_public_surface_rejects_static_property_access() {
    expect_parallel_surface_error(
        r#"
final class SharedStatePublic { public static int $value = 42; }
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static fn (): int => SharedStatePublic::$value);
    return 0;
});
"#,
        "task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
    );
}

/// Rejects worker fopen dispatch through a parent-owned PHP wrapper registry.
#[test]
fn parallel_rejects_user_wrapper_registry_access() {
    expect_parallel_error(
        r#"
final class WorkerWrapper {
    public function stream_open(string $path, string $mode, int $options): bool { return true; }
}
stream_wrapper_register("parallel.wrapper", "WorkerWrapper");
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): mixed => fopen("parallel.wrapper://resource", "r+"));
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker changes to the process-wide BCMath scale.
#[test]
fn parallel_rejects_bcscale_process_global_write_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): int => bcscale(0));
"#,
        "task may access process-global PHP storage",
    );
}

/// Includes callback re-entry into PHP globals in external-function safety analysis.
#[test]
fn parallel_rejects_extern_calls_that_can_reenter_a_php_callable() {
    expect_parallel_surface_error(
        r#"
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;
extern function signal(int $sig, callable $handler): ptr;
function register_process_signal_handler(callable $handler): void {
    signal(15, $handler);
}
$result = run(static function (TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        register_process_signal_handler(static function (int $sig): void {
            global $callbackState;
            $callbackState = 42;
        });
        return 0;
    });
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Checks external task targets whose callback arguments can access parent PHP globals.
#[test]
fn parallel_rejects_an_extern_callable_task_that_accepts_callbacks() {
    expect_parallel_surface_error(
        r#"
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;
extern function elephc_invoke_callback(callable $callback): int;
function read_process_global_callback(): int {
    global $callbackState;
    return $callbackState;
}
run(static function (TaskGroup $tasks): int {
    return $tasks->spawn(
        elephc_invoke_callback(...),
        "read_process_global_callback",
    )->await();
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker changes to the process-wide current directory.
#[test]
fn parallel_rejects_chdir_process_global_write_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): bool => chdir("."));
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker changes to process-wide environment variables.
#[test]
fn parallel_rejects_putenv_process_global_write_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): bool => putenv("ELEPHC_PARALLEL=1"));
"#,
        "task may access mutable process-global runtime state or an unclassified external function",
    );
}

/// Rejects worker changes to the shared default timezone.
#[test]
fn parallel_rejects_timezone_process_global_write_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): bool => date_default_timezone_set("UTC"));
"#,
        "task may access mutable process-global runtime state or an unclassified external function",
    );
}

/// Rejects worker changes to the process-wide file-creation mask.
#[test]
fn parallel_rejects_umask_process_global_write_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): int => umask(0));
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects process-wide alarm scheduling from the injected worker surface.
#[test]
fn parallel_rejects_pcntl_alarm_process_global_write_in_worker() {
    expect_parallel_surface_error(
        r#"
use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;
run(static function (TaskGroup $tasks): void {
    $tasks->spawn(static fn (): int => pcntl_alarm(1));
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker changes to shared iconv encoding configuration.
#[test]
fn parallel_rejects_iconv_encoding_process_global_write_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): bool => iconv_set_encoding("input_encoding", "UTF-8"));
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects file-stat queries that can call a shared PHP wrapper's url_stat method.
#[test]
fn parallel_rejects_user_wrapper_stat_access() {
    expect_parallel_error(
        r#"
final class WorkerWrapper {
    public function url_stat(string $path, int $flags): array { return []; }
}
stream_wrapper_register("parallel.wrapper", "WorkerWrapper");
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): bool => file_exists("parallel.wrapper://resource"));
"#,
        "task may access process-global PHP storage",
    );
}

/// Checks wrapper-aware file_get_contents against the worker isolation boundary.
#[test]
fn parallel_rejects_user_wrapper_file_get_contents() {
    expect_user_wrapper_path_error("file_get_contents(\"parallel.wrapper://resource\");");
}

/// Checks wrapper-aware line-array reads against the worker isolation boundary.
#[test]
fn parallel_rejects_user_wrapper_file() {
    expect_user_wrapper_path_error("file(\"parallel.wrapper://resource\");");
}

/// Checks hash_file's path argument for shared PHP wrapper dispatch.
#[test]
fn parallel_rejects_user_wrapper_hash_file() {
    expect_user_wrapper_path_error("hash_file(\"md5\", \"parallel.wrapper://resource\");");
}

/// Rejects wrapper-aware writes through shared parent PHP state.
#[test]
fn parallel_rejects_user_wrapper_file_put_contents() {
    expect_user_wrapper_path_error(
        "file_put_contents(\"parallel.wrapper://resource\", \"payload\");",
    );
}

/// Checks every covered image input builtin for shared PHP wrapper dispatch.
#[test]
fn parallel_rejects_user_wrapper_image_inputs() {
    for operation in [
        "getimagesize(\"parallel.wrapper://image\");",
        "imagecreatefrombmp(\"parallel.wrapper://image\");",
        "imagecreatefromgif(\"parallel.wrapper://image\");",
        "imagecreatefromjpeg(\"parallel.wrapper://image\");",
        "imagecreatefrompng(\"parallel.wrapper://image\");",
        "imagecreatefromtga(\"parallel.wrapper://image\");",
        "imagecreatefromwebp(\"parallel.wrapper://image\");",
    ] {
        expect_parallel_image_path_error(operation);
    }
}

/// Checks copy's destination path as well as its source for wrapper dispatch.
#[test]
fn parallel_rejects_user_wrapper_copy_destination() {
    expect_user_wrapper_path_error("copy(\"local-source\", \"parallel.wrapper://destination\");");
}

/// Rejects shared wrapper dispatch through copy's source path.
#[test]
fn parallel_rejects_user_wrapper_copy_source() {
    expect_user_wrapper_path_error("copy(\"parallel.wrapper://source\", \"local-destination\");");
}

/// Rejects shared wrapper dispatch through rename's source path.
#[test]
fn parallel_rejects_user_wrapper_rename_source() {
    expect_user_wrapper_path_error("rename(\"parallel.wrapper://source\", \"local-destination\");");
}

/// Rejects wrapper-aware directory enumeration from a worker.
#[test]
fn parallel_rejects_user_wrapper_directory_open() {
    expect_user_wrapper_path_error("scandir(\"parallel.wrapper://directory\");");
}

/// Checks path mutation and ownership builtins for shared wrapper dispatch.
#[test]
fn parallel_rejects_user_wrapper_path_mutations() {
    for operation in [
        "unlink(\"parallel.wrapper://path\");",
        "mkdir(\"parallel.wrapper://path\");",
        "rmdir(\"parallel.wrapper://path\");",
        "touch(\"parallel.wrapper://path\");",
        "chmod(\"parallel.wrapper://path\", 0);",
        "chown(\"parallel.wrapper://path\", 0);",
        "chgrp(\"parallel.wrapper://path\", 0);",
        "lchown(\"parallel.wrapper://path\", 0);",
        "lchgrp(\"parallel.wrapper://path\", 0);",
    ] {
        expect_user_wrapper_path_error(operation);
    }
}

/// Fails closed when a runtime path may select a shared wrapper scheme.
#[test]
fn parallel_rejects_unknown_user_wrapper_path_access() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (string $path): bool => file_exists($path), "runtime://path");
"#,
        "task may access process-global PHP storage",
    );
}

/// Checks rename's destination independently for possible wrapper dispatch.
#[test]
fn parallel_rejects_user_wrapper_rename_destination() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static fn (): bool => rename("local-file", "runtime://path"));
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects adding entries to the shared PHP stream-wrapper registry in a worker.
#[test]
fn parallel_rejects_stream_wrapper_registration_in_worker() {
    expect_parallel_surface_error(
        r#"
final class WorkerWrapper {
    public function stream_open(string $path, string $mode, int $options): bool { return true; }
}
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        stream_wrapper_register("parallel.worker", "WorkerWrapper");
        return 0;
    });
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker removal of shared stream-wrapper registrations.
#[test]
fn parallel_rejects_stream_wrapper_unregistration_in_worker() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static fn (): bool => stream_wrapper_unregister("file"));
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker restoration of process-wide stream-wrapper defaults.
#[test]
fn parallel_rejects_stream_wrapper_restore_in_worker() {
    expect_parallel_error(
        r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    stream_wrapper_restore("file");
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects worker reads of shared wrapper and filter registry inventories.
#[test]
fn parallel_rejects_stream_registry_introspection_in_worker() {
    for operation in ["stream_get_wrappers();", "stream_get_filters();"] {
        let body = r#"
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    __STREAM_QUERY__
    return 0;
});
"#
        .replace("__STREAM_QUERY__", operation);
        expect_parallel_error(&body, "task may access process-global PHP storage");
    }
}

/// Rejects worker registration of PHP filters in the shared registry.
#[test]
fn parallel_rejects_stream_filter_registration_in_worker() {
    expect_parallel_surface_error(
        r#"
final class WorkerFilter {}
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): int {
    $tasks->spawn(static function (): int {
        stream_filter_register("parallel.worker", "WorkerFilter");
        return 0;
    });
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects appending a parent-registered PHP stream filter inside a worker.
#[test]
fn parallel_rejects_attaching_a_registered_user_filter_in_worker() {
    expect_parallel_error(
        r#"
final class WorkerFilter {}
stream_filter_register("parallel.worker", "WorkerFilter");
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    $stream = fopen("parallel-filter.tmp", "w+");
    stream_filter_append($stream, "parallel.worker");
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Rejects prepending a parent-registered PHP stream filter inside a worker.
#[test]
fn parallel_rejects_prepending_a_registered_user_filter_in_worker() {
    expect_parallel_error(
        r#"
final class WorkerFilter {}
stream_filter_register("parallel.worker", "WorkerFilter");
$tasks = new \Elephc\Parallel\TaskGroup();
$tasks->spawn(static function (): int {
    $stream = fopen("parallel-filter.tmp", "w+");
    stream_filter_prepend($stream, "parallel.worker");
    return 0;
});
"#,
        "task may access process-global PHP storage",
    );
}

/// Keeps Future construction private rather than accepting forged native job ids.
#[test]
fn parallel_future_cannot_be_forged_from_a_native_job_id() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static fn (\Elephc\Parallel\TaskGroup $tasks): int => 0);
$future = new \Elephc\Parallel\Future(1);
"#,
        "Cannot access private constructor: Elephc\\Parallel\\Future::__construct",
    );
}

/// Keeps TaskGroup construction under the owning Parallel run root.
#[test]
fn parallel_task_group_cannot_be_constructed_outside_run() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static fn (\Elephc\Parallel\TaskGroup $tasks): int => 0);
$tasks = new \Elephc\Parallel\TaskGroup();
"#,
        "Cannot access private constructor: Elephc\\Parallel\\TaskGroup::__construct",
    );
}

/// Allows Cancellation only as a direct capability, not nested argument payload data.
#[test]
fn parallel_rejects_cancellation_nested_in_an_argument_array() {
    expect_parallel_error(
        r#"
namespace Elephc\Async { final class Cancellation {} }
$tasks = new \Elephc\Parallel\TaskGroup();
$token = new \Elephc\Async\Cancellation();
$tasks->spawn(static fn (array $payload): int => 1, [$token]);
"#,
        "argument #1 cannot cross an isolated runtime context: Elephc\\Async\\Cancellation may only be passed as a direct task argument or captured directly; it cannot be nested or returned",
    );
}

/// Rejects Cancellation capabilities nested inside captured transfer arrays.
#[test]
fn parallel_rejects_cancellation_nested_in_a_capture_array() {
    expect_parallel_error(
        r#"
namespace Elephc\Async { final class Cancellation {} }
$tasks = new \Elephc\Parallel\TaskGroup();
$payload = [new \Elephc\Async\Cancellation()];
$tasks->spawn(static function () use ($payload): int { return 1; });
"#,
        "capture $payload cannot cross an isolated runtime context: Elephc\\Async\\Cancellation may only be passed as a direct task argument or captured directly; it cannot be nested or returned",
    );
}

/// Rejects Cancellation capabilities nested in a worker's result array.
#[test]
fn parallel_rejects_cancellation_task_results() {
    expect_parallel_error(
        r#"
namespace Elephc\Async { final class Cancellation {} }
$tasks = new \Elephc\Parallel\TaskGroup();
$token = new \Elephc\Async\Cancellation();
$tasks->spawn(static function () use ($token): array { return [$token]; });
"#,
        "task return value cannot cross an isolated runtime context: Elephc\\Async\\Cancellation may only be passed as a direct task argument or captured directly; it cannot be nested or returned",
    );
}

/// Rejects returning a Cancellation capability directly from a worker.
#[test]
fn parallel_rejects_a_direct_cancellation_task_result() {
    expect_parallel_error(
        r#"
namespace Elephc\Async { final class Cancellation {} }
$tasks = new \Elephc\Parallel\TaskGroup();
$token = new \Elephc\Async\Cancellation();
$tasks->spawn(static function () use ($token): \Elephc\Async\Cancellation { return $token; });
"#,
        "task return value cannot cross an isolated runtime context: Elephc\\Async\\Cancellation may only be passed as a direct task argument or captured directly; it cannot be nested or returned",
    );
}

/// Rejects leaking TaskGroup scope authority through an opaque Mixed root result.
#[test]
fn parallel_task_group_cannot_escape_its_root_scope_even_through_mixed() {
    expect_parallel_surface_error(
        r#"
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): mixed {
    return $tasks;
});
"#,
        "Elephc\\Parallel\\run(): TaskGroup cannot escape its owning scope",
    );
}
