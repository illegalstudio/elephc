//! Purpose:
//! Emits `__rt_stream_record_mode`, which stores the mode string `stream_get_meta_data()`
//! reports for a stream.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `fopen()` lowering, once the opened stream has been boxed and its URI recorded.
//!
//! Key details:
//! - PHP echoes the mode the caller passed and does not normalise it, so `rb` stays `rb` and `a`
//!   stays `a`. Deriving it from the descriptor's `F_GETFL` access bits — which is what the
//!   metadata helper does without this — collapses `a` to `w`, `w+` to `r+`, and loses the `b`.
//! - The memory wrappers are the exception: they report the mode of the memory stream PHP built,
//!   not the caller's. `php://memory`/`php://temp` answer `a+b` for an append mode, `w+b` when the
//!   mode asks for any write access, and `rb` otherwise; `php://output` always answers `wb` and
//!   `php://input` always `rb`.
//! - The wrapper is recognised from the URI already published on the state, so this helper must run
//!   after `__rt_stream_record_meta`. One byte after `php://` separates every case: `memory`/`temp`
//!   map, `output` and `input` are fixed, and `stdin`/`stdout`/`stderr`/`fd`/`filter` echo.
//! - The recorded bytes are persisted into owned storage and released by
//!   `__rt_stream_destroy_state` beside the URI, so a mode built at run time cannot dangle.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_MODE_LEN_OFFSET, STREAM_MODE_PTR_OFFSET, STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_stream_record_mode(handle, mode_ptr, mode_len) -> handle`.
///
/// AArch64 receives `x0`/`x1`/`x2`; x86_64 receives `rdi`/`rsi`/`rdx`. A stale handle is returned
/// unchanged. The handle comes back in the result register so the caller can chain.
pub fn emit_stream_record_mode(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_stream_record_mode_aarch64(emitter),
        Arch::X86_64 => emit_stream_record_mode_x86_64(emitter),
    }
}

/// Emits the AArch64 mode recorder.
fn emit_stream_record_mode_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: record the mode string a stream reports ---");
    emitter.label_global("__rt_stream_record_mode");
    emitter.instruction("sub sp, sp, #64");                                     // frame for the handle, the mode pair and a small literal scratch
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the caller's mode pointer
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the caller's mode length
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_srm_done");                               // a stale handle records nothing
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the state across persistence

    // -- classify the wrapper from the URI the metadata recorder already published --
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]"));   // the recorded URI
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]"));   // and its length
    emitter.instruction("cbz x10, __rt_srm_verbatim");                          // no URI: the caller's spelling stands
    emitter.instruction("cmp x11, #7");                                         // "php://" plus the byte that names the wrapper
    emitter.instruction("b.lt __rt_srm_verbatim");                              // too short to be a php:// URI
    for (offset, byte) in b"php://".iter().enumerate() {
        emitter.instruction(&format!("ldrb w12, [x10, #{}]", offset));          // load one candidate scheme byte
        emitter.instruction(&format!("cmp w12, #{}", byte));                    // compare against the canonical php:// byte
        emitter.instruction("b.ne __rt_srm_verbatim");                          // any other wrapper echoes the caller's mode
    }
    emitter.instruction("ldrb w12, [x10, #6]");                                 // the first byte of the php:// wrapper name
    emitter.instruction("cmp w12, #0x6F");                                      // 'o' as in output
    emitter.instruction("b.eq __rt_srm_wb");                                    // the output wrapper always reports wb
    emitter.instruction("cmp w12, #0x69");                                      // 'i' as in input
    emitter.instruction("b.eq __rt_srm_rb");                                    // the input wrapper always reports rb
    emitter.instruction("cmp w12, #0x6D");                                      // 'm' as in memory
    emitter.instruction("b.eq __rt_srm_memory");                                // the memory wrapper maps the mode
    emitter.instruction("cmp w12, #0x74");                                      // 't' as in temp
    emitter.instruction("b.eq __rt_srm_memory");                                // temp is the same memory stream
    emitter.instruction("b __rt_srm_verbatim");                                 // stdin, stdout, stderr, fd and filter echo

    // -- the memory wrappers report the mode of the stream PHP built, not the caller's --
    emitter.label("__rt_srm_memory");
    emitter.instruction("ldr x10, [sp, #8]");                                   // the caller's mode pointer
    emitter.instruction("ldr x11, [sp, #16]");                                  // and its length
    emitter.instruction("cbz x11, __rt_srm_rb");                                // an empty mode is read-only
    emitter.instruction("ldrb w12, [x10]");                                     // the leading mode letter
    emitter.instruction("cmp w12, #0x61");                                      // 'a' selects the append memory stream
    emitter.instruction("b.eq __rt_srm_apb");                                   // which reports a+b
    emitter.instruction("mov x13, #0");                                         // index of the mode byte under inspection
    emitter.label("__rt_srm_scan");
    emitter.instruction("cmp x13, x11");                                        // scanned the whole mode?
    emitter.instruction("b.ge __rt_srm_rb");                                    // no write access requested: read-only
    emitter.instruction("ldrb w12, [x10, x13]");                                // load one mode byte
    emitter.instruction("cmp w12, #0x77");                                      // 'w'
    emitter.instruction("b.eq __rt_srm_wpb");                                   // any write access selects the default memory stream
    emitter.instruction("cmp w12, #0x61");                                      // 'a'
    emitter.instruction("b.eq __rt_srm_wpb");
    emitter.instruction("cmp w12, #0x2B");                                      // '+'
    emitter.instruction("b.eq __rt_srm_wpb");
    emitter.instruction("add x13, x13, #1");                                    // advance to the next mode byte
    emitter.instruction("b __rt_srm_scan");                                     // keep scanning

    emit_literal_mode_aarch64(emitter, "__rt_srm_wb", b"wb");
    emit_literal_mode_aarch64(emitter, "__rt_srm_rb", b"rb");
    emit_literal_mode_aarch64(emitter, "__rt_srm_apb", b"a+b");
    emit_literal_mode_aarch64(emitter, "__rt_srm_wpb", b"w+b");

    emitter.label("__rt_srm_verbatim");
    emitter.instruction("ldr x1, [sp, #8]");                                    // report the caller's own mode bytes
    emitter.instruction("ldr x2, [sp, #16]");                                   // with the caller's own length

    emitter.label("__rt_srm_persist");
    emitter.instruction("str x2, [sp, #40]");                                   // remember the reported length across persistence
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the bytes into owned storage
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the state for ownership replacement
    emitter.instruction(&format!("ldr x10, [x9, #{STREAM_MODE_PTR_OFFSET}]"));  // capture any previous owned mode
    emitter.instruction(&format!("str x1, [x9, #{STREAM_MODE_PTR_OFFSET}]"));   // publish the new owned mode pointer
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the reported length
    emitter.instruction(&format!("str x11, [x9, #{STREAM_MODE_LEN_OFFSET}]"));  // publish the reported length
    emitter.instruction("mov x0, x10");                                         // pass the detached previous mode to a safe release
    emitter.instruction("bl __rt_heap_free_safe");                              // release only live heap-backed mode storage

    emitter.label("__rt_srm_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // hand the handle back unchanged
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// Emits one AArch64 arm that materialises a fixed mode spelling in the helper's scratch slot.
///
/// The bytes are built on the frame rather than in the data section because the persistence step
/// below copies them into owned storage anyway, so nothing outlives the call.
fn emit_literal_mode_aarch64(emitter: &mut Emitter, label: &str, mode: &[u8]) {
    emitter.label(label);
    for (offset, byte) in mode.iter().enumerate() {
        emitter.instruction(&format!("mov w12, #{}", byte));                    // one byte of the reported mode
        emitter.instruction(&format!("strb w12, [sp, #{}]", 32 + offset));      // stage it in the scratch slot
    }
    emitter.instruction("add x1, sp, #32");                                     // point at the staged mode
    emitter.instruction(&format!("mov x2, #{}", mode.len()));                   // with its byte length
    emitter.instruction("b __rt_srm_persist");                                  // persist and publish it
}

/// Emits the x86_64 mode recorder.
fn emit_stream_record_mode_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: record the mode string a stream reports ---");
    emitter.label_global("__rt_stream_record_mode");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve the spill slots and literal scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the caller's mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the caller's mode length
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_srm_done_x86");                                // a stale handle records nothing
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the state across persistence

    // See the AArch64 counterpart for the classification rules.
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_srm_verbatim_x86");                            // no URI: the caller's spelling stands
    emitter.instruction("cmp r11, 7");                                          // "php://" plus the byte that names the wrapper
    emitter.instruction("jl __rt_srm_verbatim_x86");                            // too short to be a php:// URI
    for (offset, byte) in b"php://".iter().enumerate() {
        emitter.instruction(&format!("cmp BYTE PTR [r10 + {}], {}", offset, byte)); // compare one byte against the canonical php:// prefix
        emitter.instruction("jne __rt_srm_verbatim_x86");                       // any other wrapper echoes the caller's mode
    }
    emitter.instruction("movzx eax, BYTE PTR [r10 + 6]");                       // the first byte of the php:// wrapper name
    emitter.instruction("cmp al, 0x6F");                                        // 'o' as in output
    emitter.instruction("je __rt_srm_wb_x86");                                  // the output wrapper always reports wb
    emitter.instruction("cmp al, 0x69");                                        // 'i' as in input
    emitter.instruction("je __rt_srm_rb_x86");                                  // the input wrapper always reports rb
    emitter.instruction("cmp al, 0x6D");                                        // 'm' as in memory
    emitter.instruction("je __rt_srm_memory_x86");                              // the memory wrapper maps the mode
    emitter.instruction("cmp al, 0x74");                                        // 't' as in temp
    emitter.instruction("je __rt_srm_memory_x86");                              // temp is the same memory stream
    emitter.instruction("jmp __rt_srm_verbatim_x86");                           // stdin, stdout, stderr, fd and filter echo

    emitter.label("__rt_srm_memory_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // the caller's mode pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // and its length
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_srm_rb_x86");                                  // an empty mode is read-only
    emitter.instruction("cmp BYTE PTR [r10], 0x61");                            // 'a' selects the append memory stream
    emitter.instruction("je __rt_srm_apb_x86");                                 // which reports a+b
    emitter.instruction("xor ecx, ecx");                                        // index of the mode byte under inspection
    emitter.label("__rt_srm_scan_x86");
    emitter.instruction("cmp rcx, r11");                                        // scanned the whole mode?
    emitter.instruction("jae __rt_srm_rb_x86");                                 // no write access requested: read-only
    emitter.instruction("movzx eax, BYTE PTR [r10 + rcx]");                     // load one mode byte
    emitter.instruction("cmp al, 0x77");                                        // 'w'
    emitter.instruction("je __rt_srm_wpb_x86");                                 // any write access selects the default memory stream
    emitter.instruction("cmp al, 0x61");                                        // 'a'
    emitter.instruction("je __rt_srm_wpb_x86");
    emitter.instruction("cmp al, 0x2B");                                        // '+'
    emitter.instruction("je __rt_srm_wpb_x86");
    emitter.instruction("inc rcx");                                             // advance to the next mode byte
    emitter.instruction("jmp __rt_srm_scan_x86");                               // keep scanning

    emit_literal_mode_x86_64(emitter, "__rt_srm_wb_x86", b"wb");
    emit_literal_mode_x86_64(emitter, "__rt_srm_rb_x86", b"rb");
    emit_literal_mode_x86_64(emitter, "__rt_srm_apb_x86", b"a+b");
    emit_literal_mode_x86_64(emitter, "__rt_srm_wpb_x86", b"w+b");

    emitter.label("__rt_srm_verbatim_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // report the caller's own mode bytes
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // with the caller's own length

    emitter.label("__rt_srm_persist_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // remember the reported length across persistence
    emitter.instruction("call __rt_str_persist");                               // duplicate the bytes into owned storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the state for ownership replacement
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [r10 + {STREAM_MODE_PTR_OFFSET}]"
    ));                                                                         // capture any previous owned mode
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {STREAM_MODE_PTR_OFFSET}], rax"
    ));                                                                         // publish the new owned mode pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the reported length
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {STREAM_MODE_LEN_OFFSET}], rax"
    ));                                                                         // publish the reported length
    emitter.instruction("mov rax, r11");                                        // pass the detached previous mode to a safe release
    emitter.instruction("call __rt_heap_free_safe");                            // release only live heap-backed mode storage

    emitter.label("__rt_srm_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // hand the handle back unchanged
    emitter.instruction("add rsp, 64");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits one x86_64 arm that materialises a fixed mode spelling in the helper's scratch slot.
fn emit_literal_mode_x86_64(emitter: &mut Emitter, label: &str, mode: &[u8]) {
    emitter.label(label);
    for (offset, byte) in mode.iter().enumerate() {
        emitter.instruction(&format!(
            "mov BYTE PTR [rbp - {}], {}", 40 - offset, byte
        ));                                                                     // stage one byte of the reported mode
    }
    emitter.instruction("lea rax, [rbp - 40]");                                 // point at the staged mode
    emitter.instruction(&format!("mov rdx, {}", mode.len()));                   // with its byte length
    emitter.instruction("jmp __rt_srm_persist_x86");                            // persist and publish it
}
