//! Purpose:
//! Emits direct and indirect call instructions for the active target architecture.
//! Keeps call-site syntax separate from argument materialization and result handling.
//!
//! Called from:
//! - `crate::codegen`, wrappers, and runtime-facing helper emitters.
//!
//! Key details:
//! - The caller is responsible for ABI setup; these helpers only transfer control to a label or register.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits a direct call to a compile-time-known label.
/// The caller has already set up the ABI (stack frame, arguments, etc.).
/// This only transfers control to the callee.
pub fn emit_call_label(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("bl {}", label));                      // branch-and-link to the named direct-call target
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("call {}", label));                    // call the named direct-call target through the native x86_64 instruction
        }
    }
}

/// Emits an indirect call through a register holding the callee address.
/// The callee is not known at compile time; the caller has already loaded
/// the target address into `reg` and set up the ABI. This only transfers control.
pub fn emit_call_reg(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("blr {}", reg));                       // branch to the indirect-call target held in the requested register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("call {}", reg));                      // call the indirect target held in the requested register
        }
    }
}
