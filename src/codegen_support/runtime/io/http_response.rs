//! Purpose:
//! Emits status/header helpers and joins shared MIME state when mbstring is selected.
//! Web builds also forward accepted headers to their response sink.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The EIR lowering of the `http_response_code` / `header` builtins calls these labels.
//!
//! Key details:
//! - Web builds forward status/header operations through their bridge setters.
//! - Mbstring builds additionally track validated MIME metadata outside the web bridge.
//! - Non-web binaries never reference web bridge symbols.
//! - `__rt_http_response_code`: status code in the first int arg register
//!   (`x0`/`rdi`); returns the resulting status in `x0`/`rax`.
//! - `__rt_header`: the four `header()` C-ABI args are already in the integer
//!   argument registers (`x0`=line ptr, `x1`=len, `x2`=replace, `x3`=code; x86_64
//!   `rdi`/`rsi`/`rdx`/`rcx`); the routine forwards them unchanged. No result.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_http_response_code` runtime helper.
///
/// Input: the status code in the first integer argument register. Output: the
/// resulting status as an int in the result register. In `--web` it calls
/// `elephc_web_set_status` (which reads the code and returns the previous status);
/// in non-web it returns 0 without referencing the bridge.
pub fn emit_http_response_code(emitter: &mut Emitter, web: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_http_response_code_x86_64(emitter, web);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: http_response_code ---");
    emitter.label_global("__rt_http_response_code");

    emitter.instruction("sub sp, sp, #16");                                     // frame so the bridge call can clobber the link register
    emitter.instruction("str x30, [sp]");                                       // save the caller return address before the nested call

    if web {
        emitter.bl_c("elephc_web_set_status");                                  // x0 = previous status; sets the status when the code is > 0
    } else {
        emitter.instruction("mov x0, #0");                                      // non-web: no status machinery, return 0
    }

    emitter.instruction("ldr x30, [sp]");                                       // restore the caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller (result in x0)
}

/// Emits the x86_64 Linux variant of `__rt_http_response_code`.
fn emit_http_response_code_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: http_response_code ---");
    emitter.label_global("__rt_http_response_code");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align rsp for the call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base

    if web {
        emitter.bl_c("elephc_web_set_status");                                  // rax = previous status; sets the status when the code is > 0
    } else {
        emitter.instruction("xor eax, eax");                                    // non-web: no status machinery, return 0
    }

    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller (result in rax)
}

/// Emits the `__rt_header` runtime helper.
///
/// The four `header()` C-ABI arguments (line pointer, line length, `$replace`,
/// `$response_code`) are already in the integer argument registers when this is
/// called. Mbstring builds validate response metadata before optional web publication.
/// Other builds retain their existing web-forwarding or CLI no-op behavior.
pub fn emit_header(emitter: &mut Emitter, web: bool, mbstring: bool) {
    if mbstring {
        super::response_metadata::emit_header(emitter, web);
        return;
    }
    if emitter.target.arch == Arch::X86_64 {
        emit_header_x86_64(emitter, web);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: header ---");
    emitter.label_global("__rt_header");

    if web {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // save frame/link registers (the forward call clobbers x30)
        emitter.instruction("mov x29, sp");                                     // establish a frame pointer for the call
        emitter.bl_c("elephc_web_header");                                      // forward (x0=ptr, x1=len, x2=replace, x3=code) to the bridge
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore frame/link registers
    }

    emitter.instruction("ret");                                                 // return to the caller (header() is void)
}

/// Emits the x86_64 Linux variant of `__rt_header`.
fn emit_header_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: header ---");
    emitter.label_global("__rt_header");

    if web {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer and align rsp for the call
        emitter.instruction("mov rbp, rsp");                                    // establish a stable frame base
        emitter.bl_c("elephc_web_header");                                      // forward (rdi=ptr, rsi=len, rdx=replace, rcx=code) to the bridge
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
    }

    emitter.instruction("ret");                                                 // return to the caller (header() is void)
}

/// Emits the web-gated `header_remove()` runtime bridge.
pub fn emit_header_remove(emitter: &mut Emitter, web: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_header_remove_x86_64(emitter, web);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: header_remove ---");
    emitter.label_global("__rt_header_remove");
    if web {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve frame and return address across the bridge call
        emitter.instruction("mov x29, sp");                                    // establish a frame pointer for the C ABI call
        emitter.bl_c("elephc_web_header_remove");
        emitter.instruction("ldp x29, x30, [sp], #16");                        // restore the caller frame
    }
    emitter.instruction("ret");
}

/// Emits the x86_64 Linux variant of the web-gated `header_remove()` helper.
fn emit_header_remove_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: header_remove ---");
    emitter.label_global("__rt_header_remove");
    if web {
        emitter.instruction("push rbp");                                       // preserve the caller frame and align the bridge call
        emitter.instruction("mov rbp, rsp");
        emitter.bl_c("elephc_web_header_remove");
        emitter.instruction("pop rbp");
    }
    emitter.instruction("ret");
}

/// Emits the `__rt_headers_list` runtime helper backing PHP's `headers_list()`.
///
/// Output: `x0`/`rax` = a fresh string array the caller owns. Under `--web` it holds one
/// `"Name: value"` line per pending response header, in send order, read through
/// `elephc_web_response_header_line`; everywhere else it is empty, which is what the php CLI
/// answers too (the CLI SAPI keeps no header list). The web bridge formats each line into a
/// scratch buffer it reuses, so every line is copied by `__rt_array_push_str` before the next
/// call.
pub fn emit_headers_list(emitter: &mut Emitter, web: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_headers_list_x86_64(emitter, web);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: headers_list ---");
    emitter.label_global("__rt_headers_list");
    emitter.instruction("sub sp, sp, #48");                                     // reserve slots for the result array, the index and the line length
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across the helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("mov x0, #8");                                          // initial capacity for a typical handful of headers
    emitter.instruction("mov x1, #16");                                         // string slots carry a pointer and a length
    emitter.instruction("bl __rt_array_new");                                   // allocate the result array the caller will own
    emitter.instruction("str x0, [sp, #0]");                                    // keep the result array across the bridge calls
    if web {
        emitter.instruction("str xzr, [sp, #8]");                               // start at the first pending header
        emitter.label("__rt_headers_list_loop");
        emitter.instruction("ldr x0, [sp, #8]");                                // pass the header index
        emitter.instruction("add x1, sp, #16");                                 // pass the slot that receives the line length
        emitter.bl_c("elephc_web_response_header_line");
        emitter.instruction("cbz x0, __rt_headers_list_done");                  // a null line means every header has been listed
        emitter.instruction("mov x1, x0");                                      // the line bytes borrowed from the bridge scratch
        emitter.instruction("ldr x2, [sp, #16]");                               // the line length the bridge reported
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the result array
        emitter.instruction("bl __rt_array_push_str");                          // copy the line into an owned string slot
        emitter.instruction("str x0, [sp, #0]");                                // keep the possibly-grown result array
        emitter.instruction("ldr x9, [sp, #8]");                                // reload the header index
        emitter.instruction("add x9, x9, #1");                                  // advance to the next pending header
        emitter.instruction("str x9, [sp, #8]");                                // store the advanced index
        emitter.instruction("b __rt_headers_list_loop");                        // list the next header
        emitter.label("__rt_headers_list_done");
    }
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the result array
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned string array
}

/// Emits the x86_64 Linux variant of `__rt_headers_list`.
fn emit_headers_list_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: headers_list ---");
    emitter.label_global("__rt_headers_list");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame base
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the result array, the index and the line length
    emitter.instruction("mov rdi, 8");                                          // initial capacity for a typical handful of headers
    emitter.instruction("mov rsi, 16");                                         // string slots carry a pointer and a length
    emitter.instruction("call __rt_array_new");                                 // allocate the result array the caller will own
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the result array across the bridge calls
    if web {
        emitter.instruction("mov QWORD PTR [rbp - 16], 0");                     // start at the first pending header
        emitter.label("__rt_headers_list_loop");
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // pass the header index
        emitter.instruction("lea rsi, [rbp - 24]");                             // pass the slot that receives the line length
        emitter.bl_c("elephc_web_response_header_line");
        emitter.instruction("test rax, rax");                                   // a null line means every header has been listed
        emitter.instruction("jz __rt_headers_list_done");                       // stop once the bridge runs out of headers
        emitter.instruction("mov rsi, rax");                                    // the line bytes borrowed from the bridge scratch
        emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                   // the line length the bridge reported
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // reload the result array
        emitter.instruction("call __rt_array_push_str");                        // copy the line into an owned string slot
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // keep the possibly-grown result array
        emitter.instruction("add QWORD PTR [rbp - 16], 1");                     // advance to the next pending header
        emitter.instruction("jmp __rt_headers_list_loop");                      // list the next header
        emitter.label("__rt_headers_list_done");
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the result array
    emitter.instruction("add rsp, 32");                                         // release the helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned string array
}
