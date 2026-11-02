//! Purpose:
//! Emits the runtime helper backing `headers_sent()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform` during runtime emission.
//!
//! Key details:
//! - `_headers_sent` becomes one only on the real-output path after output buffering.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_headers_sent`, returning the process flag in the integer result register.
pub fn emit_headers_sent(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_headers_sent_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: headers_sent ---");
    emitter.label_global("__rt_headers_sent");
    abi::emit_symbol_address(emitter, "x9", "_headers_sent");
    emitter.instruction("ldr x0, [x9]");
    emitter.instruction("ret");
}

/// Emits the x86_64 variant of `__rt_headers_sent`.
fn emit_headers_sent_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: headers_sent ---");
    emitter.label_global("__rt_headers_sent");
    abi::emit_symbol_address(emitter, "r8", "_headers_sent");
    emitter.instruction("mov rax, QWORD PTR [r8]");
    emitter.instruction("ret");
}
