//! Purpose:
//! Emits `__rt_filter_param_warning`, the pair of warnings php raises when a built-in filter is
//! handed a `$params` it cannot read.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`, on the attach path when the node creation refused.
//!
//! Key details:
//! - php's four `convert.*` filters parse `$params` as an ARRAY and reject anything else. Measured
//!   on `php -n` 8.5.6, `stream_filter_append($h, "convert.base64-encode", STREAM_FILTER_WRITE,
//!   null)` raises TWO warnings and answers `false`, while `string.toupper`, `dechunk`, `zlib.*`
//!   and `bzip2.*` accept a null, an int or a string without complaint — they never read it.
//! - OMITTING the argument is not the same as passing null: php tests the zval POINTER, which is
//!   NULL only when the argument was not supplied. The attach lowering preserves that distinction
//!   by retaining nothing when the call has three operands.
//! - The message is composed from chunks through `__rt_diag_warning` rather than assembled in a
//!   buffer, because the second half is the existing "unable to locate" wording with a different
//!   verb ("create or locate") and a different tail, and the composer that builds the existing one
//!   hardcodes its own `"` and newline.

use crate::codegen_support::runtime::data::{
    FILTER_PARAM_CREATE_APPEND_HEAD, FILTER_PARAM_CREATE_PREPEND_HEAD, FILTER_PARAM_CREATE_TAIL,
    FILTER_PARAM_INVALID_APPEND_HEAD, FILTER_PARAM_INVALID_PREPEND_HEAD, FILTER_PARAM_INVALID_TAIL,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_filter_param_warning(name_ptr, name_len, prepend)`.
///
/// `prepend` selects which function names itself in both headings; the filter name is repeated in
/// each, so it is passed rather than baked in.
pub fn emit_filter_param_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 composer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the invalid-filter-parameter warnings ---");
    emitter.label_global("__rt_filter_param_warning");
    // Frame: [0]=name ptr [8]=name len [16]=prepend.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the composer frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the filter name, named twice below
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("str x2, [sp, #16]");                                   // which function is reporting

    // -- "Warning: stream_filter_append(): Stream filter (" --
    emitter.instruction("cbnz x2, __rt_fpw_invalid_prepend");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_invalid_append");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_INVALID_APPEND_HEAD.len()));
    emitter.instruction("b __rt_fpw_invalid_head");
    emitter.label("__rt_fpw_invalid_prepend");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_invalid_prepend");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_INVALID_PREPEND_HEAD.len()));
    emitter.label("__rt_fpw_invalid_head");
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    emitter.instruction("ldr x1, [sp, #0]");                                    // the filter name
    emitter.instruction("ldr x2, [sp, #8]");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_invalid_tail");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_INVALID_TAIL.len()));
    emitter.instruction("bl __rt_diag_warning");

    // -- "Warning: stream_filter_append(): Unable to create or locate filter \"" --
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("cbnz x9, __rt_fpw_create_prepend");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_create_append");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_CREATE_APPEND_HEAD.len()));
    emitter.instruction("b __rt_fpw_create_head");
    emitter.label("__rt_fpw_create_prepend");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_create_prepend");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_CREATE_PREPEND_HEAD.len()));
    emitter.label("__rt_fpw_create_head");
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("ldr x1, [sp, #0]");                                    // the filter name again
    emitter.instruction("ldr x2, [sp, #8]");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_create_tail");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_CREATE_TAIL.len()));
    emitter.instruction("bl __rt_diag_warning");

    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the composer frame
    emitter.instruction("ret");
}

/// Emits the x86_64 composer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the invalid-filter-parameter warnings ---");
    emitter.label_global("__rt_filter_param_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the composer frame
    emitter.instruction("sub rsp, 32");                                         // reserve the name and selector slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the filter name, named twice below
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // which function is reporting

    emitter.instruction("test rdx, rdx");
    emitter.instruction("jnz __rt_fpw_invalid_prepend_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fp_invalid_append");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_INVALID_APPEND_HEAD.len()));
    emitter.instruction("jmp __rt_fpw_invalid_head_x86");
    emitter.label("__rt_fpw_invalid_prepend_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fp_invalid_prepend");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_INVALID_PREPEND_HEAD.len()));
    emitter.label("__rt_fpw_invalid_head_x86");
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the filter name
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fp_invalid_tail");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_INVALID_TAIL.len()));
    emitter.instruction("call __rt_diag_warning");

    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jnz __rt_fpw_create_prepend_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fp_create_append");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_CREATE_APPEND_HEAD.len()));
    emitter.instruction("jmp __rt_fpw_create_head_x86");
    emitter.label("__rt_fpw_create_prepend_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fp_create_prepend");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_CREATE_PREPEND_HEAD.len()));
    emitter.label("__rt_fpw_create_head_x86");
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the filter name again
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fp_create_tail");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_CREATE_TAIL.len()));
    emitter.instruction("call __rt_diag_warning");

    emitter.instruction("add rsp, 32");                                         // release the composer frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
