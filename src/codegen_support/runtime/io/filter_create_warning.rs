//! Purpose:
//! Emits `__rt_filter_create_warning`, php's `Unable to create or locate filter "<name>"`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::stream_filters`, when a `convert.iconv.*` name
//!   names a factory that exists but a conversion that cannot be opened.
//! - `__rt_filter_param_warning`, whose second half is exactly this line.
//!
//! Key details:
//! - php has TWO wordings and picks between them by WHY the attach failed. A name no factory
//!   claims gets `Unable to locate filter "nosuchfilter"`. A name a factory DOES claim but then
//!   refuses gets `Unable to create or locate filter "convert.iconv.nope/alsonope"` — measured on
//!   `php -n` 8.5.6. Every `convert.iconv.` name reaches the second, since the prefix is what
//!   selects the factory.
//! - Composed from chunks through `__rt_diag_warning` rather than assembled into a buffer,
//!   because the name comes from a run-time string whose length is not known here.

use crate::codegen_support::runtime::data::{
    FILTER_PARAM_CREATE_APPEND_HEAD, FILTER_PARAM_CREATE_PREPEND_HEAD, FILTER_PARAM_CREATE_TAIL,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_filter_create_warning(name_ptr, name_len, prepend)`.
///
/// `prepend` is non-zero for `stream_filter_prepend()`, which names itself in the heading.
pub fn emit_filter_create_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 composer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unable-to-create-or-locate filter warning ---");
    emitter.label_global("__rt_filter_create_warning");
    // Frame: [0]=name ptr [8]=name len [16]=prepend.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the composer frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the composer frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the filter name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // and its byte length
    emitter.instruction("str x2, [sp, #16]");                                   // which function names itself

    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("cbnz x9, __rt_fcw_prepend");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_create_append");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_CREATE_APPEND_HEAD.len()));
    emitter.instruction("b __rt_fcw_head");
    emitter.label("__rt_fcw_prepend");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_create_prepend");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_CREATE_PREPEND_HEAD.len()));
    emitter.label("__rt_fcw_head");
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    emitter.instruction("ldr x1, [sp, #0]");                                    // the filter name php quotes
    emitter.instruction("ldr x2, [sp, #8]");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_fp_create_tail");
    emitter.instruction(&format!("mov x2, #{}", FILTER_PARAM_CREATE_TAIL.len()));
    emitter.instruction("bl __rt_diag_warning");

    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the composer frame
    emitter.instruction("ret");
}

/// The x86_64 composer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unable-to-create-or-locate filter warning ---");
    emitter.label_global("__rt_filter_create_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the composer frame
    emitter.instruction("sub rsp, 32");                                         // reserve the name and selector slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the filter name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // and its byte length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // which function names itself

    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");
    emitter.instruction("jne __rt_fcw_prepend_x");
    emitter.instruction("lea rdi, [rip + _diag_fp_create_append]");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_CREATE_APPEND_HEAD.len()));
    emitter.instruction("jmp __rt_fcw_head_x");
    emitter.label("__rt_fcw_prepend_x");
    emitter.instruction("lea rdi, [rip + _diag_fp_create_prepend]");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_CREATE_PREPEND_HEAD.len()));
    emitter.label("__rt_fcw_head_x");
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the filter name php quotes
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("lea rdi, [rip + _diag_fp_create_tail]");
    emitter.instruction(&format!("mov rsi, {}", FILTER_PARAM_CREATE_TAIL.len()));
    emitter.instruction("call __rt_diag_warning");

    emitter.instruction("add rsp, 32");                                         // release the composer frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
