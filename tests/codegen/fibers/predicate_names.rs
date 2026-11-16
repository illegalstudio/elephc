//! Purpose:
//! Regression tests for user methods that share a name with a Fiber state predicate
//! (`isStarted`, `isRunning`, `isSuspended`, `isTerminated`) and are called on an untyped
//! receiver.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `mixed` receiver dispatches on its runtime class: only a real Fiber reaches the
//!   runtime state predicate, any other class runs its own method. Before the fix every such
//!   call was lowered as the Fiber predicate, so a user object reported
//!   `Call to a member function isRunning() on null`, and a program without Fiber failed to
//!   compile.

use super::*;

/// Verifies that a `mixed` receiver runs a user class's own `isRunning()` and the Fiber
/// predicate on a real Fiber, both through the same call site.
#[test]
fn test_mixed_receiver_dispatches_user_is_running_beside_fiber() {
    let out = compile_and_run(
        r#"<?php
interface Game { public function isRunning(): bool; }
final class Demo implements Game {
    public function isRunning(): bool { return true; }
}
function state(mixed $value): bool {
    return $value->isRunning();
}
$fiber = new Fiber(function (): void {});
echo state(new Demo()) ? "demo running\n" : "demo stopped\n";
echo state($fiber) ? "fiber running\n" : "fiber idle\n";
"#,
    );
    assert_eq!(out, "demo running\nfiber idle\n");
}

/// Verifies that a `mixed` receiver calling a user `isRunning()` compiles and runs in a
/// program that never uses Fiber.
#[test]
fn test_mixed_receiver_user_is_running_without_fiber() {
    let out = compile_and_run(
        r#"<?php
final class Machine {
    public function isRunning(): bool { return true; }
}
function state(mixed $value): bool {
    return $value->isRunning();
}
echo state(new Machine()) ? "running\n" : "stopped\n";
"#,
    );
    assert_eq!(out, "running\n");
}
