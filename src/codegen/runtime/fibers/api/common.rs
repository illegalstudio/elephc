//! Purpose:
//! Emits the `__rt_mixed_from_value` runtime helper assembly for common.
//! Keeps emitted runtime labels and generated code call sites aligned across supported targets.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Runtime labels, registers, and data symbols here are ABI shared with generated assembly call sites.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_mixed_from_value` call sequence to allocate a boxed `Mixed` null cell.
///
/// For both targets the sequence sets x0/rax to tag 8 (PHP null) and clears the low/high
/// payload words before branching to the runtime allocator. The result is returned in the
/// ABI's normal scalar return location (x0 for ARM64, rax for x86_64).
pub(super) fn emit_box_null_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = PHP null
            emitter.instruction("mov x1, #0");                                  // null has no low payload word
            emitter.instruction("mov x2, #0");                                  // null has no high payload word
            emitter.instruction("bl __rt_mixed_from_value");                    // allocate a boxed Mixed null cell for the PHP-visible result
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = PHP null
            emitter.instruction("xor edi, edi");                                // null has no low payload word
            emitter.instruction("xor esi, esi");                                // null has no high payload word
            emitter.instruction("call __rt_mixed_from_value");                  // allocate a boxed Mixed null cell for the PHP-visible result
        }
    }
}
