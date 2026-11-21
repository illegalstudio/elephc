//! Purpose:
//! Compile-time diagnostics for the Elephc Async structured scheduler surface.
//!
//! Called from:
//! - `cargo test --test error_tests async_scheduler`.
//!
//! Key details:
//! - The prelude is injected before name resolution so diagnostics exercise its
//!   real public signature rather than failing as an undefined function.

use super::*;

/// Reports the real injected run signature's missing body argument.
#[test]
fn async_run_requires_a_body() {
    expect_error(
        r#"<?php
use function Elephc\Async\run;
run();
"#,
        "expects 1 arguments, got 0",
    );
}

/// Rejects a non-callable Async root body before native lowering.
#[test]
fn async_run_requires_a_callable_body() {
    expect_error(
        r#"<?php
use function Elephc\Async\run;
run(42);
"#,
        "expects Callable, got Int",
    );
}

/// Warns when a blocking Parallel join runs in the cooperative Async root task.
#[test]
fn parallel_join_in_async_root_task_warns_about_blocking_the_scheduler() {
    expect_warning(
        r#"<?php
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $parallel): void {
    $future = $parallel->spawn(static fn (): int => 42);
    \Elephc\Async\run(static function (\Elephc\Async\TaskGroup $tasks) use ($future): void {
        $future->join();
    });
});
"#,
        "Future::join() inside an Elephc Async task blocks the whole cooperative scheduler thread",
    );
}

/// Warns that draining a Parallel root inside Async blocks its scheduler thread.
#[test]
fn parallel_run_in_async_root_task_warns_about_blocking_the_scheduler() {
    expect_warning(
        r#"<?php
\Elephc\Async\run(static function (\Elephc\Async\TaskGroup $tasks): void {
    \Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $parallel): void {
        $parallel->spawn(static fn (): int => 42);
    });
});
"#,
        "Parallel\\run() inside an Elephc Async task blocks the whole cooperative scheduler thread until the Parallel scope drains",
    );
}

/// Emits exactly one blocking-join warning for a spawned Async task body.
#[test]
fn parallel_join_in_spawned_async_task_warns_once() {
    let result = check_source_full(
        r#"<?php
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $parallel): void {
    $future = $parallel->spawn(static fn (): int => 42);
    \Elephc\Async\run(static function (\Elephc\Async\TaskGroup $tasks) use ($future): void {
        $tasks->spawn(static function () use ($future): void {
            $future->join();
        });
    });
});
"#,
    )
    .expect("expected source to type-check");
    let warnings = result
        .warnings
        .iter()
        .filter(|warning| warning.message.contains("blocks the whole cooperative scheduler thread"))
        .count();
    assert_eq!(warnings, 1, "expected one blocking-join warning: {:?}", result.warnings);
}

/// Avoids Async blocking warnings for joins executed solely in a Parallel scope.
#[test]
fn parallel_join_outside_async_task_does_not_warn() {
    expect_no_warning(
        r#"<?php
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $parallel): void {
    $future = $parallel->spawn(static fn (): int => 42);
    $future->join();
});
"#,
        "blocks the whole cooperative scheduler thread",
    );
}

/// Does not treat an unexecuted nested closure as an active Async blocking call.
#[test]
fn parallel_join_in_unexecuted_nested_closure_does_not_warn() {
    expect_no_warning(
        r#"<?php
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $parallel): void {
    $future = $parallel->spawn(static fn (): int => 42);
    \Elephc\Async\run(static function (\Elephc\Async\TaskGroup $tasks) use ($future): void {
        $later = static function () use ($future): void {
            $future->join();
        };
    });
});
"#,
        "blocks the whole cooperative scheduler thread",
    );
}
