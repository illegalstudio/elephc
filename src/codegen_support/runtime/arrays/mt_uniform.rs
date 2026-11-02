//! Purpose:
//! Emits `__rt_mt_uniform`, the trampoline `array_rand`/`shuffle` use to draw from the
//! global Mt19937 state without naming the optional random bridge directly.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed`.
//!
//! Key details:
//! - The exclusive bound already sits in the first integer argument register, so the helper
//!   only has to load the late-bound entry and tail-call it.
//! - The slot is published by `crate::codegen_support::random` at every `array_rand`/`shuffle`
//!   call site; an unpublishing program cannot reach this helper.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the shared `__rt_mt_uniform` trampoline.
pub fn emit_mt_uniform(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mt_uniform ---");
    emitter.label_global("__rt_mt_uniform");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(
                emitter,
                "x9",
                "_elephc_random_mt_uniform_fn",
                0,
            );
            emitter.instruction("br x9");                                       // tail-call the published global Mt19937 range entry
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(
                emitter,
                "r11",
                "_elephc_random_mt_uniform_fn",
                0,
            );
            emitter.instruction("jmp r11");                                     // tail-call the published global Mt19937 range entry
        }
    }
}
