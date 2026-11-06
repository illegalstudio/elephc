//! Purpose:
//! Shares JSON token candidate emission and error-only decode diagnostic validation.
//!
//! Called from:
//! - The JSON validator emitters and structural decoder syntax failure paths.
//!
//! Key details:
//! - Valid decode input never enters the diagnostic validator.
//! - Structural slicing failures revalidate the live original source to identify the failing token.
//! - Revalidation runs only while unwinding an error, so parser cursors and depth are disposable.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Records the current validator token or EOF cursor without scanning the source.
pub(super) fn note_cursor(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);
        abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);
        emitter.instruction("add rax, rcx");                                    // address the failing token or the exclusive source end
        emitter.instruction("call __rt_json_note_error_candidate");             // publish the token before recording the error
    } else {
        abi::emit_load_symbol_to_reg(emitter, "x0", "_json_validate_ptr", 0);
        abi::emit_load_symbol_to_reg(emitter, "x12", "_json_validate_idx", 0);
        emitter.instruction("add x0, x0, x12");                                 // address the failing token or the exclusive source end
        emitter.instruction("bl __rt_json_note_error_candidate");               // publish the token before recording the error
    }
}

/// Emits the failure-only entry shared by structural decoder syntax errors on every target.
pub(super) fn emit_decode_syntax_error(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_json_decode_syntax_error");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // preserve the decoder's frame and align calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable diagnostic frame
        abi::emit_load_symbol_to_reg(emitter, "rax", "_json_error_source_ptr", 0);
        emitter.instruction("test rax, rax");                                   // require a live original source before revalidation
        emitter.instruction("je __rt_json_decode_syntax_fallback_x");           // retain the caller's candidate without an original source
        abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_error_source_len", 0);
        emitter.instruction("call __rt_json_validate");                         // identify the first failing token only after decode has failed
        emitter.instruction("test rax, rax");                                   // did validation already record or throw the failure?
        emitter.instruction("je __rt_json_decode_syntax_done_x");               // preserve the validator's error and numeric coordinates
        emitter.label("__rt_json_decode_syntax_fallback_x");
        emitter.instruction("mov rax, 4");                                      // retain a decoder syntax failure even if validation accepts the source
        emitter.instruction("call __rt_json_throw_error");                      // materialize the saved candidate and optionally throw
        emitter.label("__rt_json_decode_syntax_done_x");
        emitter.instruction("xor eax, eax");                                    // return the structural decoder's null signal
        emitter.instruction("pop rbp");                                         // restore the decoder frame
        emitter.instruction("ret");                                             // unwind the failed decoder invocation
    } else {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve the decoder frame and return address
        emitter.instruction("mov x29, sp");                                     // establish a stable diagnostic frame
        abi::emit_load_symbol_to_reg(emitter, "x1", "_json_error_source_ptr", 0);
        emitter.instruction("cbz x1, __rt_json_decode_syntax_fallback");        // retain the caller's candidate without an original source
        abi::emit_load_symbol_to_reg(emitter, "x2", "_json_error_source_len", 0);
        emitter.instruction("bl __rt_json_validate");                           // identify the first failing token only after decode has failed
        emitter.instruction("cbz x0, __rt_json_decode_syntax_done");            // preserve the validator's error and numeric coordinates
        emitter.label("__rt_json_decode_syntax_fallback");
        emitter.instruction("mov x0, #4");                                      // retain a decoder syntax failure even if validation accepts the source
        emitter.instruction("bl __rt_json_throw_error");                        // materialize the saved candidate and optionally throw
        emitter.label("__rt_json_decode_syntax_done");
        emitter.instruction("mov x0, #0");                                      // return the structural decoder's null signal
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore the decoder frame and return address
        emitter.instruction("ret");                                             // unwind the failed decoder invocation
    }
}
