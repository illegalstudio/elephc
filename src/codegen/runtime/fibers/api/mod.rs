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

pub fn emit_fiber_throw_state_error(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_throw_state_error_x86_64(emitter);
        return;
    }

    arm64::emit_throw_state_error(emitter);
}

pub fn emit_fiber_construct(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_construct_x86_64(emitter);
        return;
    }

    arm64::emit_construct(emitter);
}

pub fn emit_fiber_start(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_start_x86_64(emitter);
        return;
    }

    arm64::emit_start(emitter);
}

pub fn emit_fiber_resume(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_resume_x86_64(emitter);
        return;
    }

    arm64::emit_resume(emitter);
}

pub fn emit_fiber_suspend(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_suspend_x86_64(emitter);
        return;
    }

    arm64::emit_suspend(emitter);
}

pub fn emit_fiber_throw(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_throw_x86_64(emitter);
        return;
    }

    arm64::emit_throw(emitter);
}

pub fn emit_fiber_get_current(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_get_current_x86_64(emitter);
        return;
    }

    arm64::emit_get_current(emitter);
}

pub fn emit_fiber_get_return(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_get_return_x86_64(emitter);
        return;
    }

    arm64::emit_get_return(emitter);
}

pub fn emit_fiber_state_getter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_state_getter_x86_64(emitter);
        return;
    }

    arm64::emit_state_getter(emitter);
}
