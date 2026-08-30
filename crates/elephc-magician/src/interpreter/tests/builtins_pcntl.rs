//! Purpose:
//! Interpreter-only regression tests for Magician's PCNTL signal dispatch.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Process-global signal state is serialized and restored after each case.

use super::super::*;
use super::support::*;
use std::sync::Mutex;

static PCNTL_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Rejects a standalone Magician Fiber start while an eval signal handler is running.
#[test]
fn eval_handler_cannot_switch_fibers_without_aot_runtime_hooks() {
    let _guard = PCNTL_TEST_LOCK.lock().expect("PCNTL test lock poisoned");
    let register = parse_fragment(
        br#"pcntl_signal(SIGUSR1, function(): void {
    $fiber = new Fiber(function(): void {});
    try { $fiber->start(); }
    catch (FiberError $error) { echo $error->getMessage(); }
});"#,
    )
    .expect("parse PCNTL registration");
    let dispatch =
        parse_fragment(b"pcntl_signal_dispatch();").expect("parse PCNTL dispatch");
    let cleanup = parse_fragment(b"pcntl_signal(SIGUSR1, SIG_DFL);")
        .expect("parse PCNTL cleanup");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(&mut context, &register, &mut scope, &mut values)
        .expect("register eval handler");
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    let dispatched = execute_program_with_context(
        &mut context,
        &dispatch,
        &mut scope,
        &mut values,
    );
    let cleaned = execute_program_with_context(
        &mut context,
        &cleanup,
        &mut scope,
        &mut values,
    );

    dispatched.expect("dispatch eval handler");
    cleaned.expect("restore default signal disposition");
    assert_eq!(
        values.output,
        "Cannot switch fibers in current execution context"
    );
}
