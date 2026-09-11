//! Purpose:
//! Delivers framed mbstring parameter diagnostics through the runtime's stderr/suppression path.
//!
//! Called from:
//! - The protected MbInvokeHostV1 diagnostic callback.
//!
//! Key details:
//! - The coordinator provides validated PHP warning/deprecation levels and binary message bytes.
//! - Prefix, complete message, and newline are emitted before the next argument conversion.
//! - PHP error-handler invocation remains a shared runtime diagnostic integration requirement.

use super::*;

/// Emits the diagnostic body inside an already installed protected callback frame.
pub(super) fn body(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #232]");                              // recover the validated PHP diagnostic level
        emitter.instruction("cmp x9, #8192");                                   // deprecations use their own PHP message prefix
        emitter.instruction("b.eq __rt_mbstring_diagnostic_deprecated");        // select the deprecation prefix before writing the payload
        abi::emit_symbol_address(emitter, "x1", "_mbstring_warning_prefix");
        emitter.instruction("mov x2, #9");                                      // include the complete Warning prefix and trailing space
        emitter.instruction("b __rt_mbstring_diagnostic_prefix");               // share binary message delivery across diagnostic levels
        emitter.label("__rt_mbstring_diagnostic_deprecated");
        abi::emit_symbol_address(emitter, "x1", "_mbstring_deprecated_prefix");
        emitter.instruction("mov x2, #12");                                     // include the complete Deprecated prefix and trailing space
        emitter.label("__rt_mbstring_diagnostic_prefix");
        emitter.instruction("bl __rt_diag_warning");                            // emit the prefix only outside an active suppression scope
        emitter.instruction("ldp x1, x2, [sp, #240]");                          // recover the original binary diagnostic and exact length
        emitter.instruction("bl __rt_diag_warning");                            // preserve embedded NUL and newline bytes in the diagnostic payload
        abi::emit_symbol_address(emitter, "x1", "_mbstring_diagnostic_newline");
        emitter.instruction("mov x2, #1");                                      // terminate the complete diagnostic with exactly one newline
        emitter.instruction("bl __rt_diag_warning");                            // finish delivery before any later argument conversion
    } else {
        emitter.instruction("cmp QWORD PTR [rsp + 232], 8192");                 // distinguish deprecations from the validated warning level
        emitter.instruction("je __rt_mbstring_diagnostic_deprecated");          // select PHP's deprecation prefix
        abi::emit_symbol_address(emitter, "rdi", "_mbstring_warning_prefix");
        emitter.instruction("mov esi, 9");                                      // include the Warning prefix and its trailing space
        emitter.instruction("jmp __rt_mbstring_diagnostic_prefix");             // share binary payload delivery across levels
        emitter.label("__rt_mbstring_diagnostic_deprecated");
        abi::emit_symbol_address(emitter, "rdi", "_mbstring_deprecated_prefix");
        emitter.instruction("mov esi, 12");                                     // include the Deprecated prefix and its trailing space
        emitter.label("__rt_mbstring_diagnostic_prefix");
        emitter.instruction("call __rt_diag_warning");                          // emit the prefix through the runtime suppression policy
        emitter.instruction("mov rdi, QWORD PTR [rsp + 240]");                  // recover the complete binary diagnostic message
        emitter.instruction("mov rsi, QWORD PTR [rsp + 248]");                  // preserve its exact byte length including embedded zero bytes
        emitter.instruction("call __rt_diag_warning");                          // deliver the message before later argument callbacks
        abi::emit_symbol_address(emitter, "rdi", "_mbstring_diagnostic_newline");
        emitter.instruction("mov esi, 1");                                      // terminate the complete diagnostic with one newline
        emitter.instruction("call __rt_diag_warning");                          // complete delivery inside the protected callback boundary
    }
}
