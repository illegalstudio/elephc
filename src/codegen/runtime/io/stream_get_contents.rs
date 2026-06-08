//! Purpose:
//! Emits the `__rt_stream_get_contents` runtime helper assembly for stream_get_contents.
//! Reads every remaining byte from a stream through the same fread path used by
//! bounded reads.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The result string is a borrowed slice of `_concat_buf`, matching `__rt_fread`.
//! - The read-all loop uses `__rt_fread` so TLS sessions, filters, and wrapper
//!   reads share one I/O dispatch path.

use crate::codegen::abi::emit_symbol_address;
use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the read-all stream helper.
///
/// Input: `x0 = fd`. Output: `x1 = string pointer`, `x2 = total bytes read`.
/// The helper loops through `__rt_fread`, compacts each returned chunk into
/// `_concat_buf`, and stops when EOF or an empty read is produced.
pub fn emit_stream_get_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_contents_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate locals plus saved frame pointer and return address
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source file descriptor

    // -- record the start of the result inside the concat buffer --
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emitter.instruction("str x10, [sp, #8]");                                   // save the result start offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the result start pointer
    emitter.instruction("str x12, [sp, #16]");                                  // save the result start pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the running byte total to zero

    // -- read 4096-byte chunks through fread until EOF --
    emitter.label("__rt_stream_get_contents_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source file descriptor
    emitter.instruction("mov w11, #0x4000");                                    // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w11, w11, #16");                                   // form 0x40000000
    emitter.instruction("cmp x0, x11");                                         // synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_stream_get_contents_after_feof");            // normal fd: skip wrapper EOF dispatch
    emitter.instruction("bl __rt_feof");                                        // wrapper: check stream_eof before reading
    emitter.instruction("cbnz x0, __rt_stream_get_contents_done");              // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_after_feof");
    emitter.instruction("ldr x9, [sp, #24]");                                   // running result length
    emitter.instruction("ldr x12, [sp, #8]");                                   // result start offset
    emitter.instruction("add x12, x12, x9");                                    // compact append offset = start + total
    emit_symbol_address(emitter, "x13", "_concat_off");
    emitter.instruction("str x12, [x13]");                                      // make __rt_fread append at the compact tail
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for __rt_fread
    emitter.instruction("mov x1, #4096");                                       // request one read-all chunk
    emitter.instruction("bl __rt_fread");                                       // x1=chunk ptr, x2=chunk len
    emitter.instruction("cbz x2, __rt_stream_get_contents_release_done");       // empty read stops the read-all loop
    emitter.instruction("str x1, [sp, #32]");                                   // save chunk pointer across the copy
    emitter.instruction("str x2, [sp, #40]");                                   // save chunk length across the copy
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload running result length after __rt_fread
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("ldr x12, [sp, #8]");                                   // result start offset
    emitter.instruction("add x11, x11, x12");                                   // result base pointer
    emitter.instruction("add x11, x11, x9");                                    // destination = result base + total
    emitter.instruction("mov x12, #0");                                         // byte-copy index
    emitter.label("__rt_stream_get_contents_copy");
    emitter.instruction("cmp x12, x2");                                         // copied this whole chunk?
    emitter.instruction("b.ge __rt_stream_get_contents_copy_done");             // leave the copy loop once chunk bytes are copied
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load the next chunk byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it at the compact destination
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_stream_get_contents_copy");                     // copy the next byte
    emitter.label("__rt_stream_get_contents_copy_done");
    emitter.instruction("ldr x9, [sp, #24]");                                   // running result length before this chunk
    emitter.instruction("ldr x10, [sp, #40]");                                  // copied chunk length
    emitter.instruction("add x9, x9, x10");                                     // include the copied chunk in the total
    emitter.instruction("str x9, [sp, #24]");                                   // store the updated result length
    emitter.instruction("ldr x12, [sp, #8]");                                   // result start offset
    emitter.instruction("add x12, x12, x9");                                    // compact tail offset after this chunk
    emit_symbol_address(emitter, "x13", "_concat_off");
    emitter.instruction("str x12, [x13]");                                      // publish the compacted concat-buffer tail
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("b __rt_stream_get_contents_loop");                     // read the next chunk

    // -- release the terminal empty chunk and return the accumulated string --
    emitter.label("__rt_stream_get_contents_release_done");
    emitter.instruction("mov x0, x1");                                          // final empty chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release it if it is heap-backed
    emitter.label("__rt_stream_get_contents_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the result start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the accumulated result length
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the accumulated string slice

    emit_stream_get_contents_bounded_aarch64(emitter);
}

/// Emits the AArch64 bounded stream_get_contents helper.
///
/// Input: `x0 = fd`, `x1 = max bytes`. Output: `x1 = ptr`, `x2 = len`.
/// The loop calls `__rt_fread` repeatedly, compacts each returned chunk into
/// `_concat_buf`, and stops at the requested byte count or EOF.
fn emit_stream_get_contents_bounded_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents_bounded ---");
    emitter.label_global("__rt_stream_get_contents_bounded");

    emitter.instruction("sub sp, sp, #80");                                     // allocate locals plus saved frame pointer and return address
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested byte cap
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot the concat-buffer start offset
    emitter.instruction("str x10, [sp, #16]");                                  // save the result start offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the result start pointer
    emitter.instruction("str x12, [sp, #24]");                                  // save the result start pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // running result length = 0

    emitter.label("__rt_stream_get_contents_bounded_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction("ldr x10, [sp, #8]");                                   // requested byte cap
    emitter.instruction("cmp x9, x10");                                         // has the result reached the requested cap?
    emitter.instruction("b.ge __rt_stream_get_contents_bounded_done");          // stop once the cap is filled
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source descriptor
    emitter.instruction("mov w11, #0x4000");                                    // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w11, w11, #16");                                   // form 0x40000000
    emitter.instruction("cmp x0, x11");                                         // synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_stream_get_contents_bounded_after_feof");    // normal fd: skip wrapper EOF dispatch
    emitter.instruction("bl __rt_feof");                                        // wrapper: check stream_eof before reading
    emitter.instruction("cbnz x0, __rt_stream_get_contents_bounded_done");      // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_bounded_after_feof");
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction("ldr x10, [sp, #8]");                                   // requested byte cap
    emitter.instruction("sub x1, x10, x9");                                     // remaining bytes needed
    emitter.instruction("mov x11, #4096");                                      // maximum chunk request
    emitter.instruction("cmp x1, x11");                                         // is the remaining cap smaller than the chunk size?
    emitter.instruction("csel x1, x1, x11, lt");                                // request min(remaining, 4096)
    emitter.instruction("ldr x12, [sp, #16]");                                  // result start offset
    emitter.instruction("add x12, x12, x9");                                    // compact append offset = start + total
    emit_symbol_address(emitter, "x13", "_concat_off");
    emitter.instruction("str x12, [x13]");                                      // make __rt_fread append at the compact tail
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for __rt_fread
    emitter.instruction("bl __rt_fread");                                       // x1=chunk ptr, x2=chunk len
    emitter.instruction("cbz x2, __rt_stream_get_contents_bounded_release_done"); // empty read stops the bounded loop
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction("ldr x10, [sp, #8]");                                   // requested byte cap
    emitter.instruction("sub x10, x10, x9");                                    // remaining bytes allowed
    emitter.instruction("cmp x2, x10");                                         // did the source return more than requested?
    emitter.instruction("csel x2, x2, x10, ls");                                // clamp the chunk to the remaining cap
    emitter.instruction("str x1, [sp, #40]");                                   // save chunk pointer across the copy
    emitter.instruction("str x2, [sp, #48]");                                   // save chunk length across the copy
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("ldr x12, [sp, #16]");                                  // result start offset
    emitter.instruction("add x11, x11, x12");                                   // result base pointer
    emitter.instruction("add x11, x11, x9");                                    // destination = result base + total
    emitter.instruction("mov x12, #0");                                         // byte-copy index
    emitter.label("__rt_stream_get_contents_bounded_copy");
    emitter.instruction("cmp x12, x2");                                         // copied this whole chunk?
    emitter.instruction("b.ge __rt_stream_get_contents_bounded_copy_done");     // leave the copy loop once chunk bytes are copied
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load the next chunk byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it at the compact destination
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_stream_get_contents_bounded_copy");             // copy the next byte
    emitter.label("__rt_stream_get_contents_bounded_copy_done");
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length before this chunk
    emitter.instruction("ldr x10, [sp, #48]");                                  // copied chunk length
    emitter.instruction("add x9, x9, x10");                                     // include the copied chunk in the total
    emitter.instruction("str x9, [sp, #32]");                                   // store the updated result length
    emitter.instruction("ldr x12, [sp, #16]");                                  // result start offset
    emitter.instruction("add x12, x12, x9");                                    // compact tail offset after this chunk
    emit_symbol_address(emitter, "x13", "_concat_off");
    emitter.instruction("str x12, [x13]");                                      // publish the compacted concat-buffer tail
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("b __rt_stream_get_contents_bounded_loop");             // read the next bounded chunk

    emitter.label("__rt_stream_get_contents_bounded_release_done");
    emitter.instruction("mov x0, x1");                                          // final empty chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release it if it is heap-backed
    emitter.label("__rt_stream_get_contents_bounded_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // return the result start pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // return the accumulated result length
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bounded string slice
}

/// Emits the Linux x86_64 read-all stream helper.
fn emit_stream_get_contents_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve aligned locals for read-all accumulation
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source file descriptor
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the result start offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base address
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the running byte total to zero

    emitter.label("__rt_stream_get_contents_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source file descriptor
    emitter.instruction("mov r10d, 0x40000000");                                // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r10");                                        // synthetic user-wrapper fd?
    emitter.instruction("jl __rt_stream_get_contents_after_feof_x86");          // normal fd: skip wrapper EOF dispatch
    emitter.instruction("call __rt_feof");                                      // wrapper: check stream_eof before reading
    emitter.instruction("test rax, rax");                                       // did stream_eof report true?
    emitter.instruction("jnz __rt_stream_get_contents_done_x86");               // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_after_feof_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // running result length
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // result start offset
    emitter.instruction("add r11, r8");                                         // compact append offset = start + total
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r11");              // make __rt_fread append at the compact tail
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload fd for __rt_fread
    emitter.instruction("mov rsi, 4096");                                       // request one read-all chunk
    emitter.instruction("call __rt_fread");                                     // rax=chunk ptr, rdx=chunk len
    emitter.instruction("test rdx, rdx");                                       // empty chunk?
    emitter.instruction("jz __rt_stream_get_contents_release_done_x86");        // empty read stops the read-all loop
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save chunk pointer across the copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save chunk length across the copy
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload running result length after __rt_fread
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // result base pointer
    emitter.instruction("add r10, r8");                                         // destination = result base + total
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // source chunk pointer
    emitter.instruction("xor rcx, rcx");                                        // byte-copy index
    emitter.label("__rt_stream_get_contents_copy_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // copied this whole chunk?
    emitter.instruction("jge __rt_stream_get_contents_copy_done_x86");          // leave the copy loop once chunk bytes are copied
    emitter.instruction("mov r9b, BYTE PTR [r11 + rcx]");                       // load the next chunk byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // store it at the compact destination
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_stream_get_contents_copy_x86");               // copy the next byte
    emitter.label("__rt_stream_get_contents_copy_done_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // running result length before this chunk
    emitter.instruction("add r8, QWORD PTR [rbp - 48]");                        // include the copied chunk in the total
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // store the updated result length
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // result start offset
    emitter.instruction("add r11, r8");                                         // compact tail offset after this chunk
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r11");              // publish the compacted concat-buffer tail
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the chunk pointer
    emitter.instruction("call __rt_decref_any");                                // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("jmp __rt_stream_get_contents_loop_x86");               // read the next chunk

    emitter.label("__rt_stream_get_contents_release_done_x86");
    emitter.instruction("call __rt_decref_any");                                // release the empty chunk if it is heap-backed
    emitter.label("__rt_stream_get_contents_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the result start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the accumulated result length
    emitter.instruction("add rsp, 64");                                         // release the helper locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the accumulated string slice

    emit_stream_get_contents_bounded_linux_x86_64(emitter);
}

/// Emits the x86_64 bounded stream_get_contents helper.
///
/// Input: `rdi = fd`, `rsi = max bytes`. Output: `rax = ptr`, `rdx = len`.
/// The helper compacts each `__rt_fread` chunk into `_concat_buf` so filters or
/// wrappers that return separate buffers still produce one contiguous result.
fn emit_stream_get_contents_bounded_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents_bounded ---");
    emitter.label_global("__rt_stream_get_contents_bounded");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve aligned locals for bounded accumulation
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested byte cap
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // snapshot the concat-buffer start offset
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the result start offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // running result length = 0

    emitter.label("__rt_stream_get_contents_bounded_loop_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // requested byte cap
    emitter.instruction("cmp r8, r9");                                          // has the result reached the requested cap?
    emitter.instruction("jge __rt_stream_get_contents_bounded_done_x86");       // stop once the cap is filled
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source descriptor
    emitter.instruction("mov r10d, 0x40000000");                                // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r10");                                        // synthetic user-wrapper fd?
    emitter.instruction("jl __rt_stream_get_contents_bounded_after_feof_x86");  // normal fd: skip wrapper EOF dispatch
    emitter.instruction("call __rt_feof");                                      // wrapper: check stream_eof before reading
    emitter.instruction("test rax, rax");                                       // did stream_eof report true?
    emitter.instruction("jnz __rt_stream_get_contents_bounded_done_x86");       // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_bounded_after_feof_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // requested byte cap
    emitter.instruction("sub rsi, r8");                                         // remaining bytes needed
    emitter.instruction("mov r10, 4096");                                       // maximum chunk request
    emitter.instruction("cmp rsi, r10");                                        // is the remaining cap bigger than one chunk?
    emitter.instruction("cmovg rsi, r10");                                      // request min(remaining, 4096)
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // result start offset
    emitter.instruction("add r11, r8");                                         // compact append offset = start + total
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r11");              // make __rt_fread append at the compact tail
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload fd for __rt_fread
    emitter.instruction("call __rt_fread");                                     // rax=chunk ptr, rdx=chunk len
    emitter.instruction("test rdx, rdx");                                       // empty chunk?
    emitter.instruction("jz __rt_stream_get_contents_bounded_release_done_x86"); // empty read stops the bounded loop
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // requested byte cap
    emitter.instruction("sub r9, r8");                                          // remaining bytes allowed
    emitter.instruction("cmp rdx, r9");                                         // did the source return more than requested?
    emitter.instruction("cmova rdx, r9");                                       // clamp the chunk to the remaining cap
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save chunk pointer across the copy
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save chunk length across the copy
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base
    emitter.instruction("add r10, QWORD PTR [rbp - 24]");                       // result base pointer
    emitter.instruction("add r10, r8");                                         // destination = result base + total
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // source chunk pointer
    emitter.instruction("xor rcx, rcx");                                        // byte-copy index
    emitter.label("__rt_stream_get_contents_bounded_copy_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 56]");                       // copied this whole chunk?
    emitter.instruction("jge __rt_stream_get_contents_bounded_copy_done_x86");  // leave the copy loop once chunk bytes are copied
    emitter.instruction("mov r9b, BYTE PTR [r11 + rcx]");                       // load the next chunk byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // store it at the compact destination
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_stream_get_contents_bounded_copy_x86");       // copy the next byte
    emitter.label("__rt_stream_get_contents_bounded_copy_done_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length before this chunk
    emitter.instruction("add r8, QWORD PTR [rbp - 56]");                        // include the copied chunk in the total
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // store the updated result length
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // result start offset
    emitter.instruction("add r11, r8");                                         // compact tail offset after this chunk
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r11");              // publish the compacted concat-buffer tail
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the chunk pointer
    emitter.instruction("call __rt_decref_any");                                // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("jmp __rt_stream_get_contents_bounded_loop_x86");       // read the next bounded chunk

    emitter.label("__rt_stream_get_contents_bounded_release_done_x86");
    emitter.instruction("call __rt_decref_any");                                // release the empty chunk if it is heap-backed
    emitter.label("__rt_stream_get_contents_bounded_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the result start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the accumulated result length
    emitter.instruction("add rsp, 64");                                         // release the helper locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bounded string slice
}
