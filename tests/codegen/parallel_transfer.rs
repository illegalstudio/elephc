//! Purpose:
//! Positive checker coverage for values allowed across the future Parallel boundary.
//!
//! Called from:
//! - `cargo test --test codegen_tests parallel_transfer`.
//!
//! Key details:
//! - A local synchronous `TaskGroup` fixture exercises only the compile-time transfer contract.
//! - It does not claim that the Parallel runtime/executor is implemented by these tests.

use crate::support::*;

/// Checks scalar/nested-array capture transferability with a synchronous checker fixture.
#[test]
fn parallel_transfer_accepts_scalars_nested_arrays_and_static_captures() {
    let out = compile_and_run(
        r#"<?php
namespace Elephc\Parallel {
    final class TaskGroup {
        public function spawn(\Closure $task, mixed ...$args): mixed {
            return $task(...$args);
        }
    }
}
namespace {
    $tasks = new \Elephc\Parallel\TaskGroup();
    $prefix = "ok";
    $task = static function (array $values) use ($prefix): string {
        return $prefix . "|" . $values[0] . "|" . $values[1][1];
    };
    $result = $tasks->spawn($task, [1, [true, "x"]]);
    echo $result;
}
"#,
    );
    assert_eq!(out, "ok|1|x");
}

/// Accepts the specific Cancellation by-reference exception in the transfer checker fixture.
#[test]
fn parallel_transfer_accepts_the_named_cancellation_reference_exception() {
    let out = compile_and_run(
        r#"<?php
namespace Elephc\Async { final class Cancellation {} }
namespace Elephc\Parallel {
    final class TaskGroup {
        public function spawn(\Closure $task, mixed ...$args): mixed {
            return $task(...$args);
        }
    }
}
namespace {
    $tasks = new \Elephc\Parallel\TaskGroup();
    $cancellation = new \Elephc\Async\Cancellation();
    $result = $tasks->spawn(static function () use (&$cancellation): int {
        return $cancellation instanceof \Elephc\Async\Cancellation ? 1 : 0;
    });
    echo $result;
}
"#,
    );
    assert_eq!(out, "1");
}

/// Preserves a proven transferable array result through closure assignment and aliasing.
#[test]
fn parallel_transfer_keeps_array_return_proof_on_an_assigned_closure() {
    let out = compile_and_run(
        r#"<?php
namespace Elephc\Parallel {
    final class TaskGroup {
        public function spawn(\Closure $task, mixed ...$args): mixed {
            return $task(...$args);
        }
    }
}
namespace {
    $tasks = new \Elephc\Parallel\TaskGroup();
    $prefix = "ok";
    $task = static fn (): array => [$prefix, [1, true, "x"]];
    $copy = $task;
    $result = $tasks->spawn($copy);
    echo $result[0], "|", $result[1][0], "|", $result[1][2];
}
"#,
    );
    assert_eq!(out, "ok|1|x");
}

/// Accepts a reference-aliased scalar as a by-value task argument in the checker fixture.
#[test]
fn parallel_transfer_copies_a_reference_aliased_scalar_argument_by_value() {
    let out = compile_and_run(
        r#"<?php
namespace Elephc\Parallel {
    final class TaskGroup {
        public function spawn(\Closure $task, mixed ...$args): mixed {
            return $task(...$args);
        }
    }
}
namespace {
    $tasks = new \Elephc\Parallel\TaskGroup();
    $value = 7;
    $alias =& $value;
    echo $tasks->spawn(static fn (int $copy): int => $copy + 1, $alias);
}
"#,
    );
    assert_eq!(out, "8");
}
