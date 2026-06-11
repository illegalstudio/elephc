//! Purpose:
//! Emits the WRITE-direction `convert.iconv.<from>/<to>` stream filter: a
//! streaming per-`fwrite` transcoder installed when `stream_filter_append` is
//! called with `STREAM_FILTER_WRITE`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::stream_filter_iconv::emit()` when the
//!   runtime mode is `STREAM_FILTER_WRITE` (2). READ / ALL keep the attach-time
//!   read transform in `stream_filter_iconv`.
//!
//! Key details:
//! - Mirrors the `zlib.deflate` / `bzip2.compress` write-filter mechanism: a
//!   per-fd handle (`_iconv_handles[fd]` holds the `iconv_t`), write-filter id
//!   12 in `_stream_write_filters[fd]`, and two per-program USER-asm helpers
//!   whose addresses are published into `_iconv_fwrite_fn` / `_iconv_close_fn`.
//!   The shared runtime (`__rt_fwrite`, `fclose`) reaches libc `iconv` ONLY
//!   through those pointers, so it never names an iconv symbol — keeping the
//!   macOS `-liconv` dependency to programs that actually attach the filter.
//! - The fwrite helper loops `iconv(cd, &in, &inleft, &out, &outleft)` into the
//!   shared `_stream_grow_scratch` (64 KiB) window, writing each produced chunk
//!   to the fd, until the input is drained. The `iconv_t` is persistent, so
//!   shift state carries across writes. v1: a write that ends mid-multibyte
//!   sequence (no progress, output empty) stops to avoid a spin — acceptable
//!   for whole-string writes of complete text.
//! - The close helper `iconv_close`s the descriptor and clears the handle.
//! - The init block is spliced INLINE (entered with rsp 16-aligned), so on
//!   x86_64 it reserves `sub rsp, 24` (push rbp + 24 ≡ 0 mod 16) to keep rsp
//!   16-aligned at the libc calls; the call-entered helpers use `sub rsp, 64`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Capacity of the shared `_stream_grow_scratch` window used as the iconv output buffer.
const ICONV_SCRATCH: i64 = 65536;

/// Emits the iconv WRITE-filter attachment. The descriptor is in the int result
/// register at entry; on return the stream is re-boxed as a resource Mixed cell.
pub fn emit_iconv_write_attach(emitter: &mut Emitter, ctx: &mut Context, from_sym: &str, to_sym: &str) {
    let fwrite_label = ctx.next_label("iconv_w_fwrite");
    let close_label = ctx.next_label("iconv_w_close");
    let skip_label = ctx.next_label("iconv_w_skip_helpers");
    match emitter.target.arch {
        Arch::AArch64 => emit_arm64(emitter, ctx, from_sym, to_sym, &fwrite_label, &close_label, &skip_label),
        Arch::X86_64 => emit_x86_64(emitter, ctx, from_sym, to_sym, &fwrite_label, &close_label, &skip_label),
    }
}

/// ARM64 helpers + inline init.
fn emit_arm64(
    emitter: &mut Emitter,
    ctx: &mut Context,
    from_sym: &str,
    to_sym: &str,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
) {
    let loop_label = ctx.next_label("iconv_w_loop");
    let after_write = ctx.next_label("iconv_w_after_write");
    let done_label = ctx.next_label("iconv_w_done");
    let skip_store = ctx.next_label("iconv_w_skip_store");

    emitter.instruction(&format!("b {}", skip_label));                          // skip over the inline iconv helper routines

    // ================================================================
    // iconv write helper. Input: x0 = fd, x1 = payload ptr, x2 = payload len.
    // Output: x0 = the input payload length (bytes "written").
    // Frame: [0]=fd [8]=retlen [16]=inbuf [24]=inleft [32]=outbuf [40]=outleft.
    // ================================================================
    emitter.label(fwrite_label);
    emitter.instruction("sub sp, sp, #64");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor
    emitter.instruction("str x2, [sp, #8]");                                    // save the payload length as the return value
    emitter.instruction("str x1, [sp, #16]");                                   // iconv inbuf = payload pointer
    emitter.instruction("str x2, [sp, #24]");                                   // iconv inbytesleft = payload length

    emitter.label(&loop_label);
    abi::emit_symbol_address(emitter, "x9", "_stream_grow_scratch");
    emitter.instruction("str x9, [sp, #32]");                                   // iconv outbuf = scratch window base
    emitter.instruction(&format!("mov w10, #{}", ICONV_SCRATCH & 0xFFFF));      // low half of the scratch capacity
    emitter.instruction(&format!("movk w10, #{}, lsl #16", ICONV_SCRATCH >> 16)); // high half of the scratch capacity
    emitter.instruction("str x10, [sp, #40]");                                  // iconv outbytesleft = scratch capacity
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the descriptor to index the handle table
    abi::emit_symbol_address(emitter, "x9", "_iconv_handles");
    emitter.instruction("ldr x0, [x9, x0, lsl #3]");                            // arg 0 = the iconv_t for this descriptor
    emitter.instruction("add x1, sp, #16");                                     // arg 1 = &inbuf
    emitter.instruction("add x2, sp, #24");                                     // arg 2 = &inbytesleft
    emitter.instruction("add x3, sp, #32");                                     // arg 3 = &outbuf
    emitter.instruction("add x4, sp, #40");                                     // arg 4 = &outbytesleft
    emitter.bl_c("iconv");                                                      // transcode a chunk of the payload
    // produced = scratch capacity - remaining outbytesleft
    emitter.instruction(&format!("mov w10, #{}", ICONV_SCRATCH & 0xFFFF));      // low half of the scratch capacity
    emitter.instruction(&format!("movk w10, #{}, lsl #16", ICONV_SCRATCH >> 16)); // high half of the scratch capacity
    emitter.instruction("ldr x11, [sp, #40]");                                  // remaining outbytesleft after iconv
    emitter.instruction("sub x12, x10, x11");                                   // produced = capacity - remaining
    emitter.instruction(&format!("cbz x12, {}", after_write));                  // nothing produced: skip the write
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd = the saved descriptor
    abi::emit_symbol_address(emitter, "x1", "_stream_grow_scratch");
    emitter.instruction("mov x2, x12");                                         // produced byte count as the write length
    emitter.syscall(4);
    emitter.label(&after_write);
    emitter.instruction("ldr x11, [sp, #24]");                                  // remaining inbytesleft
    emitter.instruction(&format!("cbz x11, {}", done_label));                   // all input consumed: done
    emitter.instruction(&format!("cbz x12, {}", done_label));                   // no progress (incomplete/invalid): stop to avoid a spin
    emitter.instruction(&format!("b {}", loop_label));                          // output filled: transcode the remainder
    emitter.label(&done_label);
    emitter.instruction("ldr x0, [sp, #8]");                                    // return value = the saved payload length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bytes-consumed count

    // ================================================================
    // iconv close helper. Input: x0 = fd. Closes the iconv descriptor.
    // ================================================================
    emitter.label(close_label);
    emitter.instruction("sub sp, sp, #16");                                     // helper frame: [0]=fd [8]=x30
    emitter.instruction("str x30, [sp, #8]");                                   // save the return address
    abi::emit_symbol_address(emitter, "x9", "_iconv_handles");
    emitter.instruction("ldr x1, [x9, x0, lsl #3]");                            // load this descriptor's iconv_t
    emitter.instruction(&format!("cbz x1, {}_done", close_label));              // nothing attached: nothing to close
    emitter.instruction("str x0, [sp, #0]");                                    // save the descriptor across iconv_close
    emitter.instruction("mov x0, x1");                                          // arg 0 = the iconv_t
    emitter.bl_c("iconv_close");                                                // release the iconv descriptor
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the descriptor
    abi::emit_symbol_address(emitter, "x9", "_iconv_handles");
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear this descriptor's iconv handle
    emitter.label(&format!("{}_done", close_label));
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the fclose path

    // ================================================================
    // Initialization (inline): iconv_open + register the handle for this fd.
    // ================================================================
    emitter.label(skip_label);
    emitter.instruction("sub sp, sp, #16");                                     // frame: [0]=fd [8]=x30
    emitter.instruction("str x30, [sp, #8]");                                   // save the return address across iconv_open
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor
    abi::emit_symbol_address(emitter, "x0", to_sym);                            // arg 0 = tocode
    abi::emit_symbol_address(emitter, "x1", from_sym);                          // arg 1 = fromcode
    emitter.bl_c("iconv_open");                                                 // open the charset conversion descriptor
    emitter.instruction("cmn x0, #1");                                          // is the descriptor (iconv_t)-1?
    emitter.instruction(&format!("b.eq {}", skip_store));                       // iconv_open failed → attach no filter
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the file descriptor
    abi::emit_symbol_address(emitter, "x9", "_iconv_handles");
    emitter.instruction("str x0, [x9, x1, lsl #3]");                            // store the iconv_t for this descriptor
    abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
    emitter.instruction("mov w10, #12");                                        // write-filter id 12 = convert.iconv write
    emitter.instruction("strb w10, [x9, x1]");                                  // record the iconv write filter for this descriptor
    abi::emit_symbol_address(emitter, "x10", fwrite_label);
    abi::emit_symbol_address(emitter, "x9", "_iconv_fwrite_fn");
    emitter.instruction("str x10, [x9]");                                       // _iconv_fwrite_fn = the iconv write helper
    abi::emit_symbol_address(emitter, "x10", close_label);
    abi::emit_symbol_address(emitter, "x9", "_iconv_close_fn");
    emitter.instruction("str x10, [x9]");                                       // _iconv_close_fn = the iconv close helper
    emitter.label(&skip_store);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the return address
    emitter.instruction("add sp, sp, #16");                                     // release the initialization frame
    emitter.instruction("mov x1, x0");                                          // resource payload = the descriptor
    emitter.instruction("mov x2, #0");                                          // resource mixed payloads have no high word
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}

/// x86_64 helpers + inline init.
fn emit_x86_64(
    emitter: &mut Emitter,
    ctx: &mut Context,
    from_sym: &str,
    to_sym: &str,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
) {
    let loop_label = ctx.next_label("iconv_w_loop");
    let after_write = ctx.next_label("iconv_w_after_write");
    let done_label = ctx.next_label("iconv_w_done");
    let skip_store = ctx.next_label("iconv_w_skip_store");

    emitter.instruction(&format!("jmp {}", skip_label));                        // skip over the inline iconv helper routines

    // ================================================================
    // iconv write helper. Input: rdi = fd, rsi = payload ptr, rdx = payload len.
    // Output: rax = the input payload length (bytes "written").
    // Frame: [-8]=fd [-16]=retlen [-24]=inbuf [-32]=inleft [-40]=outbuf [-48]=outleft.
    // ================================================================
    emitter.label(fwrite_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // helper frame (0 mod 16: aligned at the libc calls)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the payload length as the return value
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // iconv inbuf = payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // iconv inbytesleft = payload length

    emitter.label(&loop_label);
    abi::emit_symbol_address(emitter, "r9", "_stream_grow_scratch");            // scratch window base
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // iconv outbuf = scratch window base
    emitter.instruction(&format!("mov QWORD PTR [rbp - 48], {}", ICONV_SCRATCH)); // iconv outbytesleft = scratch capacity
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the descriptor to index the handle table
    abi::emit_symbol_address(emitter, "r9", "_iconv_handles");                  // iconv handle table base
    emitter.instruction("mov rdi, QWORD PTR [r9 + rdi*8]");                     // arg 0 = the iconv_t for this descriptor
    emitter.instruction("lea rsi, [rbp - 24]");                                 // arg 1 = &inbuf
    emitter.instruction("lea rdx, [rbp - 32]");                                 // arg 2 = &inbytesleft
    emitter.instruction("lea rcx, [rbp - 40]");                                 // arg 3 = &outbuf
    emitter.instruction("lea r8, [rbp - 48]");                                  // arg 4 = &outbytesleft
    emitter.instruction("call iconv");                                          // transcode a chunk of the payload
    // produced = scratch capacity - remaining outbytesleft
    emitter.instruction(&format!("mov rax, {}", ICONV_SCRATCH));                // scratch capacity
    emitter.instruction("sub rax, QWORD PTR [rbp - 48]");                       // produced = capacity - remaining
    emitter.instruction("test rax, rax");                                       // anything produced this pass?
    emitter.instruction(&format!("jz {}", after_write));                        // nothing produced: skip the write
    emitter.instruction("mov rdx, rax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the saved descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_grow_scratch");           // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the transcoded chunk through libc write()
    emitter.label(&after_write);
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // remaining inbytesleft?
    emitter.instruction(&format!("je {}", done_label));                         // all input consumed: done
    emitter.instruction(&format!("mov rax, {}", ICONV_SCRATCH));                // recompute produced to test for progress
    emitter.instruction("sub rax, QWORD PTR [rbp - 48]");                       // produced this pass
    emitter.instruction(&format!("jz {}", done_label));                         // no progress (incomplete/invalid): stop to avoid a spin
    emitter.instruction(&format!("jmp {}", loop_label));                        // output filled: transcode the remainder
    emitter.label(&done_label);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return value = the saved payload length
    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bytes-consumed count

    // ================================================================
    // iconv close helper. Input: rdi = fd. Closes the iconv descriptor.
    // ================================================================
    emitter.label(close_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // helper frame: [-8]=fd
    abi::emit_symbol_address(emitter, "r9", "_iconv_handles");                  // iconv handle table base
    emitter.instruction("mov rsi, QWORD PTR [r9 + rdi*8]");                     // load this descriptor's iconv_t
    emitter.instruction("test rsi, rsi");                                       // anything attached?
    emitter.instruction(&format!("jz {}_done", close_label));                   // nothing attached: nothing to close
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the descriptor across iconv_close
    emitter.instruction("mov rdi, rsi");                                        // arg 0 = the iconv_t
    emitter.instruction("call iconv_close");                                    // release the iconv descriptor
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the descriptor
    abi::emit_symbol_address(emitter, "r9", "_iconv_handles");                  // iconv handle table base
    emitter.instruction("mov QWORD PTR [r9 + rdi*8], 0");                       // clear this descriptor's iconv handle
    emitter.label(&format!("{}_done", close_label));
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the fclose path

    // ================================================================
    // Initialization (inline): iconv_open + register the handle for this fd.
    // ================================================================
    emitter.label(skip_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the initialization frame pointer
    emitter.instruction("sub rsp, 24");                                         // frame: [-8]=fd (24: inline entry rsp 16-aligned, push rbp made it 8, +24≡8 mod 16 realigns to 0 at the libc calls)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the file descriptor
    abi::emit_symbol_address(emitter, "rdi", &to_sym);                          // arg 0 = tocode
    abi::emit_symbol_address(emitter, "rsi", &from_sym);                        // arg 1 = fromcode
    emitter.instruction("call iconv_open");                                     // open the charset conversion descriptor
    emitter.instruction("cmp rax, -1");                                         // is the descriptor (iconv_t)-1?
    emitter.instruction(&format!("je {}", skip_store));                         // iconv_open failed → attach no filter
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    abi::emit_symbol_address(emitter, "r9", "_iconv_handles");                  // iconv handle table base
    emitter.instruction("mov QWORD PTR [r9 + rdi*8], rax");                     // store the iconv_t for this descriptor
    abi::emit_symbol_address(emitter, "r9", "_stream_write_filters");           // write-filter table base
    emitter.instruction("mov BYTE PTR [r9 + rdi], 12");                         // write-filter id 12 = convert.iconv write
    emitter.instruction(&format!("lea r10, [rip + {}]", fwrite_label));         // address of the iconv write helper
    abi::emit_symbol_address(emitter, "r9", "_iconv_fwrite_fn");                // _iconv_fwrite_fn slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // _iconv_fwrite_fn = the iconv write helper
    emitter.instruction(&format!("lea r10, [rip + {}]", close_label));          // address of the iconv close helper
    abi::emit_symbol_address(emitter, "r9", "_iconv_close_fn");                 // _iconv_close_fn slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // _iconv_close_fn = the iconv close helper
    emitter.label(&skip_store);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // resource payload = the descriptor
    emitter.instruction("add rsp, 24");                                         // release the initialization frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("xor esi, esi");                                        // resource mixed payloads have no high word
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}
