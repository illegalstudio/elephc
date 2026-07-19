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
//! - The `z_stream` struct is LP64-sized (112 bytes); zeroing it leaves
//!   `zalloc`/`zfree` NULL so zlib uses its own allocator. The struct itself is
//!   intentionally not freed on close — a small, documented v1 leak.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// Size of the libz `z_stream` struct on LP64 targets, in bytes.
const Z_STREAM_SIZE: i64 = 112;
/// Capacity of the shared `_stream_filter_buf` scratch used as the deflate
/// output window.
const FILTER_BUF_SIZE: i64 = 65536;
/// x86_64 owned-heap kind word: the elephc heap marker in the high 32 bits.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the ARM64 helpers, then the deflate-stream initialization.
pub(crate) fn emit_arm64(
    emitter: &mut Emitter,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
    level: i64,
) {
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
    emitter.bl_c("deflate"); // run one deflate step over the input window
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
    emitter.bl_c("deflate"); // flush a chunk of the compressed tail
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
    emitter.bl_c("deflateEnd"); // release zlib's internal deflate state
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
    emitter.instruction(&format!("mov x0, #{}", Z_STREAM_SIZE));                // request a z_stream-sized heap block
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the z_stream struct, x0 = payload
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned allocation
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the z_stream block as owned heap state
    emitter.instruction("str x0, [sp, #8]");                                    // save the z_stream pointer

    // -- zero all 112 bytes so zalloc/zfree are NULL and counters start clean --
    emitter.instruction("mov x9, #0");                                          // byte clear index
    emitter.label(&format!("{}_zero", skip_label));
    emitter.instruction(&format!("cmp x9, #{}", Z_STREAM_SIZE));                // cleared the whole z_stream struct?
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
    emitter.instruction(&format!("mov x7, #{}", Z_STREAM_SIZE));                // arg 7 = sizeof(z_stream) for the ABI check
    emitter.bl_c("deflateInit2_"); // initialize a raw-deflate zlib stream

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
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi*8]");                     // r10 = z_stream pointer for this descriptor
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the z_stream pointer
    emitter.instruction("mov QWORD PTR [r10 + 0], rsi");                        // z_stream.next_in = payload pointer
    emitter.instruction("mov DWORD PTR [r10 + 8], edx");                        // z_stream.avail_in = payload length

    // -- deflate loop: drain next_in into the scratch window and write it out --
    emitter.label(&format!("{}_loop", fwrite_label));
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "r11", "_stream_filter_buf"); // scratch window base
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // z_stream.next_out = scratch window base
    emitter.instruction(&format!("mov DWORD PTR [r10 + 32], {}", FILTER_BUF_SIZE)); // z_stream.avail_out = scratch window capacity
    emitter.instruction("mov rdi, r10");                                        // arg 0 = z_stream pointer
    emitter.instruction("xor esi, esi");                                        // arg 1 = Z_NO_FLUSH (0)
    emitter.instruction("call deflate");                                        // run one deflate step over the input window
                                         // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the z_stream pointer
    emitter.instruction(&format!("mov eax, {}", FILTER_BUF_SIZE));              // scratch window capacity
    emitter.instruction("sub eax, DWORD PTR [r10 + 32]");                       // produced = capacity - avail_out
    emitter.instruction("mov edx, eax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf"); // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the compressed chunk through libc write()
                                       // -- repeat while input remains OR the output window filled completely --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the z_stream pointer
    emitter.instruction("cmp DWORD PTR [r10 + 8], 0");                          // any avail_in input bytes still pending?
    emitter.instruction(&format!("jne {}_loop", fwrite_label));                 // more input bytes: keep deflating
    emitter.instruction("cmp DWORD PTR [r10 + 32], 0");                         // did the output window fill completely?
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
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi*8]");                     // r10 = z_stream pointer for this descriptor
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
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // z_stream.next_out = scratch window base
    emitter.instruction(&format!("mov DWORD PTR [r10 + 32], {}", FILTER_BUF_SIZE)); // z_stream.avail_out = scratch window capacity
    emitter.instruction("mov rdi, r10");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, 4");                                          // arg 1 = Z_FINISH (4)
    emitter.instruction("call deflate");                                        // flush a chunk of the compressed tail
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the deflate return code (1 = Z_STREAM_END)
                                                          // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the z_stream pointer
    emitter.instruction(&format!("mov eax, {}", FILTER_BUF_SIZE));              // scratch window capacity
    emitter.instruction("sub eax, DWORD PTR [r10 + 32]");                       // produced = capacity - avail_out
    emitter.instruction("mov edx, eax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the preserved file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf"); // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the compressed tail chunk through libc write()
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // did deflate report Z_STREAM_END?
    emitter.instruction(&format!("jne {}_loop", close_label));                  // not finished yet: flush another chunk

    // -- end the deflate stream and drop the per-descriptor handle --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = z_stream pointer
    emitter.instruction("call deflateEnd");                                     // release zlib's internal deflate state
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov QWORD PTR [r9 + rdi*8], 0");                       // clear this descriptor's z_stream handle
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
    emitter.instruction(&format!("mov rax, {}", Z_STREAM_SIZE));                // request a z_stream-sized heap block
    emitter.instruction("call __rt_heap_alloc");                                // allocate the z_stream struct, rax = payload
    emitter.instruction(&format!(
        // owned-heap kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the z_stream block as owned heap state
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the z_stream pointer

    // -- zero all 112 bytes so zalloc/zfree are NULL and counters start clean --
    emitter.instruction("xor r9, r9");                                          // byte clear index
    emitter.label(&format!("{}_zero", skip_label));
    emitter.instruction(&format!("cmp r9, {}", Z_STREAM_SIZE));                 // cleared the whole z_stream struct?
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
    emitter.instruction(&format!("mov QWORD PTR [rsp + 8], {}", Z_STREAM_SIZE)); // stack arg 7 = sizeof(z_stream)
    emitter.instruction("call deflateInit2_");                                  // initialize a raw-deflate zlib stream
    emitter.instruction("add rsp, 16");                                         // release the stack-argument space

    // -- register the handle and mark the descriptor's write filter as zlib --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the z_stream pointer
    abi::emit_symbol_address(emitter, "r9", "_zstream_handles"); // z_stream handle table base
    emitter.instruction("mov QWORD PTR [r9 + rdi*8], r10");                     // store the z_stream handle for this descriptor
    abi::emit_symbol_address(emitter, "r9", "_stream_write_filters"); // write-filter table base
    emitter.instruction("mov BYTE PTR [r9 + rdi], 4");                          // write-filter id 4 = zlib.deflate

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
