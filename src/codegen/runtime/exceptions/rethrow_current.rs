//! Purpose:
//! Emits the `__rt_rethrow_current`, `__rt_throw_current` runtime helper assembly for rethrow current.
//! Keeps exception object matching, unwinding state, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::exceptions`.
//!
//! Key details:
//! - Exception matching and unwinding must keep handler-stack, call-frame cleanup, and class metadata invariants aligned.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_rethrow_current` runtime helper.
///
/// Transfers control to `__rt_throw_current` using a tail-jump, reusing the
/// current active exception state without re-throwing. Used when the compiler
/// needs to re-raise an exception that is already active on the unwinding stack.
pub fn emit_rethrow_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rethrow_current ---");
    emitter.label_global("__rt_rethrow_current");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("b __rt_throw_current");                        // re-use the ordinary throw helper with the existing active exception state
        }
        Arch::X86_64 => {
            emitter.instruction("jmp __rt_throw_current");                      // re-use the ordinary throw helper with the existing active exception state
        }
    }
}
