//! Purpose:
//! Emits PHP loose equality for two runtime strings.
//! Numeric strings compare by numeric value; non-numeric strings compare byte-for-byte.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - PHP's `zendi_smart_streq` is `zendi_smart_strcmp(..) == 0`, so this helper is a thin
//!   wrapper over `__rt_str_smart_cmp` rather than a second copy of the rules. That matters
//!   because the rules are not "parse both sides as doubles": two integer strings compare as
//!   `zend_long`, and integer text beyond `zend_long` falls back to a byte comparison.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_str_loose_eq` runtime routine.
/// Compares two PHP strings using loose equality (== semantics).
///
/// Input registers:
///   - ARM64: x1/x2 = left (ptr, len), x3/x4 = right (ptr, len)
///   - x86_64: rdi/rsi = left (ptr, len), rdx/rcx = right (ptr, len)
///
/// The operand registers are already `__rt_str_smart_cmp`'s, so the helper forwards them
/// untouched and only turns the three-way result into a boolean.
///
/// Output:
///   - ARM64: x0 = 1 if loosely equal, 0 otherwise
///   - x86_64: rax = 1 if loosely equal, 0 otherwise
///
/// Calls: `__rt_str_smart_cmp`.
pub fn emit_str_loose_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_loose_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_loose_eq ---");
    emitter.label_global("__rt_str_loose_eq");

    emitter.instruction("sub sp, sp, #32");                                     // allocate an aligned frame for the shared comparison call
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("bl __rt_str_smart_cmp");                               // apply PHP's full string-versus-string ordering
    emitter.instruction("cmp x0, #0");                                          // `zendi_smart_streq` is that ordering being equal
    emitter.instruction("cset x0, eq");                                         // produce the boolean PHP's `==` expects
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the loose string equality result in x0
}

/// x86_64 Linux implementation of the `__rt_str_loose_eq` runtime routine.
/// Identical logic to the ARM64 path but uses the System V AMD64 ABI:
///   - rdi/rsi = left (ptr, len), rdx/rcx = right (ptr, len)
///   - rax = result (1 if loosely equal, 0 otherwise)
///
/// The operands already sit in `__rt_str_smart_cmp`'s argument registers, so the frame
/// exists only to keep the stack aligned across that call.
fn emit_str_loose_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_loose_eq ---");
    emitter.label_global("__rt_str_loose_eq");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // keep the stack aligned across the shared comparison call
    abi::emit_call_label(emitter, "__rt_str_smart_cmp");                        // apply PHP's full string-versus-string ordering
    emitter.instruction("cmp rax, 0");                                          // `zendi_smart_streq` is that ordering being equal
    emitter.instruction("sete al");                                             // produce the boolean PHP's `==` expects
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the full result register
    emitter.instruction("add rsp, 16");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the loose string equality result in rax
}
