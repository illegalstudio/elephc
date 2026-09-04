//! Purpose:
//! Emits `__rt_iconv_call` and `__rt_iconv_call_bool`, the two entry points every lowered
//! iconv builtin goes through.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::iconv::emit_iconv()`.
//! - `crate::codegen::lower_inst::builtins::iconv`, which stages the argument block.
//!
//! Key details:
//! - The caller owns the argument block; these helpers own the 48-byte result block and
//!   release its payloads before returning.
//! - The bridge is reached through `_elephc_iconv_call_fn`, published at the call site, so
//!   programs that never call an iconv builtin neither reference nor link elephc-iconv.
//! - A null slot degrades to PHP `false`, which is also what the bridge reports for an
//!   operation it cannot complete.
//! - `iconv_strpos()`'s out-of-range `$offset` arrives as its own result kind and leaves
//!   through the shared catchable `\ValueError` sequence instead of returning a value.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::value_error;
use crate::codegen_support::runtime::data::ICONV_STRPOS_OFFSET_MSG;

/// Byte offsets inside the bridge's result block, mirroring `IconvResultBlock`.
pub(super) const RESULT_KIND: usize = 0;
pub(super) const RESULT_INT: usize = 8;
pub(super) const RESULT_BYTES: usize = 16;
pub(super) const RESULT_LEN: usize = 24;
const RESULT_DIAGNOSTIC: usize = 32;
const RESULT_DIAGNOSTIC_LEN: usize = 40;

/// Result kind that asks the runtime to throw `iconv_strpos()`'s `ValueError`.
const KIND_OFFSET_VALUE_ERROR: usize = 5;

/// Emits both iconv entry points for the active target.
pub(super) fn emit_iconv_call(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_iconv_call_x86_64(emitter);
        return;
    }
    emit_iconv_call_aarch64(emitter);
}

/// Emits the AArch64 `__rt_iconv_call` and `__rt_iconv_call_bool` helpers.
///
/// Input:  x0 = staged argument block pointer.
/// Output: x0 = boxed Mixed result, or 0/1 for the boolean variant.
fn emit_iconv_call_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv call ---");
    emitter.label_global("__rt_iconv_call");

    // -- reserve the result block plus saved-result and frame-linkage slots --
    emitter.instruction("sub sp, sp, #80");                                     // allocate the 48-byte result block, a saved result, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame
    emitter.instruction("add x1, sp, #0");                                      // pass the result block to the shared invoker
    emitter.instruction("bl __rt_iconv_invoke");                                // run the bridge operation and emit its diagnostic
    emitter.instruction(&format!("ldr x9, [sp, #{}]", RESULT_KIND));            // load the outcome kind the bridge reported
    emitter.instruction(&format!("cmp x9, #{}", KIND_OFFSET_VALUE_ERROR));      // is this the out-of-range offset outcome?
    emitter.instruction("b.eq __rt_iconv_call_offset_error");                   // leave through the catchable ValueError path
    emitter.instruction("add x0, sp, #0");                                      // pass the result block to the materializer
    emitter.instruction("bl __rt_iconv_materialize");                           // turn the reported outcome into a boxed Mixed value
    emitter.instruction("str x0, [sp, #56]");                                   // save the boxed result across the bridge release call
    emitter.instruction("add x0, sp, #0");                                      // pass the result block to the release helper
    emitter.instruction("bl __rt_iconv_release_block");                         // free the payloads the bridge allocated
    emitter.instruction("ldr x0, [sp, #56]");                                   // restore the boxed result for the caller
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed result

    emitter.label("__rt_iconv_call_offset_error");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame pointer before throwing
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame before throwing
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_iconv_strpos_offset_msg",
        ICONV_STRPOS_OFFSET_MSG.len(),
    );

    emitter.blank();
    emitter.comment("--- runtime: iconv call (boolean result) ---");
    emitter.label_global("__rt_iconv_call_bool");
    emitter.instruction("sub sp, sp, #80");                                     // allocate the 48-byte result block, a saved result, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame
    emitter.instruction("add x1, sp, #0");                                      // pass the result block to the shared invoker
    emitter.instruction("bl __rt_iconv_invoke");                                // run the bridge operation and emit its diagnostic
    emitter.instruction(&format!("ldr x9, [sp, #{}]", RESULT_KIND));            // load the outcome kind the bridge reported
    emitter.instruction("cmp x9, #3");                                          // kind 3 is the only outcome that means PHP true
    emitter.instruction("cset x9, eq");                                         // materialize the PHP boolean without branching
    emitter.instruction("str x9, [sp, #56]");                                   // save the boolean across the bridge release call
    emitter.instruction("add x0, sp, #0");                                      // pass the result block to the release helper
    emitter.instruction("bl __rt_iconv_release_block");                         // free the payloads the bridge allocated
    emitter.instruction("ldr x0, [sp, #56]");                                   // restore the boolean for the caller
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the PHP boolean in the integer result register

    emit_iconv_invoke_aarch64(emitter);
    emit_iconv_release_aarch64(emitter);
}

/// Emits `__rt_iconv_invoke`, the shared bridge call and diagnostic printer.
///
/// Input: x0 = argument block pointer, x1 = result block pointer owned by the caller.
fn emit_iconv_invoke_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv invoke ---");
    emitter.label_global("__rt_iconv_invoke");
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve the caller frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame for the indirect bridge call
    emitter.instruction("mov x11, x1");                                         // keep the caller's result-block pointer in a scratch register
    emitter.instruction("str x11, [sp, #16]");                                  // retain the result-block pointer across the bridge call
    emitter.instruction(&format!("str xzr, [x11, #{}]", RESULT_KIND));          // default the outcome to PHP false
    emitter.instruction(&format!("str xzr, [x11, #{}]", RESULT_DIAGNOSTIC));    // default the diagnostic to absent
    abi::emit_symbol_address(emitter, "x9", "_elephc_iconv_call_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published iconv bridge entry point
    emitter.instruction("cbz x9, __rt_iconv_invoke_missing");                   // a null slot means the program never linked elephc-iconv
    emitter.instruction("mov x1, x11");                                         // C arg1 = result block pointer
    abi::emit_call_reg(emitter, "x9");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the result-block pointer after the bridge call
    emitter.instruction(&format!("ldr x1, [x11, #{}]", RESULT_DIAGNOSTIC));     // load the diagnostic line the bridge formatted
    emitter.instruction("cbz x1, __rt_iconv_invoke_missing");                   // skip the warning when the operation succeeded
    emitter.instruction(&format!("ldr x2, [x11, #{}]", RESULT_DIAGNOSTIC_LEN)); // load the diagnostic byte length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress php-src's diagnostic line
    emitter.label("__rt_iconv_invoke_missing");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore the caller frame pointer and return address
    emitter.instruction("ret");                                                 // return to the entry point that owns the result block
}

/// Emits `__rt_iconv_release_block`, which frees the bridge's owned payloads.
///
/// Input: x0 = result block pointer. A null slot means nothing was ever allocated.
fn emit_iconv_release_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv release ---");
    emitter.label_global("__rt_iconv_release_block");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame for the indirect bridge call
    abi::emit_symbol_address(emitter, "x9", "_elephc_iconv_release_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published release entry point
    emitter.instruction("cbz x9, __rt_iconv_release_done");                     // nothing was allocated when the bridge is absent
    abi::emit_call_reg(emitter, "x9");
    emitter.label("__rt_iconv_release_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame pointer and return address
    emitter.instruction("ret");                                                 // return to the entry point that owns the result block
}

/// Emits the Linux x86_64 `__rt_iconv_call` and `__rt_iconv_call_bool` helpers.
///
/// Input:  rdi = staged argument block pointer.
/// Output: rax = boxed Mixed result, or 0/1 for the boolean variant.
fn emit_iconv_call_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv call ---");
    emitter.label_global("__rt_iconv_call");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the result block
    emitter.instruction("sub rsp, 64");                                         // reserve the 48-byte result block plus a saved result
    emitter.instruction("lea rsi, [rbp - 64]");                                 // address the result block for the shared invoker
    emitter.instruction("call __rt_iconv_invoke");                              // run the bridge operation and emit its diagnostic
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", 64 - RESULT_KIND)); // load the outcome kind the bridge reported
    emitter.instruction(&format!("cmp r10, {}", KIND_OFFSET_VALUE_ERROR));      // is this the out-of-range offset outcome?
    emitter.instruction("je __rt_iconv_call_offset_error_linux_x86_64");        // leave through the catchable ValueError path
    emitter.instruction("lea rdi, [rbp - 64]");                                 // pass the result block to the materializer
    emitter.instruction("call __rt_iconv_materialize");                         // turn the reported outcome into a boxed Mixed value
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boxed result across the bridge release call
    emitter.instruction("lea rdi, [rbp - 64]");                                 // pass the result block to the release helper
    emitter.instruction("call __rt_iconv_release_block");                       // free the payloads the bridge allocated
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the boxed result for the caller
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed result

    emitter.label("__rt_iconv_call_offset_error_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    // Unwinding first puts rsp back at THIS helper's entry alignment — `rsp % 16 == 8`, the
    // shape a `call` leaves — which is one boundary away from what the throw body's own
    // `call __rt_heap_alloc` needs. Every other user of that body reaches it from inside a
    // live frame, so it preserves alignment rather than flipping it; this path is the one
    // that has to make up the difference, in its own frame where the reason is visible.
    emitter.instruction("sub rsp, 8");                                          // realign for the shared throw body
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_iconv_strpos_offset_msg",
        ICONV_STRPOS_OFFSET_MSG.len(),
    );

    emitter.blank();
    emitter.comment("--- runtime: iconv call (boolean result) ---");
    emitter.label_global("__rt_iconv_call_bool");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the result block
    emitter.instruction("sub rsp, 64");                                         // reserve the 48-byte result block plus a saved result
    emitter.instruction("lea rsi, [rbp - 64]");                                 // address the result block for the shared invoker
    emitter.instruction("call __rt_iconv_invoke");                              // run the bridge operation and emit its diagnostic
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", 64 - RESULT_KIND)); // load the outcome kind the bridge reported
    emitter.instruction("xor eax, eax");                                        // default the PHP boolean to false
    emitter.instruction("cmp r10, 3");                                          // kind 3 is the only outcome that means PHP true
    emitter.instruction("sete al");                                             // materialize the PHP boolean without branching
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boolean across the bridge release call
    emitter.instruction("lea rdi, [rbp - 64]");                                 // pass the result block to the release helper
    emitter.instruction("call __rt_iconv_release_block");                       // free the payloads the bridge allocated
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the boolean for the caller
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP boolean in the integer result register

    emit_iconv_invoke_x86_64(emitter);
    emit_iconv_release_x86_64(emitter);
}

/// Emits the Linux x86_64 `__rt_iconv_invoke` bridge call and diagnostic printer.
///
/// Input: rdi = argument block pointer, rsi = result block pointer.
fn emit_iconv_invoke_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv invoke ---");
    emitter.label_global("__rt_iconv_invoke");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned frame for the indirect bridge call
    emitter.instruction("sub rsp, 16");                                         // keep the nested bridge call 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // retain the result-block pointer across the bridge call
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], 0", RESULT_KIND));  // default the outcome to PHP false
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], 0", RESULT_DIAGNOSTIC)); // default the diagnostic to absent
    abi::emit_load_symbol_to_reg(emitter, "r11", "_elephc_iconv_call_fn", 0);
    emitter.instruction("test r11, r11");                                       // a null slot means the program never linked elephc-iconv
    emitter.instruction("jz __rt_iconv_invoke_missing_linux_x86_64");           // skip the bridge call and keep the PHP false default
    abi::emit_call_reg(emitter, "r11");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the result-block pointer after the bridge call
    emitter.instruction(&format!("mov rdi, QWORD PTR [r11 + {}]", RESULT_DIAGNOSTIC)); // load the diagnostic line the bridge formatted
    emitter.instruction("test rdi, rdi");                                       // skip the warning when the operation succeeded
    emitter.instruction("jz __rt_iconv_invoke_missing_linux_x86_64");           // nothing to print for a successful operation
    emitter.instruction(&format!("mov rsi, QWORD PTR [r11 + {}]", RESULT_DIAGNOSTIC_LEN)); // load the diagnostic byte length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress php-src's diagnostic line
    emitter.label("__rt_iconv_invoke_missing_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the invoker frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the entry point that owns the result block
}

/// Emits the Linux x86_64 `__rt_iconv_release_block` helper.
///
/// Input: rdi = result block pointer.
fn emit_iconv_release_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv release ---");
    emitter.label_global("__rt_iconv_release_block");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned frame for the indirect bridge call
    abi::emit_load_symbol_to_reg(emitter, "r11", "_elephc_iconv_release_fn", 0);
    emitter.instruction("test r11, r11");                                       // nothing was allocated when the bridge is absent
    emitter.instruction("jz __rt_iconv_release_done_linux_x86_64");             // skip the release call for an unlinked bridge
    abi::emit_call_reg(emitter, "r11");
    emitter.label("__rt_iconv_release_done_linux_x86_64");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the entry point that owns the result block
}
