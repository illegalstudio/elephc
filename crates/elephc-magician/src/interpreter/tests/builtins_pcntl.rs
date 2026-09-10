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

/// Rejects a Fiber switch reached through an object-method `call_user_func` callback.
#[test]
fn eval_handler_cannot_switch_fibers_through_call_user_func() {
    let _guard = PCNTL_TEST_LOCK.lock().expect("PCNTL test lock poisoned");
    let register = parse_fragment(
        br#"pcntl_signal(SIGUSR1, function(): void {
    $fiber = new Fiber(function(): void {});
    try { call_user_func([$fiber, "start"]); }
    catch (FiberError $error) { echo $error->getMessage(); }
});"#,
    )
    .expect("parse indirect Fiber registration");
    let dispatch =
        parse_fragment(b"pcntl_signal_dispatch();").expect("parse PCNTL dispatch");
    let cleanup = parse_fragment(b"pcntl_signal(SIGUSR1, SIG_DFL);")
        .expect("parse PCNTL cleanup");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(&mut context, &register, &mut scope, &mut values)
        .expect("register indirect Fiber handler");
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    execute_program_with_context(&mut context, &dispatch, &mut scope, &mut values)
        .expect("dispatch indirect Fiber handler");
    execute_program_with_context(&mut context, &cleanup, &mut scope, &mut values)
        .expect("restore default signal disposition");

    assert_eq!(
        values.output,
        "Cannot switch fibers in current execution context"
    );
}

/// Rejects a Fiber switch reached through a static-method `call_user_func` callback.
///
/// Registers `Fiber::suspend` as a native AOT static so dispatch takes the
/// `EvaluatedCallable::StaticMethod` native path instead of
/// `eval_static_method_call_result_resolved`, which already rejected.
#[test]
fn eval_handler_cannot_switch_fibers_through_call_user_func_static() {
    let _guard = PCNTL_TEST_LOCK.lock().expect("PCNTL test lock poisoned");
    let register = parse_fragment(
        br#"pcntl_signal(SIGUSR1, function(): void {
    try { call_user_func(["Fiber", "suspend"]); }
    catch (FiberError $error) { echo $error->getMessage(); }
});"#,
    )
    .expect("parse static Fiber registration");
    let dispatch =
        parse_fragment(b"pcntl_signal_dispatch();").expect("parse PCNTL dispatch");
    let cleanup = parse_fragment(b"pcntl_signal(SIGUSR1, SIG_DFL);")
        .expect("parse PCNTL cleanup");
    let mut context = ElephcEvalContext::new();
    assert!(context.define_native_static_method_signature(
        "Fiber",
        "suspend",
        NativeCallableSignature::new(0),
    ));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(&mut context, &register, &mut scope, &mut values)
        .expect("register static Fiber handler");
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    execute_program_with_context(&mut context, &dispatch, &mut scope, &mut values)
        .expect("dispatch static Fiber handler");
    execute_program_with_context(&mut context, &cleanup, &mut scope, &mut values)
        .expect("restore default signal disposition");

    assert_eq!(
        values.output,
        "Cannot switch fibers in current execution context"
    );
}

/// Rejects switching method names on an eval-declared class whose receiver is named Fiber.
#[test]
fn eval_handler_cannot_switch_eval_declared_fiber_class() {
    let _guard = PCNTL_TEST_LOCK.lock().expect("PCNTL test lock poisoned");
    let register = parse_fragment(
        br#"class Fiber {
    public function start(): void { echo "method-ran"; }
}
pcntl_signal(SIGUSR1, function(): void {
    $fiber = new Fiber();
    try { $fiber->start(); }
    catch (FiberError $error) { echo $error->getMessage(); }
});"#,
    )
    .expect("parse eval-declared Fiber registration");
    let dispatch =
        parse_fragment(b"pcntl_signal_dispatch();").expect("parse PCNTL dispatch");
    let cleanup = parse_fragment(b"pcntl_signal(SIGUSR1, SIG_DFL);")
        .expect("parse PCNTL cleanup");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(&mut context, &register, &mut scope, &mut values)
        .expect("register eval-declared Fiber handler");
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    execute_program_with_context(&mut context, &dispatch, &mut scope, &mut values)
        .expect("dispatch eval-declared Fiber handler");
    execute_program_with_context(&mut context, &cleanup, &mut scope, &mut values)
        .expect("restore default signal disposition");

    assert_eq!(
        values.output,
        "Cannot switch fibers in current execution context"
    );
}

/// Pins each copied foreign handler identity once and releases every pin during teardown.
#[test]
fn foreign_handler_copy_metadata_is_idempotent_and_drains_cell_pins() {
    let _guard = PCNTL_TEST_LOCK.lock().expect("PCNTL test lock poisoned");
    let register = parse_fragment(
        b"function pinned_handler(): void {} pcntl_signal(SIGUSR1, 'pinned_handler');",
    )
    .expect("parse named PCNTL registration");
    let get = parse_fragment(b"return pcntl_signal_get_handler(SIGUSR1);")
        .expect("parse foreign handler lookup");
    let cleanup = parse_fragment(b"pcntl_signal(SIGUSR1, SIG_DFL);")
        .expect("parse PCNTL cleanup");
    let mut owner_context = ElephcEvalContext::new();
    let mut owner_scope = ElephcEvalScope::new();
    let mut foreign_context = ElephcEvalContext::new();
    let mut foreign_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let registered = execute_program_with_context(
        &mut owner_context,
        &register,
        &mut owner_scope,
        &mut values,
    )
    .expect("register named eval handler");
    values.release(registered).expect("release registration result");
    let handler = execute_program_with_context(
        &mut foreign_context,
        &get,
        &mut foreign_scope,
        &mut values,
    )
    .expect("get foreign handler");
    assert!(foreign_context.pcntl_foreign_callable_owner(handler).is_some());

    let copied = values.copy_value(handler).expect("copy foreign handler value");
    let retains_before = values.retains.len();
    foreign_context
        .copy_pcntl_foreign_callable(handler, copied, &mut values)
        .expect("copy foreign handler metadata");
    assert_eq!(values.retains.len(), retains_before + 1);
    foreign_context
        .copy_pcntl_foreign_callable(handler, copied, &mut values)
        .expect("repeat foreign handler metadata copy");
    assert_eq!(values.retains.len(), retains_before + 1);
    assert!(foreign_context.pcntl_foreign_callable_owner(copied).is_some());

    let releases_before = values.releases.len();
    foreign_context.release_pcntl_foreign_callables(&mut values);
    assert_eq!(values.releases.len(), releases_before + 2);
    assert!(foreign_context.pcntl_foreign_callable_owner(handler).is_none());
    assert!(foreign_context.pcntl_foreign_callable_owner(copied).is_none());
    values.release(handler).expect("release handler result");
    values.release(copied).expect("release copied handler");

    let cleaned = execute_program_with_context(
        &mut owner_context,
        &cleanup,
        &mut owner_scope,
        &mut values,
    )
    .expect("restore default signal disposition");
    values.release(cleaned).expect("release cleanup result");
}
