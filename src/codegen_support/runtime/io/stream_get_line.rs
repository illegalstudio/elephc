//! Purpose:
//! Emits the `__rt_stream_get_line` runtime helper assembly for the
//! stream_get_line builtin.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The caller's `$length` budget is reserved up front through `__rt_concat_reserve`, so a
//!   budget larger than the remaining 64 KiB concat scratch takes owned heap storage instead
//!   of writing past `_concat_buf` into the adjacent BSS globals.
//! - Reads one byte at a time into that reservation until the byte budget is
//!   spent, EOF is reached, or the trailing bytes match the ending delimiter
//!   (which is consumed and stripped). EOF/read failure updates `StreamState`.
//! - A read that would BLOCK is not the end of the line: php answers `false` and leaves the bytes
//!   ON the stream, so the next call sees them prefixed to whatever arrives. Measured on `php -n`
//!   8.5.6 over a non-blocking socket pair, "abc" with no newline answers `false`, and once
//!   "def\n" arrives the next call answers "abcdef". elephc consumed the "abc" and handed it back
//!   as a line php never breaks. `__rt_stream_pending_put` holds it; the entry drains it back.
//! - A third output reports whether ANY byte was consumed. PHP returns `false` only when
//!   the call found nothing at all; a delimiter sitting at the read position yields an
//!   empty string, so the stripped length alone cannot tell the two apart.
//! - A stream carrying a read-filter chain sources its bytes from `__rt_fread` instead of the
//!   descriptor, exactly as `__rt_fgets` does. Only the byte SOURCE moves: the delimiter scan
//!   and the byte budget stay one copy of the code, so the filtered and raw paths cannot drift.

use crate::codegen_support::runtime::resources::layout::STREAM_READ_FILTER_HEAD_OFFSET;
use crate::codegen_support::{abi, abi::emit_symbol_address, emit::Emitter, platform::Arch};

/// stream_get_line: read up to a length or an ending delimiter from a stream.
/// Input:  x0=handle, x1=max length, x2=ending pointer, x3=ending length
/// Output: x1=string pointer (concat scratch or owned heap storage), x2=length read
///         (delimiter stripped), x0=1 when any byte was consumed, 0 when nothing was
///         (x86_64 returns the pair in rax/rdx and the consumed flag in rcx)
pub fn emit_stream_get_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_line_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_get_line ---");
    emitter.label_global("__rt_stream_get_line");

    // Frame: [0..16) regs, [16) handle, [24) length, [32) ending ptr, [40) ending
    //        len, [48) result start, [56) running total, [64) backend fd,
    //        [80) read-filter chain head.
    emitter.instruction("sub sp, sp, #112");                                    // frame for saved regs and parse state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the opaque stream handle
    emitter.instruction("str x1, [sp, #24]");                                   // save the maximum length
    emitter.instruction("str x2, [sp, #32]");                                   // save the ending-delimiter pointer
    emitter.instruction("str x3, [sp, #40]");                                   // save the ending-delimiter length
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor through StreamState
    emitter.instruction("str x0, [sp, #64]");                                   // preserve the resolved backend descriptor

    // -- does the stream carry a read-filter chain? --
    // Probed once, up front: the answer picks the byte source for every iteration below.
    emitter.instruction("ldr x0, [sp, #16]");                                   // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable stream state
    emitter.instruction("cbz x0, __rt_sgl_unfiltered");                         // no state: nothing can be attached to it
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_READ_FILTER_HEAD_OFFSET}]")); // read-direction chain head
    emitter.instruction("str x9, [sp, #80]");                                   // remember whether the chain exists
    emitter.instruction("b __rt_sgl_filter_probed");                            // continue with the reservation
    emitter.label("__rt_sgl_unfiltered");
    emitter.instruction("str xzr, [sp, #80]");                                  // an unfiltered stream keeps the descriptor path
    emitter.label("__rt_sgl_filter_probed");
    emitter.instruction("ldr x1, [sp, #24]");                                   // the state lookup clobbered the budget register

    emitter.instruction("mov x0, x1");                                          // the line can never exceed the caller's byte budget
    emitter.instruction("cmp x0, #0");                                          // is the requested budget non-positive?
    emitter.instruction("csel x0, xzr, x0, lt");                                // a negative budget reserves nothing at all
    emitter.instruction("str x0, [sp, #88]");                                   // remember the clamped budget: the filtered path claims exactly it
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the whole budget
    emitter.instruction("str x0, [sp, #48]");                                   // save the result start pointer
    emitter.instruction("str xzr, [sp, #56]");                                  // running total starts at zero
    emitter.instruction("str xzr, [sp, #72]");                                  // nothing consumed yet: the caller sees PHP false


    // -- a read-filter chain outranks the backend: __rt_fread pulls through it either way --
    // The reservation above is NOT claimed on the raw path, because nothing nested allocates
    // from the scratch there. `__rt_fread` does, so the filtered path has to claim the whole
    // window first or the byte it hands back would land on top of the line being built.
    // `__rt_concat_publish` writes an absolute offset, so the shrink at `_done` still lands
    // on the bytes actually read.
    emitter.instruction("ldr x9, [sp, #80]");                                   // the read-direction chain head
    emitter.instruction("cbz x9, __rt_sgl_backend_probe");                      // unfiltered: leave the scratch cursor alone
    emitter.instruction("ldr x1, [sp, #48]");                                   // the reserved result window
    emitter.instruction("ldr x2, [sp, #88]");                                   // for the whole clamped budget
    emitter.instruction("bl __rt_concat_publish");                              // claim it so nested filtered reads append after it
    emitter.instruction("b __rt_stream_get_line_loop");                         // filtered: the loop reads through the chain
    emitter.label("__rt_sgl_backend_probe");

    // -- user-wrapper fd: read via stream_read into _user_wrapper_drain_buf --
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the backend descriptor
    emitter.instruction("mov w9, #0x4000");                                     // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w9, w9, #16");                                     // form 0x40000000 in w9
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.lo __rt_stream_get_line_loop");                      // native descriptors use the byte-read loop below
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("add x10, x9, x10");                                    // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp x0, x10");                                         // is the backend above the wrapper range?
    emitter.instruction("b.lo __rt_sgl_wrapper_entry");                         // wrappers read via the feof-gated stream_read loop below

    emitter.label("__rt_stream_get_line_loop");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("ldr x11, [sp, #24]");                                  // maximum length
    emitter.instruction("cmp x10, x11");                                        // reached the byte budget?
    emitter.instruction("b.ge __rt_stream_get_line_done");                      // stop at the maximum length

    emitter.instruction("ldr x9, [sp, #80]");                                   // the read-direction chain head
    emitter.instruction("cbnz x9, __rt_sgl_filtered_byte");                     // filtered: take the byte from the chain
    emitter.instruction("ldr x1, [sp, #48]");                                   // reserved result start pointer
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("add x1, x1, x10");                                     // single-byte write pointer inside the reservation

    // -- a byte the stream is already HOLDING is the byte the descriptor would have given --
    //
    // It has to arrive through the same door as a byte off the descriptor, because the delimiter
    // scan lives BELOW that door: a bulk drain straight into the result window skipped it, and
    // `stream_get_line($h, 1024, "ef")` after an `fgets()` that left `def` on the stream answered
    // the whole `def` where php answers `d`. The delimiter can also STRADDLE the boundary between
    // held bytes and the descriptor, and the tail comparison below is what already handles that.
    emitter.instruction("str x1, [sp, #96]");                                   // the destination this iteration writes
    emitter.instruction("ldr x0, [sp, #16]");                                   // the opaque stream handle
    emitter.instruction("mov x2, #1");                                          // one held byte
    emitter.instruction("bl __rt_stream_pending_take");                         // x0 = 1 when one came back
    emitter.instruction("cbnz x0, __rt_sgl_byte_counted");                      // join the shared delimiter scan
    emitter.instruction("ldr x1, [sp, #96]");                                   // the destination again

    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the resolved backend descriptor
    emitter.instruction("mov x2, #1");                                          // read exactly one byte
    emitter.syscall(3);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_get_line_read_ok")); // continue when the read succeeded
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", plat.would_block_errno())); // Linux: compare read result with -EAGAIN/-EWOULDBLOCK
    } else {
        emitter.instruction(&format!("cmp x0, #{}", plat.would_block_errno())); // macOS: compare errno with EAGAIN/EWOULDBLOCK
    }
    emitter.instruction("b.eq __rt_sgl_would_block");                           // transient nonblocking miss is not EOF
    emitter.instruction("b __rt_stream_get_line_eof");                          // a read failure ends the line

    // -- nothing more to read RIGHT NOW: php keeps the partial line and answers false --
    emitter.label("__rt_sgl_would_block");
    emitter.instruction("ldr x2, [sp, #56]");                                   // what the line has so far
    emitter.instruction("cbz x2, __rt_stream_get_line_done");                   // nothing gathered: already false
    emitter.instruction("ldr x0, [sp, #16]");                                   // the opaque stream handle
    emitter.instruction("ldr x1, [sp, #48]");                                   // the bytes to give back
    emitter.instruction("bl __rt_stream_pending_put");                          // they stay ON the stream
    emitter.instruction("str xzr, [sp, #56]");                                  // the caller receives nothing
    emitter.instruction("str xzr, [sp, #72]");                                  // which php spells false
    emitter.instruction("b __rt_stream_get_line_done");
    emitter.label("__rt_stream_get_line_read_ok");
    emitter.instruction("cbz x0, __rt_stream_get_line_eof");                    // a zero-byte read means EOF

    emitter.label("__rt_sgl_byte_counted");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("add x10, x10, #1");                                    // count the new byte
    emitter.instruction("str x10, [sp, #56]");                                  // store the running total
    emitter.instruction("mov x11, #1");                                         // a byte reached the buffer
    emitter.instruction("str x11, [sp, #72]");                                  // so the result is a string, even if the delimiter strips it empty

    // -- check whether the trailing bytes match the ending delimiter --
    emitter.instruction("ldr x3, [sp, #40]");                                   // ending-delimiter length
    emitter.instruction("cbz x3, __rt_stream_get_line_loop");                   // no delimiter: keep reading
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("cmp x10, x3");                                         // enough bytes for a delimiter match?
    emitter.instruction("b.lt __rt_stream_get_line_loop");                      // not yet: keep reading
    emitter.instruction("ldr x12, [sp, #48]");                                  // result start pointer
    emitter.instruction("sub x13, x10, x3");                                    // offset of the candidate tail
    emitter.instruction("add x13, x12, x13");                                   // pointer to the candidate tail
    emitter.instruction("ldr x14, [sp, #32]");                                  // ending-delimiter pointer
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index
    emitter.label("__rt_stream_get_line_cmp");
    emitter.instruction("cmp x15, x3");                                         // compared every delimiter byte?
    emitter.instruction("b.ge __rt_stream_get_line_matched");                   // a full match ends the line
    emitter.instruction("ldrb w16, [x13, x15]");                                // a tail byte
    emitter.instruction("ldrb w17, [x14, x15]");                                // the matching delimiter byte
    emitter.instruction("cmp w16, w17");                                        // do they differ?
    emitter.instruction("b.ne __rt_stream_get_line_loop");                      // mismatch: keep reading
    emitter.instruction("add x15, x15, #1");                                    // advance the comparison
    emitter.instruction("b __rt_stream_get_line_cmp");                          // compare the next delimiter byte

    emitter.label("__rt_stream_get_line_matched");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("sub x10, x10, x3");                                    // drop the delimiter from the result
    emitter.instruction("str x10, [sp, #56]");                                  // store the stripped total
    emitter.instruction("b __rt_stream_get_line_done");                         // a delimiter match is not EOF

    // -- filtered byte source: one byte at a time out of the chain's buffered output --
    // `__rt_fread` is the only helper that runs the chain, so a one-byte request is what
    // makes this loop see filtered bytes. The byte is copied into the claimed window before
    // the chunk is released, and the delimiter scan below is the same code the raw path runs.
    emitter.label("__rt_sgl_filtered_byte");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the opaque stream handle, not the descriptor
    emitter.instruction("mov x1, #1");                                          // one filtered byte
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_stream_get_line_eof");                    // the chain is drained and flushed: EOF
    emitter.instruction("ldrb w13, [x1]");                                      // the filtered byte
    emitter.instruction("ldr x12, [sp, #48]");                                  // the claimed result window
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("strb w13, [x12, x10]");                                // append it to the line
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand this byte's scratch window back before the next read
    emitter.instruction("mov x0, x1");                                          // chunk ptr for release
    emitter.instruction("bl __rt_decref_any");                                  // release the chunk before continuing the line
    emitter.instruction("b __rt_sgl_byte_counted");                             // join the shared count and delimiter scan

    // -- user-wrapper line read: feof-gated stream_read into _user_wrapper_drain_buf
    //    (a SEPARATE buffer from _concat_buf, which each __rt_fread result may
    //    occupy). [sp,#48] = drain-buf base, [sp,#56] = running length. Stops at
    //    the byte budget, the ending delimiter (stripped), or EOF. --
    emitter.label("__rt_sgl_wrapper_entry");
    emit_symbol_address(emitter, "x12", "_user_wrapper_drain_buf");
    emitter.instruction("str x12, [sp, #48]");                                  // result start = drain-buf base
    emitter.label("__rt_sgl_wrapper_loop");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("ldr x11, [sp, #24]");                                  // maximum length
    emitter.instruction("cmp x10, x11");                                        // reached the byte budget?
    emitter.instruction("b.ge __rt_stream_get_line_done");                      // stop at the maximum length
    // The opaque HANDLE, not the fd: both helpers resolve the descriptor themselves, and only the
    // handle resolves the STATE where the stream's buffered bytes live.
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the opaque stream handle
    super::feof::emit_feof_call(emitter, true);                                 // elephc's own probe: never warns, the read does
    emitter.instruction("cbnz x0, __rt_stream_get_line_done");                  // at EOF: return the bytes gathered so far
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the opaque stream handle
    emitter.instruction("mov x1, #1");                                          // read exactly one byte
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_stream_get_line_done");                   // defensive: empty read also ends the line
    emitter.instruction("ldrb w13, [x1]");                                      // load the read byte
    emitter.instruction("ldr x10, [sp, #56]");                                  // current running total
    emitter.instruction("ldr x12, [sp, #48]");                                  // drain-buf base
    emitter.instruction("strb w13, [x12, x10]");                                // append the byte to the line buffer
    emitter.instruction("add x10, x10, #1");                                    // advance the running total
    emitter.instruction("str x10, [sp, #56]");                                  // store the updated total
    emitter.instruction("mov x11, #1");                                         // a byte reached the buffer
    emitter.instruction("str x11, [sp, #72]");                                  // so the result is a string, even if the delimiter strips it empty
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand this chunk's scratch window back before the next read
    emitter.instruction("mov x0, x1");                                          // chunk ptr (byte already copied)
    emitter.instruction("bl __rt_decref_any");                                  // release the owned chunk
    emitter.instruction("ldr x3, [sp, #40]");                                   // ending-delimiter length
    emitter.instruction("cbz x3, __rt_sgl_wrapper_loop");                       // no delimiter: keep reading
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("cmp x10, x3");                                         // enough bytes for a delimiter match?
    emitter.instruction("b.lt __rt_sgl_wrapper_loop");                          // not yet: keep reading
    emitter.instruction("ldr x12, [sp, #48]");                                  // drain-buf base
    emitter.instruction("sub x13, x10, x3");                                    // offset of the candidate tail
    emitter.instruction("add x13, x12, x13");                                   // pointer to the candidate tail
    emitter.instruction("ldr x14, [sp, #32]");                                  // ending-delimiter pointer
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index
    emitter.label("__rt_sgl_wrapper_cmp");
    emitter.instruction("cmp x15, x3");                                         // compared every delimiter byte?
    emitter.instruction("b.ge __rt_sgl_wrapper_matched");                       // a full match ends the line
    emitter.instruction("ldrb w16, [x13, x15]");                                // a tail byte
    emitter.instruction("ldrb w17, [x14, x15]");                                // the matching delimiter byte
    emitter.instruction("cmp w16, w17");                                        // do they differ?
    emitter.instruction("b.ne __rt_sgl_wrapper_loop");                          // mismatch: keep reading
    emitter.instruction("add x15, x15, #1");                                    // advance the comparison
    emitter.instruction("b __rt_sgl_wrapper_cmp");                              // compare the next delimiter byte
    emitter.label("__rt_sgl_wrapper_matched");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("sub x10, x10, x3");                                    // drop the delimiter from the result
    emitter.instruction("str x10, [sp, #56]");                                  // store the stripped total
    emitter.instruction("b __rt_stream_get_line_done");                         // a delimiter match is not EOF

    emitter.label("__rt_stream_get_line_eof");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the opaque stream handle
    emitter.instruction("mov x1, #1");                                          // publish the EOF state
    emitter.instruction("bl __rt_stream_eof_set");                              // update only this stream's stable state

    emitter.label("__rt_stream_get_line_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // return the result start pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // return the bytes read
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results

    // -- `php://temp` calls a drained line read the end of the stream --
    // Same extra read `fgets()` performs, and for the same reason: php reaches both through one
    // buffer fill. A refused read leaves its bytes in hand, so the probe finds them and stops.
    emitter.instruction("stp x1, x2, [sp, #88]");                               // the result survives the probe's own calls
    emitter.instruction("ldr x0, [sp, #16]");                                   // the opaque stream handle
    emitter.instruction("bl __rt_stream_temp_eof_probe");                       // one extra read, for the one wrapper that takes it
    emitter.instruction("ldp x1, x2, [sp, #88]");                               // restore the result pointer and length

    emitter.instruction("ldr x0, [sp, #72]");                                   // report whether ANY byte was consumed, after the publish call
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the frame
    emitter.instruction("ret");                                                 // return the line slice
}

/// Emits the Linux x86_64 stream runtime helper for stream get line.
fn emit_stream_get_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_line ---");
    emitter.label_global("__rt_stream_get_line");

    // Frame: [rbp-8) handle, [rbp-16) length, [rbp-24) ending ptr, [rbp-32) ending
    //        len, [rbp-40) result start, [rbp-48) running total, [rbp-56) backend fd,
    //        [rbp-72) read-filter chain head, [rbp-80) clamped budget,
    //        [rbp-88) filtered chunk pointer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // frame for the parse state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the maximum length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the ending-delimiter pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the ending-delimiter length
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor through StreamState
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the resolved backend descriptor

    // -- does the stream carry a read-filter chain? --
    // See the AArch64 counterpart: probed once, because the answer picks the byte source.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // rax = the stable stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sgl_unfiltered_x86");                          // no state: nothing can be attached to it
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_READ_FILTER_HEAD_OFFSET}]")); // read-direction chain head
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // remember whether the chain exists
    emitter.instruction("jmp __rt_sgl_filter_probed_x86");                      // continue with the reservation
    emitter.label("__rt_sgl_unfiltered_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // an unfiltered stream keeps the descriptor path
    emitter.label("__rt_sgl_filter_probed_x86");

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the line can never exceed the caller's byte budget
    emitter.instruction("xor r8d, r8d");                                        // a negative budget reserves nothing at all
    emitter.instruction("cmp rax, 0");                                          // is the requested budget non-positive?
    emitter.instruction("cmovl rax, r8");                                       // clamp a negative budget to a zero-byte reservation
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // remember the clamped budget for the filtered claim
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the whole budget
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // running total starts at zero
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // nothing consumed yet: the caller sees PHP false

    // -- take back what a previous refusal held on this stream --
    // See the AArch64 counterpart: those bytes carried no delimiter, so they need no scan of
    // their own; the tail comparison runs as each further byte arrives.

    // -- a read-filter chain outranks the backend: __rt_fread pulls through it either way --
    // See the AArch64 counterpart: the filtered path has to CLAIM the reservation, because
    // `__rt_fread` allocates from the same scratch and would otherwise hand back a pointer
    // on top of the line being built.
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // the read-direction chain head
    emitter.instruction("je __rt_sgl_backend_probe_x86");                       // unfiltered: leave the scratch cursor alone
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // claim the whole clamped budget
    emitter.instruction("call __rt_concat_publish");                            // so nested filtered reads append after it
    emitter.instruction("jmp __rt_stream_get_line_loop_x86");                   // filtered: the loop reads through the chain
    emitter.label("__rt_sgl_backend_probe_x86");

    // -- user-wrapper fd: read via stream_read into _user_wrapper_drain_buf --
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the backend descriptor
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jb __rt_stream_get_line_loop_x86");                    // native descriptors use the byte-read loop below
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("add r10, r9");                                         // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp rax, r10");                                        // is the backend above the wrapper range?
    emitter.instruction("jb __rt_sgl_wrapper_entry_x86");                       // wrappers read via the feof-gated stream_read loop below

    emitter.label("__rt_stream_get_line_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // reached the byte budget?
    emitter.instruction("jge __rt_stream_get_line_done_x86");                   // stop at the maximum length

    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // the read-direction chain head
    emitter.instruction("jne __rt_sgl_filtered_byte_x86");                      // filtered: take the byte from the chain
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reserved result start pointer
    emitter.instruction("add rsi, QWORD PTR [rbp - 48]");                       // single-byte write pointer inside the reservation

    // See the AArch64 counterpart: a byte the stream is already HOLDING has to arrive through
    // the same door as a byte off the descriptor, because the delimiter scan lives below it.
    emitter.instruction("mov QWORD PTR [rbp - 96], rsi");                       // the destination this iteration writes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov rdx, 1");                                          // one held byte
    emitter.instruction("call __rt_stream_pending_take");                       // rax = 1 when one came back
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_stream_get_line_read_ok_x86");                // join the shared delimiter scan
    emitter.instruction("mov rsi, QWORD PTR [rbp - 96]");                       // the destination again

    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the resolved backend descriptor
    emitter.instruction("mov rdx, 1");                                          // read exactly one byte
    emitter.instruction("call read");                                           // read one byte through libc read()
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as a byte, EOF, or failure
    emitter.instruction("jg __rt_stream_get_line_read_ok_x86");                 // positive byte count: publish the appended byte
    emitter.instruction("jl __rt_stream_get_line_read_failed_x86");             // negative result: inspect errno before setting EOF
    emitter.instruction("jmp __rt_stream_get_line_eof_x86");                    // zero-byte read means real EOF

    // -- filtered byte source: one byte at a time out of the chain's buffered output --
    // See the AArch64 counterpart. The byte is copied into the claimed window before the
    // chunk is released, and the delimiter scan below is the same code the raw path runs.
    emitter.label("__rt_sgl_filtered_byte_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle, not the descriptor
    emitter.instruction("mov esi, 1");                                          // one filtered byte
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_stream_get_line_eof_x86");                     // the chain is drained and flushed: EOF
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save the chunk ptr across the release calls
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // the filtered byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // the claimed result window
    emitter.instruction("add r10, QWORD PTR [rbp - 48]");                       // the byte's destination inside it
    emitter.instruction("mov BYTE PTR [r10], cl");                              // append it to the line
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand this byte's scratch window back before the next read
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // chunk ptr for release
    emitter.instruction("call __rt_decref_any");                                // release the chunk before continuing the line

    emitter.label("__rt_stream_get_line_read_ok_x86");
    emitter.instruction("inc QWORD PTR [rbp - 48]");                            // count the new byte
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // a byte reached the buffer, so the result is a string

    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // ending-delimiter length
    emitter.instruction("test rcx, rcx");                                       // no delimiter configured?
    emitter.instruction("jz __rt_stream_get_line_loop_x86");                    // keep reading without a delimiter
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, rcx");                                        // enough bytes for a delimiter match?
    emitter.instruction("jl __rt_stream_get_line_loop_x86");                    // not yet: keep reading
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // result start pointer
    emitter.instruction("mov r9, rax");                                         // running total
    emitter.instruction("sub r9, rcx");                                         // offset of the candidate tail
    emitter.instruction("add r8, r9");                                          // pointer to the candidate tail
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // ending-delimiter pointer
    emitter.instruction("xor esi, esi");                                        // delimiter comparison index
    emitter.label("__rt_stream_get_line_cmp_x86");
    emitter.instruction("cmp rsi, rcx");                                        // compared every delimiter byte?
    emitter.instruction("jge __rt_stream_get_line_matched_x86");                // a full match ends the line
    emitter.instruction("movzx edi, BYTE PTR [r8 + rsi]");                      // a tail byte
    emitter.instruction("movzx edx, BYTE PTR [r10 + rsi]");                     // the matching delimiter byte
    emitter.instruction("cmp edi, edx");                                        // do they differ?
    emitter.instruction("jne __rt_stream_get_line_loop_x86");                   // mismatch: keep reading
    emitter.instruction("inc rsi");                                             // advance the comparison
    emitter.instruction("jmp __rt_stream_get_line_cmp_x86");                    // compare the next delimiter byte

    emitter.label("__rt_stream_get_line_matched_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("sub rax, rcx");                                        // drop the delimiter from the result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // store the stripped total
    emitter.instruction("jmp __rt_stream_get_line_done_x86");                   // a delimiter match is not EOF

    emitter.label("__rt_stream_get_line_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("jne __rt_sgl_not_would_block_x86");

    // -- nothing more to read RIGHT NOW: php keeps the partial line and answers false --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // what the line has so far
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_stream_get_line_done_x86");                    // nothing gathered: already false
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // the bytes to give back
    emitter.instruction("call __rt_stream_pending_put");                        // they stay ON the stream
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // the caller receives nothing
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // which php spells false
    emitter.instruction("jmp __rt_stream_get_line_done_x86");
    emitter.label("__rt_sgl_not_would_block_x86");

    // -- user-wrapper line read: feof-gated stream_read into _user_wrapper_drain_buf
    //    (a SEPARATE buffer from _concat_buf, which each __rt_fread result may
    //    occupy). [rbp-40] = drain-buf base, [rbp-48] = running length. --
    emitter.label("__rt_sgl_wrapper_entry_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_drain_buf");        // drain-buf base
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // result start = drain-buf base
    emitter.label("__rt_sgl_wrapper_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // reached the byte budget?
    emitter.instruction("jge __rt_stream_get_line_done_x86");                   // stop at the maximum length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    super::feof::emit_feof_call(emitter, true);                                 // elephc's own probe: never warns, the read does
    emitter.instruction("test rax, rax");                                       // at EOF?
    emitter.instruction("jnz __rt_stream_get_line_done_x86");                   // at EOF: return the bytes gathered so far
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov rsi, 1");                                          // read exactly one byte
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");                                       // zero-length read?
    emitter.instruction("jz __rt_stream_get_line_done_x86");                    // defensive: empty read also ends the line
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the read byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // current running total
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // drain-buf base
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // append the byte to the line buffer
    emitter.instruction("inc r10");                                             // advance the running total
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // store the updated total
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // a byte reached the buffer, so the result is a string
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand this chunk's scratch window back before the next read
    emitter.instruction("call __rt_decref_any");                                // release the owned chunk (rax = chunk ptr)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // ending-delimiter length
    emitter.instruction("test rcx, rcx");                                       // no delimiter configured?
    emitter.instruction("jz __rt_sgl_wrapper_loop_x86");                        // keep reading without a delimiter
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, rcx");                                        // enough bytes for a delimiter match?
    emitter.instruction("jl __rt_sgl_wrapper_loop_x86");                        // not yet: keep reading
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // drain-buf base
    emitter.instruction("mov r9, rax");                                         // running total
    emitter.instruction("sub r9, rcx");                                         // offset of the candidate tail
    emitter.instruction("add r8, r9");                                          // pointer to the candidate tail
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // ending-delimiter pointer
    emitter.instruction("xor esi, esi");                                        // delimiter comparison index
    emitter.label("__rt_sgl_wrapper_cmp_x86");
    emitter.instruction("cmp rsi, rcx");                                        // compared every delimiter byte?
    emitter.instruction("jge __rt_sgl_wrapper_matched_x86");                    // a full match ends the line
    emitter.instruction("movzx edi, BYTE PTR [r8 + rsi]");                      // a tail byte
    emitter.instruction("movzx edx, BYTE PTR [r10 + rsi]");                     // the matching delimiter byte
    emitter.instruction("cmp edi, edx");                                        // do they differ?
    emitter.instruction("jne __rt_sgl_wrapper_loop_x86");                       // mismatch: keep reading
    emitter.instruction("inc rsi");                                             // advance the comparison
    emitter.instruction("jmp __rt_sgl_wrapper_cmp_x86");                        // compare the next delimiter byte
    emitter.label("__rt_sgl_wrapper_matched_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("sub rax, rcx");                                        // drop the delimiter from the result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // store the stripped total
    emitter.instruction("jmp __rt_stream_get_line_done_x86");                   // a delimiter match is not EOF

    emitter.label("__rt_stream_get_line_eof_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov esi, 1");                                          // publish the EOF state
    emitter.instruction("call __rt_stream_eof_set");                            // update only this stream's stable state

    emitter.label("__rt_stream_get_line_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the result start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // return the bytes read
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results

    // -- `php://temp` calls a drained line read the end of the stream --
    // See the AArch64 counterpart.
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // the result survives the probe's own calls
    emitter.instruction("mov QWORD PTR [rbp - 104], rdx");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_temp_eof_probe");                     // one extra read, for the one wrapper that takes it
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // restore the result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 104]");                      // restore the result length

    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // report whether ANY byte was consumed, after the publish call
    emitter.instruction("add rsp, 112");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the line slice
}
