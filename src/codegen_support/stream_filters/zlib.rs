//! Purpose:
//! Emits the `zlib.deflate` write-direction stream filter attachment helpers for
//! EIR stream-filter lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io` when lowering `stream_filter_append`
//!   or `stream_filter_prepend` for the `"zlib.deflate"` literal filter.
//!
//! Key details:
//! - The libz symbols (`deflate`, `deflateEnd`, `deflateInit_`) are referenced
//!   only from this builtin's USER asm. The shared runtime object never names a
//!   libz symbol, so non-zlib programs still link without `-lz`.
//! - Two per-program helper routines (`fwrite` and `close`) are emitted inline,
//!   skipped over by an unconditional branch, and their addresses stored into
//!   the `_zlib_fwrite_fn` / `_zlib_close_fn` globals. `__rt_fwrite` and the
//!   `fclose` builtin reach libz indirectly through those function pointers.
//! - Per-descriptor `z_stream` state lives in the `_zstream_handles` table,
//!   indexed by file descriptor. The write-filter table entry is set to id 4.
//! - `z_stream` is target-layout-sensitive: 112 bytes on LP64 and 88 bytes on
//!   Windows LLP64, where `unsigned long` is 32-bit. Zeroing it leaves
//!   `zalloc`/`zfree` NULL so zlib uses its own allocator. The struct itself is
//!   intentionally not freed on close — a small, documented v1 leak.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Platform, Target};

/// Target C layout for the `z_stream` fields emitted code reads or writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ZStreamLayout {
    pub(crate) size: i64,
    pub(crate) next_out: i64,
    pub(crate) avail_out: i64,
    pub(crate) total_out: i64,
}

/// Returns zlib's C `z_stream` layout for the target data model.
pub(crate) fn z_stream_layout(target: Target) -> ZStreamLayout {
    if target.platform == Platform::Windows {
        ZStreamLayout {
            size: 88,
            next_out: 16,
            avail_out: 24,
            total_out: 28,
        }
    } else {
        ZStreamLayout {
            size: 112,
            next_out: 24,
            avail_out: 32,
            total_out: 40,
        }
    }
}
/// Capacity of the shared `_stream_filter_buf` scratch used as the deflate
/// output window.
const FILTER_BUF_SIZE: i64 = 65536;

/// Emits the ARM64 helpers, then the deflate-stream initialization.
pub(crate) fn emit_arm64(
    emitter: &mut Emitter,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
    level: i64,
) {
    let zstream = z_stream_layout(emitter.target);
    // -- jump past the helper bodies so normal flow never falls into them --
    emitter.instruction(&format!("b {}", skip_label));                          // skip over the inline zlib helper routines

    // ================================================================
    // zlib deflate fwrite helper.
    // Input:  x0 = fd, x1 = payload pointer, x2 = payload length.
    // Output: x0 = the input payload length (bytes "written").
    // ================================================================
    emitter.label(fwrite_label);
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]=fd [8]=length [16]=z_stream ptr [32]=x29 [40]=x30
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor for the write loop
    emitter.instruction("str x2, [sp, #8]");                                    // save the payload length as the return value

    // -- load this descriptor's z_stream handle and seed the input window --
    abi::emit_symbol_address(emitter, "x9", "_zstream_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // x10 = z_stream pointer for this descriptor
    emitter.instruction("str x10, [sp, #16]");                                  // save the z_stream pointer across the calls
    emitter.instruction("str x1, [x10, #0]");                                   // z_stream.next_in = payload pointer
    emitter.instruction("str w2, [x10, #8]");                                   // z_stream.avail_in = payload length

    // -- deflate loop: drain next_in into the scratch window and write it out --
    emitter.label(&format!("{}_loop", fwrite_label));
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "x11", "_stream_filter_buf");
    emitter.instruction("str x11, [x10, #24]");                                 // z_stream.next_out = scratch window base
    emitter.instruction(&format!("mov w12, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w12, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); // high half of the scratch window capacity
    emitter.instruction("str w12, [x10, #32]");                                 // z_stream.avail_out = scratch window capacity
    emitter.instruction("mov x0, x10");                                         // arg 0 = z_stream pointer
    emitter.instruction("mov w1, #0");                                          // arg 1 = Z_NO_FLUSH (0)
    emitter.bl_c("deflate");                                                    // run one deflate step over the input window
                             // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the z_stream pointer after the deflate call
    emitter.instruction("ldr w12, [x10, #32]");                                 // reload avail_out left after this deflate step
    emitter.instruction(&format!("mov w13, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w13, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); // high half of the scratch window capacity
    emitter.instruction("sub w12, w13, w12");                                   // produced = capacity - avail_out
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("uxtw x2, w12");                                        // produced byte count as the write length
    emitter.syscall(4);
    // -- repeat while input remains OR the output window filled completely --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the z_stream pointer after the write
    emitter.instruction("ldr w14, [x10, #8]");                                  // reload avail_in still pending
    emitter.instruction(&format!("cbnz w14, {}_loop", fwrite_label));           // more input bytes: keep deflating
    emitter.instruction("ldr w12, [x10, #32]");                                 // reload avail_out left after this deflate step
    emitter.instruction(&format!("cbz w12, {}_loop", fwrite_label));            // window was filled: drain the remainder
                                                                     // -- done: return the original payload length --
    emitter.instruction("ldr x0, [sp, #8]");                                    // return value = the saved payload length
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bytes-consumed count

    // ================================================================
    // zlib deflate close helper.
    // Input:  x0 = fd. Flushes the deflate tail and ends the stream.
    // ================================================================
    emitter.label(close_label);
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]=fd [8]=z_stream [16]=ret code [32]=x29 [40]=x30
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_zstream_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // x10 = z_stream pointer for this descriptor
    emitter.instruction(&format!("cbz x10, {}_done", close_label));             // nothing to flush when no filter is attached
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor across deflate calls
    emitter.instruction("str x10, [sp, #8]");                                   // save the z_stream pointer
    emitter.instruction("str xzr, [x10, #0]");                                  // z_stream.next_in = NULL: no further input
    emitter.instruction("str wzr, [x10, #8]");                                  // z_stream.avail_in = 0: input is exhausted

    // -- flush loop: deflate with Z_FINISH until Z_STREAM_END --
    emitter.label(&format!("{}_loop", close_label));
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "x11", "_stream_filter_buf");
    emitter.instruction("str x11, [x10, #24]");                                 // z_stream.next_out = scratch window base
    emitter.instruction(&format!("mov w12, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w12, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); // high half of the scratch window capacity
    emitter.instruction("str w12, [x10, #32]");                                 // z_stream.avail_out = scratch window capacity
    emitter.instruction("mov x0, x10");                                         // arg 0 = z_stream pointer
    emitter.instruction("mov w1, #4");                                          // arg 1 = Z_FINISH (4)
    emitter.bl_c("deflate");                                                    // flush a chunk of the compressed tail
    emitter.instruction("str x0, [sp, #16]");                                   // save the deflate return code (1 = Z_STREAM_END)
                                              // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the z_stream pointer
    emitter.instruction("ldr w12, [x10, #32]");                                 // reload avail_out left after this flush step
    emitter.instruction(&format!("mov w13, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w13, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); // high half of the scratch window capacity
    emitter.instruction("sub w12, w13, w12");                                   // produced = capacity - avail_out
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("uxtw x2, w12");                                        // produced byte count as the write length
    emitter.syscall(4);
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload the saved deflate return code
    emitter.instruction("cmp x12, #1");                                         // did deflate report Z_STREAM_END?
    emitter.instruction(&format!("b.ne {}_loop", close_label));                 // not finished yet: flush another chunk

    // -- end the deflate stream and drop the per-descriptor handle --
    emitter.instruction("ldr x0, [sp, #8]");                                    // arg 0 = z_stream pointer
    emitter.bl_c("deflateEnd");                                                 // release zlib's internal deflate state
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    abi::emit_symbol_address(emitter, "x9", "_zstream_handles");
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear this descriptor's z_stream handle
    emitter.label(&format!("{}_done", close_label));
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the fclose path

    // ================================================================
    // Initialization: allocate and register a z_stream for this fd.
    // ================================================================
    emitter.label(skip_label);
    emitter.instruction("sub sp, sp, #16");                                     // frame: [0]=fd [8]=z_stream pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor across the calls
    emitter.instruction(&format!("mov x0, #{}", zstream.size));                 // request a target-layout z_stream-sized heap block
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the z_stream struct, x0 = payload
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned allocation
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the z_stream block as owned heap state
    emitter.instruction("str x0, [sp, #8]");                                    // save the z_stream pointer

    // -- zero the target-layout z_stream so zalloc/zfree are NULL --
    emitter.instruction("mov x9, #0");                                          // byte clear index
    emitter.label(&format!("{}_zero", skip_label));
    emitter.instruction(&format!("cmp x9, #{}", zstream.size));                 // cleared the whole target-layout z_stream struct?
    emitter.instruction(&format!("b.ge {}_zeroed", skip_label));                // the struct is fully zeroed
    emitter.instruction("strb wzr, [x0, x9]");                                  // zero one z_stream byte
    emitter.instruction("add x9, x9, #1");                                      // advance the clear index
    emitter.instruction(&format!("b {}_zero", skip_label));                     // continue zeroing the struct
    emitter.label(&format!("{}_zeroed", skip_label));

    // -- deflateInit2_(strm, level, Z_DEFLATED, -15, memLevel, strategy, ...) --
    // windowBits -15 selects raw deflate (no zlib header), matching PHP's
    // zlib.deflate stream filter.
    emitter.instruction("ldr x0, [sp, #8]");                                    // arg 0 = z_stream pointer
    emitter.instruction(&format!("mov x1, #{}", level));                        // arg 1 = compression level ($params, default Z_DEFAULT_COMPRESSION -1)
    emitter.instruction("mov x2, #8");                                          // arg 2 = Z_DEFLATED method
    emitter.instruction("mov x3, #-15");                                        // arg 3 = windowBits -15: raw deflate, no header
    emitter.instruction("mov x4, #8");                                          // arg 4 = default memLevel
    emitter.instruction("mov x5, #0");                                          // arg 5 = Z_DEFAULT_STRATEGY
    abi::emit_symbol_address(emitter, "x6", "_zlib_version");
    emitter.instruction(&format!("mov x7, #{}", zstream.size));                 // arg 7 = target sizeof(z_stream) for the ABI check
    emitter.bl_c("deflateInit2_");                                              // initialize a raw-deflate zlib stream

    // -- register the handle and mark the descriptor's write filter as zlib --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "x9", "_zstream_handles");
    emitter.instruction("str x10, [x9, x0, lsl #3]");                           // store the z_stream handle for this descriptor
    abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
    emitter.instruction("mov w11, #4");                                         // write-filter id 4 = zlib.deflate
    emitter.instruction("strb w11, [x9, x0]");                                  // record the zlib write filter for this descriptor

    // -- publish the helper addresses so __rt_fwrite / fclose can call them --
    abi::emit_symbol_address(emitter, "x11", fwrite_label);
    abi::emit_symbol_address(emitter, "x9", "_zlib_fwrite_fn");
    emitter.instruction("str x11, [x9]");                                       // _zlib_fwrite_fn = the deflate fwrite helper
    abi::emit_symbol_address(emitter, "x11", close_label);
    abi::emit_symbol_address(emitter, "x9", "_zlib_close_fn");
    emitter.instruction("str x11, [x9]");                                       // _zlib_close_fn = the deflate close helper

    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("add sp, sp, #16");                                     // release the initialization frame
    emitter.instruction("mov x1, x0");                                          // resource payload = the descriptor
    emitter.instruction("mov x2, #0");                                          // resource mixed payloads have no high word
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value"); // re-box the stream as the filter resource
}

/// Emits the x86_64 helpers, then the deflate-stream initialization.
pub(crate) fn emit_x86_64(
    emitter: &mut Emitter,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
    level: i64,
) {
    let zstream = z_stream_layout(emitter.target);
    // -- jump past the helper bodies so normal flow never falls into them --
    emitter.instruction(&format!("jmp {}", skip_label));                        // skip over the inline zlib helper routines

    // ================================================================
    // zlib deflate fwrite helper.
    // Input:  rdi = fd, rsi = payload pointer, rdx = payload length.
    // Output: rax = the input payload length (bytes "written").
    // ================================================================
    emitter.label(fwrite_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame: [-8]=fd [-16]=length [-24]=z_stream
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor for the write loop
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the payload length as the return value

    // -- load this descriptor's z_stream handle and seed the input window --
    emit_x86_stream_slot(emitter, "r11");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore payload length clobbered by the Windows slot registry
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov r10, QWORD PTR [r9 + r11*8]");                     // r10 = z_stream pointer for this compact/legacy descriptor slot
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the z_stream pointer
    emitter.instruction("mov QWORD PTR [r10 + 0], rsi");                        // z_stream.next_in = payload pointer
    emitter.instruction("mov DWORD PTR [r10 + 8], edx");                        // z_stream.avail_in = payload length

    // -- deflate loop: drain next_in into the scratch window and write it out --
    emitter.label(&format!("{}_loop", fwrite_label));
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "r11", "_stream_filter_buf"); // scratch window base
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", zstream.next_out)); // z_stream.next_out = scratch window base
    emitter.instruction(&format!("mov DWORD PTR [r10 + {}], {}", zstream.avail_out, FILTER_BUF_SIZE)); // z_stream.avail_out = scratch window capacity
    emitter.instruction("mov rdi, r10");                                        // arg 0 = z_stream pointer
    emitter.instruction("xor esi, esi");                                        // arg 1 = Z_NO_FLUSH (0)
    emitter.emit_call_c("deflate");                                             // run one deflate step over the input window
                                         // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the z_stream pointer
    emitter.instruction(&format!("mov eax, {}", FILTER_BUF_SIZE));              // scratch window capacity
    emitter.instruction(&format!("sub eax, DWORD PTR [r10 + {}]", zstream.avail_out)); // produced = capacity - avail_out
    emitter.instruction("mov edx, eax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf"); // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the compressed chunk through libc write()
                                       // -- repeat while input remains OR the output window filled completely --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the z_stream pointer
    emitter.instruction("cmp DWORD PTR [r10 + 8], 0");                          // any avail_in input bytes still pending?
    emitter.instruction(&format!("jne {}_loop", fwrite_label));                 // more input bytes: keep deflating
    emitter.instruction(&format!("cmp DWORD PTR [r10 + {}], 0", zstream.avail_out)); // did the output window fill completely?
    emitter.instruction(&format!("je {}_loop", fwrite_label));                  // window was filled: drain the remainder
                                                               // -- done: return the original payload length --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return value = the saved payload length
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bytes-consumed count

    // ================================================================
    // zlib deflate close helper.
    // Input:  rdi = fd. Flushes the deflate tail and ends the stream.
    // ================================================================
    emitter.label(close_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame: [-8]=fd [-16]=z_stream [-24]=ret code
    emit_x86_stream_slot(emitter, "r11");
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov r10, QWORD PTR [r9 + r11*8]");                     // r10 = z_stream pointer for this compact/legacy descriptor slot
    emitter.instruction("test r10, r10");                                       // is a deflate stream attached to this descriptor?
    emitter.instruction(&format!("jz {}_done", close_label));                   // nothing to flush when no filter is attached
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor across deflate calls
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the z_stream pointer
    emitter.instruction("mov QWORD PTR [r10 + 0], 0");                          // z_stream.next_in = NULL: no further input
    emitter.instruction("mov DWORD PTR [r10 + 8], 0");                          // z_stream.avail_in = 0: input is exhausted

    // -- flush loop: deflate with Z_FINISH until Z_STREAM_END --
    emitter.label(&format!("{}_loop", close_label));
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "r11", "_stream_filter_buf"); // scratch window base
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", zstream.next_out)); // z_stream.next_out = scratch window base
    emitter.instruction(&format!("mov DWORD PTR [r10 + {}], {}", zstream.avail_out, FILTER_BUF_SIZE)); // z_stream.avail_out = scratch window capacity
    emitter.instruction("mov rdi, r10");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, 4");                                          // arg 1 = Z_FINISH (4)
    emitter.emit_call_c("deflate");                                             // flush a chunk of the compressed tail
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the deflate return code (1 = Z_STREAM_END)
                                                          // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the z_stream pointer
    emitter.instruction(&format!("mov eax, {}", FILTER_BUF_SIZE));              // scratch window capacity
    emitter.instruction(&format!("sub eax, DWORD PTR [r10 + {}]", zstream.avail_out)); // produced = capacity - avail_out
    emitter.instruction("mov edx, eax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the preserved file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf"); // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the compressed tail chunk through libc write()
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // did deflate report Z_STREAM_END?
    emitter.instruction(&format!("jne {}_loop", close_label));                  // not finished yet: flush another chunk

    // -- end the deflate stream and drop the per-descriptor handle --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = z_stream pointer
    emitter.emit_call_c("deflateEnd");                                          // release zlib's internal deflate state
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emit_x86_stream_slot(emitter, "r11");
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov QWORD PTR [r9 + r11*8], 0");                       // clear this compact/legacy descriptor slot's z_stream handle
    emitter.label(&format!("{}_done", close_label));
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the fclose path

    // ================================================================
    // Initialization: allocate and register a z_stream for this fd.
    // ================================================================
    emitter.label(skip_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the initialization frame pointer
    emitter.instruction("sub rsp, 24");                                         // frame: [-8]=fd [-16]=z_stream ptr (24 keeps rsp 16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the file descriptor across the calls
    emitter.instruction(&format!("mov rax, {}", zstream.size));                 // request a target-layout z_stream-sized heap block
    emitter.instruction("call __rt_heap_alloc");                                // allocate the z_stream struct, rax = payload
    emitter.instruction(&format!(
        // owned-heap kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the z_stream block as owned heap state
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the z_stream pointer

    // -- zero the target-layout z_stream so zalloc/zfree are NULL --
    emitter.instruction("xor r9, r9");                                          // byte clear index
    emitter.label(&format!("{}_zero", skip_label));
    emitter.instruction(&format!("cmp r9, {}", zstream.size));                  // cleared the whole target-layout z_stream struct?
    emitter.instruction(&format!("jge {}_zeroed", skip_label));                 // the struct is fully zeroed
    emitter.instruction("mov BYTE PTR [rax + r9], 0");                          // zero one z_stream byte
    emitter.instruction("inc r9");                                              // advance the clear index
    emitter.instruction(&format!("jmp {}_zero", skip_label));                   // continue zeroing the struct
    emitter.label(&format!("{}_zeroed", skip_label));

    // -- deflateInit2_(strm, level, Z_DEFLATED, -15, memLevel, strategy, ...) --
    // windowBits -15 selects raw deflate (no zlib header), matching PHP's
    // zlib.deflate stream filter. The version/size args 7-8 go on the stack.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = z_stream pointer
    emitter.instruction(&format!("mov esi, {}", level));                        // arg 1 = compression level ($params, default Z_DEFAULT_COMPRESSION -1)
    emitter.instruction("mov edx, 8");                                          // arg 2 = Z_DEFLATED method
    emitter.instruction("mov ecx, -15");                                        // arg 3 = windowBits -15: raw deflate, no header
    emitter.instruction("mov r8d, 8");                                          // arg 4 = default memLevel
    emitter.instruction("xor r9d, r9d");                                        // arg 5 = Z_DEFAULT_STRATEGY
    emitter.instruction("sub rsp, 16");                                         // reserve the two stack arguments (kept 16-aligned)
    abi::emit_symbol_address(emitter, "rax", "_zlib_version"); // the zlib version string
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // stack arg 6 = version
    emitter.instruction(&format!("mov QWORD PTR [rsp + 8], {}", zstream.size)); // stack arg 7 = target sizeof(z_stream)
    emitter.emit_call_c("deflateInit2_");                                       // initialize a raw-deflate zlib stream
    emitter.instruction("add rsp, 16");                                         // release the stack-argument space

    // -- register the handle and mark the descriptor's write filter as zlib --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emit_x86_stream_slot(emitter, "r11");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the z_stream pointer after the Windows slot lookup
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov QWORD PTR [r9 + r11*8], r10");                     // store the z_stream handle for this compact/legacy descriptor slot
    abi::emit_symbol_address(emitter, "r9", "_stream_write_filters"); // write-filter table base
    emitter.instruction("mov BYTE PTR [r9 + r11], 4");                          // write-filter id 4 = zlib.deflate

    // -- publish the helper addresses so __rt_fwrite / fclose can call them --
    emitter.instruction(&format!("lea r10, [rip + {}]", fwrite_label));         // address of the deflate fwrite helper
    abi::emit_symbol_address(emitter, "r9", "_zlib_fwrite_fn"); // _zlib_fwrite_fn slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // _zlib_fwrite_fn = the deflate fwrite helper
    emitter.instruction(&format!("lea r10, [rip + {}]", close_label));          // address of the deflate close helper
    abi::emit_symbol_address(emitter, "r9", "_zlib_close_fn"); // _zlib_close_fn slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // _zlib_close_fn = the deflate close helper

    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // resource payload = the descriptor
    emitter.instruction("add rsp, 24");                                         // release the initialization frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("xor esi, esi");                                        // resource mixed payloads have no high word
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value"); // re-box the stream as the filter resource
}

/// Emits the x86_64 slot index used by per-stream filter state tables.
///
/// Windows descriptors may be opaque SOCKET values; only Windows maps them
/// through the bounded runtime registry. Linux retains its established direct
/// descriptor indexing in the generated assembly.
fn emit_x86_stream_slot(emitter: &mut Emitter, slot_reg: &str) {
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("call __rt_win_stream_slot");                       // map raw Windows descriptor to a bounded stream-state slot
        emitter.instruction(&format!("mov {}, rax", slot_reg));                 // retain compact slot for the following table access
    } else {
        emitter.instruction(&format!("mov {}, rdi", slot_reg));                 // Linux keeps the established descriptor table index
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Regression tests for target-specific zlib `z_stream` C layouts.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Windows uses LLP64, while macOS/Linux use LP64.

    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::{emit_x86_64, z_stream_layout, ZStreamLayout};
    use crate::codegen_support::emit::Emitter;

    /// Verifies MinGW's LLP64 layout uses 32-bit `unsigned long` fields.
    #[test]
    fn windows_z_stream_layout_matches_mingw_zlib() {
        assert_eq!(
            z_stream_layout(Target::new(Platform::Windows, Arch::X86_64)),
            ZStreamLayout {
                size: 88,
                next_out: 16,
                avail_out: 24,
                total_out: 28,
            }
        );
    }

    /// Verifies Linux keeps zlib's established LP64 layout.
    #[test]
    fn linux_z_stream_layout_remains_lp64() {
        assert_eq!(
            z_stream_layout(Target::new(Platform::Linux, Arch::X86_64)),
            ZStreamLayout {
                size: 112,
                next_out: 24,
                avail_out: 32,
                total_out: 40,
            }
        );
    }

    /// Verifies the Windows deflate write helper restores its saved payload
    /// length after compact-slot lookup, whose registry uses `rdx` as scratch.
    #[test]
    fn windows_deflate_write_restores_length_after_stream_slot_lookup() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_x86_64(&mut emitter, "deflate_write", "deflate_close", "after_deflate", -1);
        let asm = emitter.output();
        let slot_lookup = asm
            .find("call __rt_win_stream_slot")
            .expect("Windows deflate helper must use a compact stream slot");
        let restore = asm[slot_lookup..]
            .find("mov rdx, QWORD PTR [rbp - 16]")
            .map(|offset| slot_lookup + offset)
            .expect("slot lookup must not leak its rdx scratch value into z_stream.avail_in");
        let seed = asm[restore..]
            .find("mov DWORD PTR [r10 + 8], edx")
            .map(|offset| restore + offset)
            .expect("deflate helper must seed avail_in from the restored length");
        assert!(slot_lookup < restore && restore < seed);
    }

    /// Verifies Windows reloads the allocated zlib stream after compact-slot
    /// lookup before publishing the per-descriptor handle.
    #[test]
    fn windows_deflate_attach_restores_handle_after_stream_slot_lookup() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_x86_64(
            &mut emitter,
            "deflate_write",
            "deflate_close",
            "after_deflate",
            -1,
        );
        let asm = emitter.output();
        let slot_lookup = asm
            .rfind("call __rt_win_stream_slot")
            .expect("Windows deflate attach must use a compact stream slot");
        let restore = asm[slot_lookup..]
            .find("mov r10, QWORD PTR [rbp - 16]")
            .map(|offset| slot_lookup + offset)
            .expect("slot lookup must not replace the allocated zlib stream");
        let store = asm[restore..]
            .find("mov QWORD PTR [r9 + r11*8], r10")
            .map(|offset| restore + offset)
            .expect("deflate attach must publish the restored stream handle");
        assert!(slot_lookup < restore && restore < store);
    }
}
