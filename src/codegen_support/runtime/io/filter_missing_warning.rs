//! Purpose:
//! Emits `__rt_filter_missing_warning`, which reports a filter name that resolves to nothing.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `stream_filter_append()` / `stream_filter_prepend()` lowerings, on the failure path.
//!
//! Key details:
//! - php-src names the FILTER in the text — `Unable to locate filter "no.such.filter"` — so
//!   the message is composed at run time rather than being a static string. elephc returned
//!   `false` silently, which left a misspelled filter name looking like a working one.
//! - The caller supplies the prefix, so `stream_filter_append()` and
//!   `stream_filter_prepend()` each name themselves without a branch in here.
//! - The name is clamped to what the buffer holds; a filter name is short in practice, and a
//!   truncated diagnostic beats writing past the buffer into the neighbouring globals.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Bytes reserved for the composed message.
pub(crate) const FILTER_MISSING_MSG_CAPACITY: usize = 256;

/// The most name bytes copied into the message.
const FILTER_NAME_CLAMP: usize = 160;

/// Emits `__rt_filter_missing_warning(prefix_ptr, prefix_len, name_ptr, name_len)`.
///
/// AArch64 takes `x0`/`x1`/`x2`/`x3`; x86_64 takes `rdi`/`rsi`/`rdx`/`rcx`.
pub fn emit_filter_missing_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 composer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unknown-filter warning ---");
    emitter.label_global("__rt_filter_missing_warning");
    emitter.instruction("sub sp, sp, #16");                                     // frame for the saved linkage
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    abi::emit_symbol_address(emitter, "x9", "_filter_missing_msg");             // the destination buffer
    emitter.instruction("mov x10, #0");                                         // bytes written so far

    // -- the caller's prefix, up to and including the opening quote --
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_fmw_prefix");
    emitter.instruction("cmp x11, x1");
    emitter.instruction("b.hs __rt_fmw_name");
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_fmw_prefix");

    // -- the filter name, clamped --
    emitter.label("__rt_fmw_name");
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_fmw_name_loop");
    emitter.instruction("cmp x11, x3");
    emitter.instruction("b.hs __rt_fmw_tail");
    emitter.instruction(&format!("cmp x11, #{FILTER_NAME_CLAMP}"));             // never overrun the buffer
    emitter.instruction("b.hs __rt_fmw_tail");
    emitter.instruction("ldrb w12, [x2, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_fmw_name_loop");

    // -- the closing quote and newline --
    emitter.label("__rt_fmw_tail");
    emitter.instruction("mov w12, #0x22");                                      // '"'
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("mov w12, #0x0a");                                      // '\n'
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");

    emitter.instruction("mov x1, x9");                                          // message pointer
    emitter.instruction("mov x2, x10");                                         // message length
    emitter.instruction("bl __rt_diag_warning");                                // stderr, and `@` suppresses it

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 composer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unknown-filter warning ---");
    emitter.label_global("__rt_filter_missing_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame

    abi::emit_symbol_address(emitter, "r9", "_filter_missing_msg");             // the destination buffer
    emitter.instruction("xor r10d, r10d");                                      // bytes written so far

    // -- the caller's prefix, up to and including the opening quote --
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_fmw_prefix_x86");
    emitter.instruction("cmp r11, rsi");
    emitter.instruction("jae __rt_fmw_name_x86");
    emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_fmw_prefix_x86");

    // -- the filter name, clamped --
    emitter.label("__rt_fmw_name_x86");
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_fmw_name_loop_x86");
    emitter.instruction("cmp r11, rcx");
    emitter.instruction("jae __rt_fmw_tail_x86");
    emitter.instruction(&format!("cmp r11, {FILTER_NAME_CLAMP}"));              // never overrun the buffer
    emitter.instruction("jae __rt_fmw_tail_x86");
    emitter.instruction("movzx eax, BYTE PTR [rdx + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_fmw_name_loop_x86");

    // -- the closing quote and newline --
    emitter.label("__rt_fmw_tail_x86");
    emitter.instruction("mov BYTE PTR [r9 + r10], 0x22");                       // '"'
    emitter.instruction("inc r10");
    emitter.instruction("mov BYTE PTR [r9 + r10], 0x0a");                       // '\n'
    emitter.instruction("inc r10");

    emitter.instruction("mov rdi, r9");                                         // message pointer
    emitter.instruction("mov rsi, r10");                                        // message length
    emitter.instruction("call __rt_diag_warning");                              // stderr, and `@` suppresses it

    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
