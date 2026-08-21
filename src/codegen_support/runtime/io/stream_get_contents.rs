//! Purpose:
//! Emits the `__rt_stream_get_contents` runtime helper assembly for stream_get_contents.
//! Reads every remaining byte from a stream through the same fread path used by
//! bounded reads.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The accumulation buffer is reserved through `__rt_concat_reserve` and enlarged through
//!   `__rt_concat_grow`, so a stream larger than the 64 KiB concat scratch produces an owned
//!   heap string instead of running off the end of `_concat_buf` into the adjacent BSS globals.
//! - The reservation claims its whole window with `__rt_concat_publish(buf, capacity)` before
//!   the first `__rt_fread`, so each chunk reservation lands *after* the accumulated result;
//!   `__rt_concat_publish(chunk, 0)` then hands the chunk window straight back.
//! - The read-all loop uses `__rt_fread` so TLS sessions, filters, and wrapper
//!   reads share one I/O dispatch path.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Initial accumulation capacity, in bytes. Two read chunks wide so the first `__rt_fread`
/// never has to grow, and small enough that short streams still stay in concat scratch.
const INITIAL_CAPACITY: usize = 8192;
/// Bytes requested from `__rt_fread` per iteration.
const READ_CHUNK: usize = 4096;

/// Emits the read-all stream helper.
///
/// Input: `x0 = fd`. Output: `x1 = string pointer`, `x2 = total bytes read`.
/// The helper loops through `__rt_fread`, copies each returned chunk into a reserved
/// accumulation buffer that grows on demand, and stops when EOF or an empty read is produced.
pub fn emit_stream_get_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_contents_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #96");                                     // allocate locals plus saved frame pointer and return address
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source file descriptor

    // -- reserve the accumulation buffer and claim its whole window --
    emitter.instruction(&format!("mov x9, #{}", INITIAL_CAPACITY));             // start from the initial accumulation capacity
    emitter.instruction("str x9, [sp, #48]");                                   // save the current accumulation capacity
    emitter.instruction("mov x0, x9");                                          // request the initial capacity from the reservation front end
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the accumulated result
    emitter.instruction("str x0, [sp, #16]");                                   // save the accumulation buffer pointer
    emitter.instruction("mov x1, x0");                                          // publish the reservation start pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // publish the full reserved capacity
    emitter.instruction("bl __rt_concat_publish");                              // claim the whole window so each chunk read appends after it
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the running byte total to zero

    // -- read fixed-size chunks through fread until EOF --
    emitter.label("__rt_stream_get_contents_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source file descriptor
    emitter.instruction("mov w11, #0x4000");                                    // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w11, w11, #16");                                   // form 0x40000000
    emitter.instruction("cmp x0, x11");                                         // synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_stream_get_contents_after_feof");            // normal fd: skip wrapper EOF dispatch
    emitter.instruction("bl __rt_feof");                                        // wrapper: check stream_eof before reading
    emitter.instruction("cbnz x0, __rt_stream_get_contents_done");              // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_after_feof");

    // -- make room for one more chunk before asking fread for it --
    emitter.instruction("ldr x9, [sp, #24]");                                   // running result length
    emitter.instruction(&format!("add x9, x9, #{}", READ_CHUNK));               // capacity needed once the next chunk lands
    emitter.instruction("ldr x10, [sp, #48]");                                  // current accumulation capacity
    emitter.instruction("cmp x9, x10");                                         // does the next chunk still fit the current reservation?
    emitter.instruction("b.ls __rt_stream_get_contents_have_room");             // no growth needed for this iteration
    emitter.instruction("lsl x10, x10, #1");                                    // double the accumulation capacity
    emitter.instruction("cmp x10, x9");                                         // is the doubled capacity already large enough?
    emitter.instruction("csel x10, x10, x9, hi");                               // keep whichever capacity is larger
    emitter.instruction("str x10, [sp, #48]");                                  // save the grown accumulation capacity
    emitter.instruction("ldr x1, [sp, #16]");                                   // old accumulation buffer pointer
    emitter.instruction("mov x2, #0");                                          // release the whole claimed window
    emitter.instruction("bl __rt_concat_publish");                              // hand the old scratch window back before moving to heap storage
    emitter.instruction("ldr x0, [sp, #16]");                                   // old accumulation buffer pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // bytes accumulated so far must survive the move
    emitter.instruction("ldr x2, [sp, #48]");                                   // grown accumulation capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the accumulated bytes into a larger owned buffer
    emitter.instruction("str x0, [sp, #16]");                                   // save the grown accumulation buffer pointer
    emitter.label("__rt_stream_get_contents_have_room");

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for __rt_fread
    emitter.instruction(&format!("mov x1, #{}", READ_CHUNK));                   // request one read-all chunk
    emitter.instruction("bl __rt_fread");                                       // x1=chunk ptr, x2=chunk len
    emitter.instruction("cbz x2, __rt_stream_get_contents_release_done");       // empty read stops the read-all loop
    emitter.instruction("str x1, [sp, #32]");                                   // save chunk pointer across the copy
    emitter.instruction("str x2, [sp, #40]");                                   // save chunk length across the copy
    emitter.instruction("ldr x11, [sp, #16]");                                  // accumulation buffer base pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // running result length before this chunk
    emitter.instruction("add x11, x11, x9");                                    // destination = accumulation base + total
    emitter.instruction("mov x12, #0");                                         // byte-copy index
    emitter.label("__rt_stream_get_contents_copy");
    emitter.instruction("cmp x12, x2");                                         // copied this whole chunk?
    emitter.instruction("b.ge __rt_stream_get_contents_copy_done");             // leave the copy loop once chunk bytes are copied
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load the next chunk byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it at the accumulation destination
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_stream_get_contents_copy");                     // copy the next byte
    emitter.label("__rt_stream_get_contents_copy_done");
    emitter.instruction("ldr x9, [sp, #24]");                                   // running result length before this chunk
    emitter.instruction("ldr x10, [sp, #40]");                                  // copied chunk length
    emitter.instruction("add x9, x9, x10");                                     // include the copied chunk in the total
    emitter.instruction("str x9, [sp, #24]");                                   // store the updated result length
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the chunk pointer
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand this chunk's scratch window back for the next read
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("b __rt_stream_get_contents_loop");                     // read the next chunk

    // -- release the terminal empty chunk and return the accumulated string --
    emitter.label("__rt_stream_get_contents_release_done");
    emitter.instruction("str x1, [sp, #32]");                                   // save the final empty chunk pointer
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand the terminal chunk's scratch window back
    emitter.instruction("ldr x0, [sp, #32]");                                   // final empty chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release it if it is heap-backed
    emitter.label("__rt_stream_get_contents_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the accumulation buffer pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the accumulated result length
    emitter.instruction("bl __rt_concat_publish");                              // shrink the claimed window down to the bytes actually accumulated
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the accumulated string

    emit_stream_get_contents_bounded_aarch64(emitter);
}

/// Emits the AArch64 bounded stream_get_contents helper.
///
/// Input: `x0 = fd`, `x1 = max bytes`. Output: `x1 = ptr`, `x2 = len`.
/// The loop calls `__rt_fread` repeatedly, copies each returned chunk into a reserved
/// accumulation buffer that grows on demand (never beyond the requested cap), and stops
/// at the requested byte count or EOF.
fn emit_stream_get_contents_bounded_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents_bounded ---");
    emitter.label_global("__rt_stream_get_contents_bounded");

    emitter.instruction("sub sp, sp, #96");                                     // allocate locals plus saved frame pointer and return address
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested byte cap

    // -- reserve min(cap, initial capacity) and claim the whole window --
    emitter.instruction(&format!("mov x9, #{}", INITIAL_CAPACITY));             // start from the initial accumulation capacity
    emitter.instruction("cmp x1, x9");                                          // is the requested cap smaller than the initial capacity?
    emitter.instruction("csel x9, x1, x9, lt");                                 // never reserve more than the caller asked for
    emitter.instruction("cmp x9, #0");                                          // a non-positive cap reserves nothing at all
    emitter.instruction("csel x9, xzr, x9, lt");                                // clamp a negative cap to a zero-byte reservation
    emitter.instruction("str x9, [sp, #56]");                                   // save the current accumulation capacity
    emitter.instruction("mov x0, x9");                                          // request the initial capacity from the reservation front end
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the accumulated result
    emitter.instruction("str x0, [sp, #24]");                                   // save the accumulation buffer pointer
    emitter.instruction("mov x1, x0");                                          // publish the reservation start pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // publish the full reserved capacity
    emitter.instruction("bl __rt_concat_publish");                              // claim the whole window so each chunk read appends after it
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

    // -- make room for one more (cap-clamped) chunk before asking fread for it --
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction(&format!("add x9, x9, #{}", READ_CHUNK));               // capacity needed once the next chunk lands
    emitter.instruction("ldr x10, [sp, #8]");                                   // requested byte cap
    emitter.instruction("cmp x9, x10");                                         // would that exceed the caller's cap?
    emitter.instruction("csel x9, x9, x10, lt");                                // never grow the reservation past the requested cap
    emitter.instruction("ldr x10, [sp, #56]");                                  // current accumulation capacity
    emitter.instruction("cmp x9, x10");                                         // does the next chunk still fit the current reservation?
    emitter.instruction("b.ls __rt_stream_get_contents_bounded_have_room");     // no growth needed for this iteration
    emitter.instruction("lsl x10, x10, #1");                                    // double the accumulation capacity
    emitter.instruction("cmp x10, x9");                                         // is the doubled capacity already large enough?
    emitter.instruction("csel x10, x10, x9, hi");                               // keep whichever capacity is larger
    emitter.instruction("ldr x11, [sp, #8]");                                   // requested byte cap
    emitter.instruction("cmp x10, x11");                                        // did doubling overshoot the caller's cap?
    emitter.instruction("csel x10, x10, x11, lt");                              // clamp the grown capacity to the requested cap
    emitter.instruction("str x10, [sp, #56]");                                  // save the grown accumulation capacity
    emitter.instruction("ldr x1, [sp, #24]");                                   // old accumulation buffer pointer
    emitter.instruction("mov x2, #0");                                          // release the whole claimed window
    emitter.instruction("bl __rt_concat_publish");                              // hand the old scratch window back before moving to heap storage
    emitter.instruction("ldr x0, [sp, #24]");                                   // old accumulation buffer pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // bytes accumulated so far must survive the move
    emitter.instruction("ldr x2, [sp, #56]");                                   // grown accumulation capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the accumulated bytes into a larger owned buffer
    emitter.instruction("str x0, [sp, #24]");                                   // save the grown accumulation buffer pointer
    emitter.label("__rt_stream_get_contents_bounded_have_room");
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction("ldr x10, [sp, #8]");                                   // requested byte cap
    emitter.instruction("sub x1, x10, x9");                                     // remaining bytes needed
    emitter.instruction(&format!("mov x11, #{}", READ_CHUNK));                  // maximum chunk request
    emitter.instruction("cmp x1, x11");                                         // is the remaining cap smaller than the chunk size?
    emitter.instruction("csel x1, x1, x11, lt");                                // request min(remaining, chunk size)
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
    emitter.instruction("ldr x11, [sp, #24]");                                  // accumulation buffer base pointer
    emitter.instruction("add x11, x11, x9");                                    // destination = accumulation base + total
    emitter.instruction("mov x12, #0");                                         // byte-copy index
    emitter.label("__rt_stream_get_contents_bounded_copy");
    emitter.instruction("cmp x12, x2");                                         // copied this whole chunk?
    emitter.instruction("b.ge __rt_stream_get_contents_bounded_copy_done");     // leave the copy loop once chunk bytes are copied
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load the next chunk byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it at the accumulation destination
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_stream_get_contents_bounded_copy");             // copy the next byte
    emitter.label("__rt_stream_get_contents_bounded_copy_done");
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length before this chunk
    emitter.instruction("ldr x10, [sp, #48]");                                  // copied chunk length
    emitter.instruction("add x9, x9, x10");                                     // include the copied chunk in the total
    emitter.instruction("str x9, [sp, #32]");                                   // store the updated result length
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the chunk pointer
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand this chunk's scratch window back for the next read
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("b __rt_stream_get_contents_bounded_loop");             // read the next bounded chunk

    emitter.label("__rt_stream_get_contents_bounded_release_done");
    emitter.instruction("str x1, [sp, #40]");                                   // save the final empty chunk pointer
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand the terminal chunk's scratch window back
    emitter.instruction("ldr x0, [sp, #40]");                                   // final empty chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release it if it is heap-backed
    emitter.label("__rt_stream_get_contents_bounded_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // return the accumulation buffer pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // return the accumulated result length
    emitter.instruction("bl __rt_concat_publish");                              // shrink the claimed window down to the bytes actually accumulated
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bounded string
}

/// Emits the Linux x86_64 read-all stream helper.
///
/// Input: `rdi = fd`. Output: `rax = ptr`, `rdx = len`. Mirrors the AArch64 accumulation
/// strategy: reserve, claim the window, grow on demand, publish the final length.
fn emit_stream_get_contents_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve aligned locals for read-all accumulation
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source file descriptor

    // -- reserve the accumulation buffer and claim its whole window --
    emitter.instruction(&format!("mov QWORD PTR [rbp - 56], {}", INITIAL_CAPACITY)); // start from the initial accumulation capacity
    emitter.instruction(&format!("mov rax, {}", INITIAL_CAPACITY));             // request the initial capacity from the reservation front end
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the accumulated result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the accumulation buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // publish the full reserved capacity
    emitter.instruction("call __rt_concat_publish");                            // claim the whole window so each chunk read appends after it
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

    // -- make room for one more chunk before asking fread for it --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // running result length
    emitter.instruction(&format!("add r8, {}", READ_CHUNK));                    // capacity needed once the next chunk lands
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // current accumulation capacity
    emitter.instruction("cmp r8, r9");                                          // does the next chunk still fit the current reservation?
    emitter.instruction("jbe __rt_stream_get_contents_have_room_x86");          // no growth needed for this iteration
    emitter.instruction("add r9, r9");                                          // double the accumulation capacity
    emitter.instruction("cmp r9, r8");                                          // is the doubled capacity already large enough?
    emitter.instruction("cmovb r9, r8");                                        // keep whichever capacity is larger
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save the grown accumulation capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // old accumulation buffer pointer
    emitter.instruction("xor edx, edx");                                        // release the whole claimed window
    emitter.instruction("call __rt_concat_publish");                            // hand the old scratch window back before moving to heap storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // old accumulation buffer pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // bytes accumulated so far must survive the move
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // grown accumulation capacity
    emitter.instruction("call __rt_concat_grow");                               // move the accumulated bytes into a larger owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the grown accumulation buffer pointer
    emitter.label("__rt_stream_get_contents_have_room_x86");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload fd for __rt_fread
    emitter.instruction(&format!("mov rsi, {}", READ_CHUNK));                   // request one read-all chunk
    emitter.instruction("call __rt_fread");                                     // rax=chunk ptr, rdx=chunk len
    emitter.instruction("test rdx, rdx");                                       // empty chunk?
    emitter.instruction("jz __rt_stream_get_contents_release_done_x86");        // empty read stops the read-all loop
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save chunk pointer across the copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save chunk length across the copy
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // accumulation buffer base pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 32]");                       // destination = accumulation base + total
    emitter.instruction("mov r11, rax");                                        // source chunk pointer
    emitter.instruction("xor rcx, rcx");                                        // byte-copy index
    emitter.label("__rt_stream_get_contents_copy_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // copied this whole chunk?
    emitter.instruction("jge __rt_stream_get_contents_copy_done_x86");          // leave the copy loop once chunk bytes are copied
    emitter.instruction("mov r9b, BYTE PTR [r11 + rcx]");                       // load the next chunk byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // store it at the accumulation destination
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_stream_get_contents_copy_x86");               // copy the next byte
    emitter.label("__rt_stream_get_contents_copy_done_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // running result length before this chunk
    emitter.instruction("add r8, QWORD PTR [rbp - 48]");                        // include the copied chunk in the total
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // store the updated result length
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the chunk pointer
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand this chunk's scratch window back for the next read
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the chunk pointer
    emitter.instruction("call __rt_decref_any");                                // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("jmp __rt_stream_get_contents_loop_x86");               // read the next chunk

    emitter.label("__rt_stream_get_contents_release_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the final empty chunk pointer
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand the terminal chunk's scratch window back
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // final empty chunk pointer
    emitter.instruction("call __rt_decref_any");                                // release the empty chunk if it is heap-backed
    emitter.label("__rt_stream_get_contents_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the accumulation buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the accumulated result length
    emitter.instruction("call __rt_concat_publish");                            // shrink the claimed window down to the bytes actually accumulated
    emitter.instruction("add rsp, 64");                                         // release the helper locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the accumulated string

    emit_stream_get_contents_bounded_linux_x86_64(emitter);
}

/// Emits the x86_64 bounded stream_get_contents helper.
///
/// Input: `rdi = fd`, `rsi = max bytes`. Output: `rax = ptr`, `rdx = len`.
/// The helper copies each `__rt_fread` chunk into a reserved accumulation buffer that grows
/// on demand but never past the requested cap, so filters or wrappers that return separate
/// buffers still produce one contiguous result.
fn emit_stream_get_contents_bounded_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents_bounded ---");
    emitter.label_global("__rt_stream_get_contents_bounded");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve aligned locals for bounded accumulation
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested byte cap

    // -- reserve min(cap, initial capacity) and claim the whole window --
    emitter.instruction(&format!("mov rax, {}", INITIAL_CAPACITY));             // start from the initial accumulation capacity
    emitter.instruction("cmp rsi, rax");                                        // is the requested cap smaller than the initial capacity?
    emitter.instruction("cmovl rax, rsi");                                      // never reserve more than the caller asked for
    emitter.instruction("xor r8d, r8d");                                        // a non-positive cap reserves nothing at all
    emitter.instruction("cmp rax, 0");                                          // is the clamped capacity negative?
    emitter.instruction("cmovl rax, r8");                                       // clamp a negative cap to a zero-byte reservation
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the current accumulation capacity
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the accumulated result
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the accumulation buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // publish the full reserved capacity
    emitter.instruction("call __rt_concat_publish");                            // claim the whole window so each chunk read appends after it
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

    // -- make room for one more (cap-clamped) chunk before asking fread for it --
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length
    emitter.instruction(&format!("add r8, {}", READ_CHUNK));                    // capacity needed once the next chunk lands
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // requested byte cap
    emitter.instruction("cmp r8, r9");                                          // would that exceed the caller's cap?
    emitter.instruction("cmovg r8, r9");                                        // never grow the reservation past the requested cap
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // current accumulation capacity
    emitter.instruction("cmp r8, r9");                                          // does the next chunk still fit the current reservation?
    emitter.instruction("jbe __rt_stream_get_contents_bounded_have_room_x86");  // no growth needed for this iteration
    emitter.instruction("add r9, r9");                                          // double the accumulation capacity
    emitter.instruction("cmp r9, r8");                                          // is the doubled capacity already large enough?
    emitter.instruction("cmovb r9, r8");                                        // keep whichever capacity is larger
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // requested byte cap
    emitter.instruction("cmp r9, r10");                                         // did doubling overshoot the caller's cap?
    emitter.instruction("cmovg r9, r10");                                       // clamp the grown capacity to the requested cap
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the grown accumulation capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // old accumulation buffer pointer
    emitter.instruction("xor edx, edx");                                        // release the whole claimed window
    emitter.instruction("call __rt_concat_publish");                            // hand the old scratch window back before moving to heap storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // old accumulation buffer pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // bytes accumulated so far must survive the move
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // grown accumulation capacity
    emitter.instruction("call __rt_concat_grow");                               // move the accumulated bytes into a larger owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the grown accumulation buffer pointer
    emitter.label("__rt_stream_get_contents_bounded_have_room_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // requested byte cap
    emitter.instruction("sub rsi, r8");                                         // remaining bytes needed
    emitter.instruction(&format!("mov r10, {}", READ_CHUNK));                   // maximum chunk request
    emitter.instruction("cmp rsi, r10");                                        // is the remaining cap bigger than one chunk?
    emitter.instruction("cmovg rsi, r10");                                      // request min(remaining, chunk size)
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
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // accumulation buffer base pointer
    emitter.instruction("add r10, r8");                                         // destination = accumulation base + total
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // source chunk pointer
    emitter.instruction("xor rcx, rcx");                                        // byte-copy index
    emitter.label("__rt_stream_get_contents_bounded_copy_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 56]");                       // copied this whole chunk?
    emitter.instruction("jge __rt_stream_get_contents_bounded_copy_done_x86");  // leave the copy loop once chunk bytes are copied
    emitter.instruction("mov r9b, BYTE PTR [r11 + rcx]");                       // load the next chunk byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // store it at the accumulation destination
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_stream_get_contents_bounded_copy_x86");       // copy the next byte
    emitter.label("__rt_stream_get_contents_bounded_copy_done_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length before this chunk
    emitter.instruction("add r8, QWORD PTR [rbp - 56]");                        // include the copied chunk in the total
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // store the updated result length
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the chunk pointer
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand this chunk's scratch window back for the next read
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the chunk pointer
    emitter.instruction("call __rt_decref_any");                                // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("jmp __rt_stream_get_contents_bounded_loop_x86");       // read the next bounded chunk

    emitter.label("__rt_stream_get_contents_bounded_release_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the final empty chunk pointer
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand the terminal chunk's scratch window back
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // final empty chunk pointer
    emitter.instruction("call __rt_decref_any");                                // release the empty chunk if it is heap-backed
    emitter.label("__rt_stream_get_contents_bounded_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the accumulation buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the accumulated result length
    emitter.instruction("call __rt_concat_publish");                            // shrink the claimed window down to the bytes actually accumulated
    emitter.instruction("add rsp, 80");                                         // release the helper locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bounded string
}
