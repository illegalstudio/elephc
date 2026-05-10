//! Purpose:
//! Emits direct and indirect call instructions for the active target architecture.
//! Keeps call-site syntax separate from argument materialization and result handling.
//!
//! Called from:
//! - `crate::codegen::expr::calls`, wrappers, and runtime-facing helper emitters
//!
//! Key details:
//! - The caller is responsible for ABI setup; these helpers only transfer control to a label or register.

use crate::codegen::{emit::Emitter, platform::Arch};

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
