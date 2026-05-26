//! Purpose:
//! Wires runtime helpers for PHP Fiber public API methods.
//! Owns architecture selection for start, resume, suspend, throw, and state-query helpers.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::fibers`.
//! - `crate::codegen::runtime::x86_minimal::emit_runtime_linux_x86_64_minimal()`.
//!
//! Key details:
//! - API helpers must preserve Fiber state transitions and delegate target-specific assembly to focused emitters.

mod arm64;
mod common;
mod x86_64;

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Allocates a FiberError object with the given message and raises it via
/// `__rt_throw_current`. Never returns to its caller.
///
/// Input: rdi/x0 = message bytes pointer, rsi/x1 = message length.
/// Dispatches to `x86_64::emit_throw_state_error_x86_64` or `arm64::emit_throw_state_error`.
pub fn emit_fiber_throw_state_error(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_throw_state_error_x86_64(emitter);
        return;
    }

    arm64::emit_throw_state_error(emitter);
}

/// Allocates a new Fiber object: zeroes all runtime-managed fields, allocates
/// the per-fiber stack via mmap, carves out a zeroed fake initial frame, and
/// sets state to `NotStarted`. Returns the new Fiber pointer in rax/x0.
///
/// Input: rdi/x0 = callable (closure pointer), rsi/x1 = Fiber class_id, rdx/x2 = entry wrapper pointer.
/// Dispatches to `x86_64::emit_construct_x86_64` or `arm64::emit_construct`.
pub fn emit_fiber_construct(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_construct_x86_64(emitter);
        return;
    }

    arm64::emit_construct(emitter);
}

/// Performs the first cooperative switch into the fiber, transitioning it from
/// `NotStarted` to `Running`. Validates that the fiber has not already been
/// started; if so, raises FiberError via `__rt_fiber_throw_state_error`.
///
/// Input: rdi/x0 = fiber*.
/// Output: rax/x0 = value yielded by the fiber (or PHP null if it terminated).
/// Dispatches to `x86_64::emit_start_x86_64` or `arm64::emit_start`.
pub fn emit_fiber_start(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_start_x86_64(emitter);
        return;
    }

    arm64::emit_start(emitter);
}

/// Delivers a Mixed value into a suspended fiber and resumes execution.
/// Validates that the fiber is in the `Suspended` state; otherwise raises
/// FiberError via `__rt_fiber_throw_state_error`. The delivered value becomes
/// the return value of the fiber's `Fiber::suspend()` call.
///
/// Input: rdi/x0 = fiber*, rsi/x1 = boxed Mixed value to deliver.
/// Output: rax/x0 = value the fiber yielded next (or PHP null if it terminated).
/// Dispatches to `x86_64::emit_resume_x86_64` or `arm64::emit_resume`.
pub fn emit_fiber_resume(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_resume_x86_64(emitter);
        return;
    }

    arm64::emit_resume(emitter);
}

/// Yields control from the running fiber back to its caller (the resumer of
/// `start()` or `resume()`). Validates that this is called from within a
/// Fiber context; otherwise raises FiberError via `__rt_fiber_throw_state_error`.
/// Checks for a pending Throwable scheduled by `Fiber::throw()` and re-raises
/// it before returning the resumer's delivered value.
///
/// Input: rdi/x0 = boxed Mixed value to deliver to the resumer.
/// Output: rax/x0 = value passed by the next `resume($v)` call.
/// Dispatches to `x86_64::emit_suspend_x86_64` or `arm64::emit_suspend`.
pub fn emit_fiber_suspend(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_suspend_x86_64(emitter);
        return;
    }

    arm64::emit_suspend(emitter);
}

/// Schedules a Throwable to be raised inside the target fiber when it is next
/// resumed. The exception is parked in the fiber's `pending_throw` slot; the
/// `suspend` helper clears and re-raises it via `__rt_throw_current`. Validates
/// that the fiber is `Suspended`; otherwise raises FiberError.
///
/// Input: rdi/x0 = fiber*, rsi/x1 = Throwable* to deliver.
/// Output: rax/x0 = value the fiber yielded (or PHP null if it terminated).
/// Dispatches to `x86_64::emit_throw_x86_64` or `arm64::emit_throw`.
pub fn emit_fiber_throw(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_throw_x86_64(emitter);
        return;
    }

    arm64::emit_throw(emitter);
}

/// Returns the currently executing Fiber instance. Called from the main thread,
/// returns a boxed PHP null (tag 8). Called from within a Fiber, returns a boxed
/// object wrapping the current Fiber (tag 6, low word = object pointer).
///
/// Output: rax/x0 = boxed PHP null when outside any fiber; otherwise boxed Fiber object.
/// Dispatches to `x86_64::emit_get_current_x86_64` or `arm64::emit_get_current`.
pub fn emit_fiber_get_current(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_get_current_x86_64(emitter);
        return;
    }

    arm64::emit_get_current(emitter);
}

/// Returns the value a terminated fiber produced. Validates that the fiber
/// has reached the `Terminated` state; otherwise raises FiberError. The
/// returned value is `incref`'d so the caller owns it independently of the Fiber.
///
/// Input: rdi/x0 = fiber* (may be NULL for a safe null default).
/// Output: rax/x0 = fiber's `transfer_value.lo` (caller-owned, refcounted Mixed).
/// Dispatches to `x86_64::emit_get_return_x86_64` or `arm64::emit_get_return`.
pub fn emit_fiber_get_return(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_get_return_x86_64(emitter);
        return;
    }

    arm64::emit_get_return(emitter);
}

/// Compares the fiber's current state against an expected value and returns
/// a boolean (1 if equal, 0 otherwise). NULL fiber pointers always return 0.
///
/// Input: rdi/x0 = fiber*, rsi/x1 = expected state value.
/// Output: rax/x0 = 1 if fiber state matches, 0 otherwise.
/// Dispatches to `x86_64::emit_state_getter_x86_64` or `arm64::emit_state_getter`.
pub fn emit_fiber_state_getter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_state_getter_x86_64(emitter);
        return;
    }

    arm64::emit_state_getter(emitter);
}
