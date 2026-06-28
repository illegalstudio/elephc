//! Purpose:
//! Emits the web-gated `__rt_http_response_code` and `__rt_header` runtime helpers
//! backing PHP's `http_response_code()` and `header()` under `--web`. Mirrors the
//! `__rt_stdout_write` / `__rt_php_input` web-gating discipline.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - The EIR lowering of the `http_response_code` / `header` builtins calls these labels.
//!
//! Key details:
//! - In a `--web` build each routine forwards to a bridge setter via `bl_c`
//!   (`elephc_web_set_status` / `elephc_web_header`); in a non-web build it is a
//!   no-op and never names the bridge symbols, so non-web binaries link without them.
//! - `__rt_http_response_code`: status code in the first int arg register
//!   (`x0`/`rdi`); returns the resulting status in `x0`/`rax`.
//! - `__rt_header`: the four `header()` C-ABI args are already in the integer
//!   argument registers (`x0`=line ptr, `x1`=len, `x2`=replace, `x3`=code; x86_64
//!   `rdi`/`rsi`/`rdx`/`rcx`); the routine forwards them unchanged. No result.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

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
/// called. In `--web` it forwards them to `elephc_web_header`; in non-web it is a
/// no-op and never references the bridge symbol. The frame save/restore must not
/// touch the argument registers.
pub fn emit_header(emitter: &mut Emitter, web: bool) {
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
