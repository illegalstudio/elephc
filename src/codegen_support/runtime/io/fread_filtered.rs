//! Purpose:
//! Emits the filtered-read path: `__rt_fread` wraps the raw read with a per-stream buffer so a
//! read filter's output is capped at the requested length, its remainder kept, and the filter
//! flushed when the stream ends.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - A read filter does not emit one byte per byte consumed, so what it produces cannot be handed
//!   straight back: PHP caps a filtered `fread($h, $n)` at `$n` and keeps the rest on the stream.
//!   The raw path has no buffer, so it returned everything the filter emitted, and a filter that
//!   answered `PSFS_FEED_ME` — producing nothing yet — had its raw, UNFILTERED input passed
//!   through to the caller instead.
//! - The loop re-reads and dispatches again while the filter withholds output, which is what
//!   `PSFS_FEED_ME` asks for. Returning the short read instead would turn the leak into silent
//!   data loss, because callers stop on the first empty chunk.
//! - End of input raises `_user_filter_closing` and runs the chain once with an empty input. That
//!   is PHP's closing dispatch, and the bytes it emits reach the reader like any others.
//! - The served bytes are copied into concat-reserved storage, so `__rt_fread`'s contract with its
//!   callers is unchanged: they never hold a pointer into the stream's own buffer.
//! - Streams with no read-filter chain tail-call the raw helper, leaving that hot path untouched.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

use crate::codegen_support::runtime::resources::layout::{
    STREAM_EOF_OFFSET, STREAM_FILTERED_BUF_CAP_OFFSET, STREAM_FILTERED_BUF_LEN_OFFSET,
    STREAM_FILTERED_BUF_POS_OFFSET, STREAM_FILTERED_BUF_PTR_OFFSET, STREAM_FILTERED_FLUSHED_OFFSET,
    STREAM_FILTERED_POS_OFFSET, STREAM_READ_FILTER_HEAD_OFFSET,
};

/// Bytes pulled from the backend per dispatch while the filter is still withholding output.
const FILTERED_READ_CHUNK: i64 = 8192;

/// Emits `__rt_stream_filtered_append` and the buffered `__rt_fread` wrapper.
pub fn emit_fread_filtered(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_filtered_append_aarch64(emitter);
            emit_fread_wrapper_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_filtered_append_x86_64(emitter);
            emit_fread_wrapper_x86_64(emitter);
        }
    }
}

/// `__rt_stream_filtered_append(x0 = handle, x1 = bytes, x2 = length)`.
///
/// Appends filtered bytes to the stream's buffer, growing it when needed. A zero length, a stale
/// handle, or a failed allocation leaves the buffer untouched.
fn emit_filtered_append_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: append to a stream's filtered-read buffer ---");
    emitter.label_global("__rt_stream_filtered_append");
    // Frame: [0]=bytes [8]=length [16]=state [24]=working buffer [32]=new capacity,
    // with the saved pair at [48]. The spill slots must stay clear of that pair: writing the
    // capacity at [40] would have landed on the saved link register.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the append frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the source pointer
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the byte count
    emitter.instruction("cbz x2, __rt_sfa_done");                               // nothing produced: leave the buffer alone
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_sfa_done");                               // a stale handle has no buffer to grow
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the state pointer

    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes already held
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_FILTERED_BUF_CAP_OFFSET}]")); // allocated capacity
    emitter.instruction("ldr x11, [sp, #8]");                                   // incoming byte count
    emitter.instruction("add x12, x9, x11");                                    // capacity the append needs
    emitter.instruction("cmp x12, x10");                                        // does it already fit?
    emitter.instruction("b.le __rt_sfa_copy");                                  // room enough: copy straight in

    // -- grow: allocate double the needed size so repeated appends stay amortized --
    emitter.instruction(&format!("ldr x13, [x0, #{STREAM_FILTERED_BUF_PTR_OFFSET}]")); // the buffer being replaced
    emitter.instruction("str x13, [sp, #24]");                                  // preserve it for the copy and the free
    emitter.instruction("lsl x0, x12, #1");                                     // request twice the needed capacity
    emitter.instruction("add x0, x0, #64");                                     // and a floor, so a tiny first append still allocates usefully
    emitter.instruction("mov x14, x0");                                         // remember the capacity being requested
    emitter.instruction("str x14, [sp, #32]");                                  // preserve it across the allocation call
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = the new buffer
    emitter.instruction("cbz x0, __rt_sfa_done");                               // out of memory: drop the append rather than corrupt the buffer
    emitter.instruction("mov x15, x0");                                         // the new buffer base
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the state pointer
    emitter.instruction(&format!("ldr x9, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes to carry over
    emitter.instruction("ldr x13, [sp, #24]");                                  // the old buffer
    emitter.instruction("mov x16, #0");                                         // copy index
    emitter.label("__rt_sfa_grow_copy");
    emitter.instruction("cmp x16, x9");                                         // carried every held byte?
    emitter.instruction("b.hs __rt_sfa_grow_done");                             // the carry-over is complete
    emitter.instruction("ldrb w17, [x13, x16]");                                // read one held byte
    emitter.instruction("strb w17, [x15, x16]");                                // write it into the new buffer
    emitter.instruction("add x16, x16, #1");                                    // advance the copy index
    emitter.instruction("b __rt_sfa_grow_copy");                                // continue carrying over
    emitter.label("__rt_sfa_grow_done");
    emitter.instruction("cbz x13, __rt_sfa_grow_publish");                      // the first append has no old buffer to release
    emitter.instruction("mov x0, x13");                                         // free the replaced buffer
    emitter.instruction("str x15, [sp, #24]");                                  // preserve the new base across the free
    emitter.instruction("bl __rt_heap_free");
    emitter.instruction("ldr x15, [sp, #24]");                                  // reload the new base
    emitter.label("__rt_sfa_grow_publish");
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the state pointer
    emitter.instruction(&format!("str x15, [x1, #{STREAM_FILTERED_BUF_PTR_OFFSET}]")); // publish the new buffer
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the requested capacity
    emitter.instruction(&format!("str x14, [x1, #{STREAM_FILTERED_BUF_CAP_OFFSET}]")); // publish the new capacity

    emitter.label("__rt_sfa_copy");
    emitter.instruction("ldr x1, [sp, #16]");                                   // state pointer
    emitter.instruction(&format!("ldr x2, [x1, #{STREAM_FILTERED_BUF_PTR_OFFSET}]")); // destination buffer
    emitter.instruction(&format!("ldr x9, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // current write offset
    // Fold the write offset into the base so the loop needs no third register: x9..x17 are the
    // only scratch available here, and x18 is the OS platform register on Apple AArch64.
    emitter.instruction("add x2, x2, x9");                                      // the append lands here
    emitter.instruction("ldr x3, [sp, #0]");                                    // source pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // byte count
    emitter.instruction("mov x16, #0");                                         // copy index
    emitter.label("__rt_sfa_append_byte");
    emitter.instruction("cmp x16, x11");                                        // appended every byte?
    emitter.instruction("b.hs __rt_sfa_append_done");                           // the append is complete
    emitter.instruction("ldrb w17, [x3, x16]");                                 // read one produced byte
    emitter.instruction("strb w17, [x2, x16]");                                 // store it into the buffer
    emitter.instruction("add x16, x16, #1");                                    // advance the copy index
    emitter.instruction("b __rt_sfa_append_byte");                              // continue appending
    emitter.label("__rt_sfa_append_done");
    emitter.instruction("add x9, x9, x11");                                     // the buffer now holds this much
    emitter.instruction(&format!("str x9, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // publish the new length

    emitter.label("__rt_sfa_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the append frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_fread(x0 = handle, x1 = length) -> x1 = pointer, x2 = length`.
///
/// The buffered wrapper. Streams without a read-filter chain tail-call `__rt_fread_raw`.
fn emit_fread_wrapper_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread (filtered, buffered) ---");
    emitter.label_global("__rt_fread");
    // Frame: [0]=handle [8]=requested length [16]=state [24]=served count [32]=served
    // destination, with the saved pair at [48] and clear of every spill slot.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the wrapper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the requested byte count
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_freadb_passthrough");                     // no live state: the raw helper reports the error
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_READ_FILTER_HEAD_OFFSET}]")); // read-direction chain head
    emitter.instruction("cbnz x9, __rt_freadb_have_chain");                     // a filtered stream takes the normal path
    // No chain — but `stream_filter_remove()` LEAVES the bytes the filter had already produced on
    // the stream, and php serves them. Asking "is a filter attached?" made those bytes
    // unreachable the moment the filter went: `fread()` answered "" where php answers the
    // remainder.
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("cmp x11, x10");
    // Drained, and the filter that produced these bytes is gone — so nothing more can ever come
    // from it, which is exactly what the flushed flag asserts. Without setting it here `feof()`
    // would hold false forever on a stream whose reader has already seen everything.
    emitter.instruction("b.lo __rt_freadb_hold");
    emitter.instruction("mov x10, #1");
    emitter.instruction(&format!("str x10, [x0, #{STREAM_FILTERED_FLUSHED_OFFSET}]"));
    emitter.instruction("b __rt_freadb_passthrough");                           // nothing held: the raw path is untouched
    emitter.label("__rt_freadb_hold");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the state pointer
    emitter.instruction("b __rt_freadb_serve");                                 // serve the remainder php keeps
    emitter.label("__rt_freadb_have_chain");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the state pointer
    // php answers `false` — not `""` — when a read filter reports PSFS_ERR_FATAL, so the
    // code the brigade publishes has to survive to the return below. It is reset to
    // PSFS_PASS_ON first because the symbol lives in BSS and starts at ZERO, which IS
    // PSFS_ERR_FATAL: without this an ordinary EOF on the very first filtered read would
    // look like a filter failure.
    abi::emit_symbol_address(emitter, "x9", "_user_filter_last_psfs");
    emitter.instruction("mov x10, #2");                                         // PSFS_PASS_ON
    emitter.instruction("str x10, [x9]");

    emitter.label("__rt_freadb_fill");
    emitter.instruction("ldr x1, [sp, #16]");                                   // state pointer
    emitter.instruction(&format!("ldr x9, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("ldr x10, [x1, #{STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("sub x11, x9, x10");                                    // bytes available to serve
    emitter.instruction("ldr x12, [sp, #8]");                                   // the requested byte count
    emitter.instruction("cmp x11, x12");                                        // is the request already satisfied?
    emitter.instruction("b.hs __rt_freadb_serve");                              // serve from the buffer
    emitter.instruction(&format!("ldr x13, [x1, #{STREAM_FILTERED_FLUSHED_OFFSET}]")); // has the closing dispatch run?
    emitter.instruction("cbnz x13, __rt_freadb_serve");                         // the stream is finished: serve what is left
    // Read the backend's EOF word straight from the state rather than through
    // `__rt_stream_eof_get`: that helper answers PHP's `feof()`, which this buffer deliberately
    // holds false while bytes are still owed to the reader.
    emitter.instruction(&format!("ldr x14, [x1, #{STREAM_EOF_OFFSET}]"));       // is the backend exhausted?
    emitter.instruction("cbnz x14, __rt_freadb_flush");                         // yes: give the filter its closing dispatch

    // -- pull another chunk and dispatch the chain over it --
    emitter.instruction("ldr x0, [sp, #0]");                                    // the stream handle
    emitter.instruction(&format!("mov x1, #{FILTERED_READ_CHUNK}"));            // one backend chunk
    emitter.instruction("bl __rt_fread_raw");                                   // x1/x2 = the raw bytes
    emitter.instruction("cbz x2, __rt_freadb_flush");                           // a zero-byte read ends the input
    emitter.instruction("ldr x0, [sp, #0]");                                    // the stream handle
    emitter.instruction(&format!("mov x3, #{STREAM_READ_FILTER_HEAD_OFFSET}")); // select the read-direction chain
    emitter.instruction("bl __rt_stream_apply_filter_chain");                   // x1/x2 = what the filters produced
    emitter.instruction("ldr x0, [sp, #0]");                                    // the stream handle
    emitter.instruction("bl __rt_stream_filtered_append");                      // park it on the stream
    emitter.instruction("b __rt_freadb_fill");                                  // a filter that withheld output gets fed again

    // -- end of input: PHP gives the chain one `filter(..., $closing = true)` dispatch --
    emitter.label("__rt_freadb_flush");
    emitter.instruction("ldr x1, [sp, #16]");                                   // state pointer
    emitter.instruction("mov x9, #1");
    emitter.instruction(&format!("str x9, [x1, #{STREAM_FILTERED_FLUSHED_OFFSET}]")); // exactly one closing dispatch
    abi::emit_symbol_address(emitter, "x10", "_user_filter_closing");
    emitter.instruction("str x9, [x10]");                                       // the brigade reads this as `$closing`
    emitter.instruction("ldr x0, [sp, #0]");                                    // the stream handle
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");              // an empty input: the flush feeds no bytes
    emitter.instruction("mov x2, #0");                                          // length 0
    emitter.instruction(&format!("mov x3, #{STREAM_READ_FILTER_HEAD_OFFSET}")); // the read-direction chain
    emitter.instruction("bl __rt_stream_apply_filter_chain");                   // x1/x2 = the flushed bytes
    abi::emit_symbol_address(emitter, "x10", "_user_filter_closing");
    emitter.instruction("str xzr, [x10]");                                      // lower the flag again immediately
    emitter.instruction("ldr x0, [sp, #0]");                                    // the stream handle
    emitter.instruction("bl __rt_stream_filtered_append");                      // park the flushed bytes too

    // -- serve at most the requested count, keeping the remainder --
    emitter.label("__rt_freadb_serve");
    emitter.instruction("ldr x1, [sp, #16]");                                   // state pointer
    emitter.instruction(&format!("ldr x9, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("ldr x10, [x1, #{STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("sub x11, x9, x10");                                    // bytes available
    emitter.instruction("ldr x12, [sp, #8]");                                   // the requested byte count
    emitter.instruction("cmp x11, x12");                                        // more available than asked for?
    emitter.instruction("csel x11, x12, x11, hs");                              // serve at most the requested count
    emitter.instruction("cbz x11, __rt_freadb_empty");                          // nothing left: report an empty read
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the served count
    emitter.instruction("mov x0, x11");                                         // reserve caller-visible storage for it
    emitter.instruction("bl __rt_concat_reserve");                              // x0 = destination the caller may own
    emitter.instruction("str x0, [sp, #32]");                                   // the destination must survive the publish call below
    emitter.instruction("mov x15, x0");                                         // the destination base
    emitter.instruction("ldr x1, [sp, #16]");                                   // state pointer
    emitter.instruction(&format!("ldr x2, [x1, #{STREAM_FILTERED_BUF_PTR_OFFSET}]")); // the filtered buffer
    emitter.instruction(&format!("ldr x10, [x1, #{STREAM_FILTERED_BUF_POS_OFFSET}]")); // the read cursor
    // Fold the cursor into the base, as the append loop does, so the copy needs no third
    // register: x18 is the OS platform register on Apple AArch64 and x19 upward are callee-saved.
    emitter.instruction("add x2, x2, x10");                                     // the served bytes start here
    emitter.instruction("ldr x11, [sp, #24]");                                  // the served count
    emitter.instruction("mov x16, #0");                                         // copy index
    emitter.label("__rt_freadb_serve_byte");
    emitter.instruction("cmp x16, x11");                                        // served every byte?
    emitter.instruction("b.hs __rt_freadb_serve_done");                         // the copy is complete
    emitter.instruction("ldrb w17, [x2, x16]");                                 // read one buffered byte
    emitter.instruction("strb w17, [x15, x16]");                                // write it into the caller's storage
    emitter.instruction("add x16, x16, #1");                                    // advance the copy index
    emitter.instruction("b __rt_freadb_serve_byte");                            // continue serving
    emitter.label("__rt_freadb_serve_done");
    emitter.instruction("ldr x1, [sp, #32]");                                   // publish the reserved window
    emitter.instruction("ldr x2, [sp, #24]");                                   // for exactly the served count
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset
    emitter.instruction("ldr x1, [sp, #16]");                                   // state pointer
    emitter.instruction(&format!("ldr x9, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("ldr x10, [x1, #{STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("ldr x11, [sp, #24]");                                  // the served count
    emitter.instruction("add x10, x10, x11");                                   // advance the read cursor
    emitter.instruction("cmp x10, x9");                                         // did that drain the buffer?
    emitter.instruction("b.ne __rt_freadb_keep");                               // a remainder stays for the next read
    emitter.instruction("mov x10, #0");                                         // drained: rewind the cursor
    emitter.instruction(&format!("str xzr, [x1, #{STREAM_FILTERED_BUF_LEN_OFFSET}]")); // and reuse the buffer from its base
    emitter.label("__rt_freadb_keep");
    emitter.instruction(&format!("str x10, [x1, #{STREAM_FILTERED_BUF_POS_OFFSET}]")); // publish the cursor
    // php's `ftell()` counts the bytes HANDED TO THE CALLER, never the ones pulled from the
    // descriptor: this loop reads whole chunks so the filter has something to work on, so the
    // descriptor is far ahead of what the reader has seen. The cursor above cannot stand in for
    // it — it rewinds every time the buffer drains.
    emitter.instruction(&format!("ldr x12, [x1, #{STREAM_FILTERED_POS_OFFSET}]")); // the position php reports
    emitter.instruction("add x12, x12, x11");                                   // advance it by the bytes just served
    emitter.instruction(&format!("str x12, [x1, #{STREAM_FILTERED_POS_OFFSET}]")); // publish php's position
    emitter.instruction("ldr x1, [sp, #32]");                                         // return the caller's storage
    emitter.instruction("ldr x2, [sp, #24]");                                   // and the served count
    emitter.instruction("mov x0, #1");                                          // served bytes are a real result
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return the filtered bytes

    emitter.label("__rt_freadb_empty");
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");              // a readable pointer for a zero-length result
    emitter.instruction("mov x2, #0");                                          // the stream is exhausted
    emitter.instruction("mov x0, #1");                                          // exhausted is not failed: php answers "" here
    // ...unless a filter REFUSED the data. php separates the two: an exhausted filtered stream
    // answers `""`, one whose filter returned PSFS_ERR_FATAL answers `false`.
    abi::emit_symbol_address(emitter, "x9", "_user_filter_last_psfs");
    emitter.instruction("ldr x9, [x9]");                                        // the code the last dispatch published
    emitter.instruction("cmp x9, #0");                                          // PSFS_ERR_FATAL
    emitter.instruction("csel x0, xzr, x0, eq");                                // a refused read is php's false
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return the empty read

    emitter.label("__rt_freadb_passthrough");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the stream handle
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the requested byte count
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame before the tail call
    emitter.instruction("add sp, sp, #64");                                     // release the wrapper frame
    emitter.instruction("b __rt_fread_raw");                                    // unfiltered streams keep the raw path
}

/// x86_64 form of [`emit_filtered_append_aarch64`].
///
/// `__rt_stream_filtered_append(rdi = handle, rax = bytes, rdx = length)`.
fn emit_filtered_append_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: append to a stream's filtered-read buffer ---");
    emitter.label_global("__rt_stream_filtered_append");
    // Frame: [rbp-8]=bytes [rbp-16]=length [rbp-24]=state [rbp-32]=old buffer [rbp-40]=new capacity
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the append spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the byte count
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_sfa_done_x");                                  // nothing produced: leave the buffer alone
    emitter.instruction("call __rt_stream_state");                              // rax = the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sfa_done_x");                                  // a stale handle has no buffer to grow
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the state pointer

    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes already held
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {STREAM_FILTERED_BUF_CAP_OFFSET}]")); // allocated capacity
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // incoming byte count
    emitter.instruction("lea rcx, [r9 + r11]");                                 // capacity the append needs
    emitter.instruction("cmp rcx, r10");                                        // does it already fit?
    emitter.instruction("jle __rt_sfa_copy_x");                                 // room enough: copy straight in

    emitter.instruction(&format!("mov r8, QWORD PTR [rax + {STREAM_FILTERED_BUF_PTR_OFFSET}]")); // the buffer being replaced
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // preserve it for the copy and the free
    emitter.instruction("lea rcx, [rcx + rcx]");                                // twice the needed capacity
    emitter.instruction("add rcx, 64");                                         // and a floor for a tiny first append
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the requested capacity
    emitter.instruction("mov rax, rcx");                                        // __rt_heap_alloc takes its size in rax
    emitter.instruction("call __rt_heap_alloc");                                // rax = the new buffer
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sfa_done_x");                                  // out of memory: drop the append
    emitter.instruction("mov r15, rax");                                        // the new buffer base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the state pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes to carry over
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // the old buffer
    emitter.instruction("xor rsi, rsi");                                        // copy index
    emitter.label("__rt_sfa_grow_copy_x");
    emitter.instruction("cmp rsi, r9");                                         // carried every held byte?
    emitter.instruction("jae __rt_sfa_grow_done_x");                            // the carry-over is complete
    emitter.instruction("movzx edi, BYTE PTR [r8 + rsi]");                      // read one held byte
    emitter.instruction("mov BYTE PTR [r15 + rsi], dil");                       // write it into the new buffer
    emitter.instruction("add rsi, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_sfa_grow_copy_x");                            // continue carrying over
    emitter.label("__rt_sfa_grow_done_x");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // the old buffer
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_sfa_grow_publish_x");                          // the first append has none to release
    emitter.instruction("mov rax, r8");                                         // __rt_heap_free takes its pointer in rax
    emitter.instruction("call __rt_heap_free");
    emitter.label("__rt_sfa_grow_publish_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the state pointer
    emitter.instruction(&format!("mov QWORD PTR [rcx + {STREAM_FILTERED_BUF_PTR_OFFSET}], r15")); // publish the new buffer
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the requested capacity
    emitter.instruction(&format!("mov QWORD PTR [rcx + {STREAM_FILTERED_BUF_CAP_OFFSET}], r10")); // publish the new capacity

    emitter.label("__rt_sfa_copy_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // state pointer
    emitter.instruction(&format!("mov r8, QWORD PTR [rcx + {STREAM_FILTERED_BUF_PTR_OFFSET}]")); // destination buffer
    emitter.instruction(&format!("mov r9, QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}]")); // current write offset
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // source pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // byte count
    emitter.instruction("xor rsi, rsi");                                        // copy index
    emitter.label("__rt_sfa_append_byte_x");
    emitter.instruction("cmp rsi, r11");                                        // appended every byte?
    emitter.instruction("jae __rt_sfa_append_done_x");                          // the append is complete
    emitter.instruction("movzx edi, BYTE PTR [r10 + rsi]");                     // read one produced byte
    emitter.instruction("lea rax, [r9 + rsi]");                                 // its destination offset
    emitter.instruction("mov BYTE PTR [r8 + rax], dil");                        // store it into the buffer
    emitter.instruction("add rsi, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_sfa_append_byte_x");                          // continue appending
    emitter.label("__rt_sfa_append_done_x");
    emitter.instruction("add r9, r11");                                         // the buffer now holds this much
    emitter.instruction(&format!("mov QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}], r9")); // publish the new length

    emitter.label("__rt_sfa_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 form of [`emit_fread_wrapper_aarch64`].
///
/// `__rt_fread(rdi = handle, rsi = length) -> rax = pointer, rdx = length`.
fn emit_fread_wrapper_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread (filtered, buffered) ---");
    emitter.label_global("__rt_fread");
    // Frame: [rbp-8]=handle [rbp-16]=requested length [rbp-24]=state [rbp-32]=served count
    //        [rbp-40]=served destination
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the wrapper spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested byte count
    emitter.instruction("call __rt_stream_state");                              // rax = the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_freadb_passthrough_x");                        // no live state: the raw helper reports it
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_READ_FILTER_HEAD_OFFSET}]")); // read-direction chain head
    emitter.instruction("test r9, r9");
    emitter.instruction("jnz __rt_freadb_have_chain_x");                        // a filtered stream takes the normal path
    // See the AArch64 arm: `stream_filter_remove()` LEAVES the bytes the filter had already
    // produced on the stream, and php serves them.
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_FILTERED_BUF_LEN_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_FILTERED_BUF_POS_OFFSET}]"
    ));
    emitter.instruction("cmp r11, r10");
    // See the AArch64 arm: drained plus a removed filter means nothing more can come.
    emitter.instruction("jb __rt_freadb_hold_x");
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_FILTERED_FLUSHED_OFFSET}], 1"
    ));
    emitter.instruction("jmp __rt_freadb_passthrough_x");                       // nothing held: the raw path is untouched
    emitter.label("__rt_freadb_hold_x");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the state pointer
    emitter.instruction("jmp __rt_freadb_serve_x");                             // serve the remainder php keeps
    emitter.label("__rt_freadb_have_chain_x");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the state pointer
    // See the AArch64 arm: the published PSFS code has to survive to the return, and the
    // BSS symbol starts at zero, which is PSFS_ERR_FATAL.
    abi::emit_symbol_address(emitter, "r9", "_user_filter_last_psfs");
    emitter.instruction("mov QWORD PTR [r9], 2");                              // PSFS_PASS_ON

    emitter.label("__rt_freadb_fill_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // state pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("mov r10, QWORD PTR [rcx + {STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("sub r9, r10");                                         // bytes available to serve
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // is the request already satisfied?
    emitter.instruction("jae __rt_freadb_serve_x");                             // serve from the buffer
    emitter.instruction(&format!("mov r11, QWORD PTR [rcx + {STREAM_FILTERED_FLUSHED_OFFSET}]")); // has the closing dispatch run?
    emitter.instruction("test r11, r11");
    emitter.instruction("jnz __rt_freadb_serve_x");                             // the stream is finished: serve what is left
    // See the AArch64 counterpart: read the backend's EOF word directly, because
    // `__rt_stream_eof_get` answers PHP's `feof()`, which this buffer holds false while bytes
    // are still owed to the reader.
    emitter.instruction(&format!("mov r11, QWORD PTR [rcx + {STREAM_EOF_OFFSET}]")); // is the backend exhausted?
    emitter.instruction("test r11, r11");
    emitter.instruction("jnz __rt_freadb_flush_x");                             // yes: give the filter its closing dispatch

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the stream handle
    emitter.instruction(&format!("mov rsi, {FILTERED_READ_CHUNK}"));            // one backend chunk
    emitter.instruction("call __rt_fread_raw");                                 // rax/rdx = the raw bytes
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_freadb_flush_x");                              // a zero-byte read ends the input
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the stream handle
    emitter.instruction(&format!("mov rsi, {STREAM_READ_FILTER_HEAD_OFFSET}")); // select the read-direction chain
    emitter.instruction("call __rt_stream_apply_filter_chain");                 // rax/rdx = what the filters produced
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the stream handle
    emitter.instruction("call __rt_stream_filtered_append");                    // park it on the stream
    emitter.instruction("jmp __rt_freadb_fill_x");                              // a filter that withheld output gets fed again

    emitter.label("__rt_freadb_flush_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // state pointer
    emitter.instruction(&format!("mov QWORD PTR [rcx + {STREAM_FILTERED_FLUSHED_OFFSET}], 1")); // exactly one closing dispatch
    abi::emit_symbol_address(emitter, "r10", "_user_filter_closing");
    emitter.instruction("mov QWORD PTR [r10], 1");                              // the brigade reads this as `$closing`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the stream handle
    abi::emit_symbol_address(emitter, "rax", "_stream_filter_buf");             // an empty input: the flush feeds no bytes
    emitter.instruction("xor edx, edx");                                        // length 0
    emitter.instruction(&format!("mov rsi, {STREAM_READ_FILTER_HEAD_OFFSET}")); // the read-direction chain
    emitter.instruction("call __rt_stream_apply_filter_chain");                 // rax/rdx = the flushed bytes
    abi::emit_symbol_address(emitter, "r10", "_user_filter_closing");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // lower the flag again immediately
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the stream handle
    emitter.instruction("call __rt_stream_filtered_append");                    // park the flushed bytes too

    emitter.label("__rt_freadb_serve_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // state pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("mov r10, QWORD PTR [rcx + {STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("sub r9, r10");                                         // bytes available
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // the requested byte count
    emitter.instruction("cmp r9, r11");                                         // more available than asked for?
    emitter.instruction("cmovae r9, r11");                                      // serve at most the requested count
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_freadb_empty_x");                              // nothing left: report an empty read
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // preserve the served count
    emitter.instruction("mov rax, r9");                                         // __rt_concat_reserve takes its size in rax
    emitter.instruction("call __rt_concat_reserve");                            // rax = destination the caller may own
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the destination
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // state pointer
    emitter.instruction(&format!("mov r8, QWORD PTR [rcx + {STREAM_FILTERED_BUF_PTR_OFFSET}]")); // the filtered buffer
    emitter.instruction(&format!("mov r10, QWORD PTR [rcx + {STREAM_FILTERED_BUF_POS_OFFSET}]")); // the read cursor
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // the served count
    emitter.instruction("xor rsi, rsi");                                        // copy index
    emitter.label("__rt_freadb_serve_byte_x");
    emitter.instruction("cmp rsi, r11");                                        // served every byte?
    emitter.instruction("jae __rt_freadb_serve_done_x");                        // the copy is complete
    emitter.instruction("lea rdi, [r10 + rsi]");                                // source offset inside the buffer
    emitter.instruction("movzx edi, BYTE PTR [r8 + rdi]");                      // read one buffered byte
    emitter.instruction("mov BYTE PTR [rax + rsi], dil");                       // write it into the caller's storage
    emitter.instruction("add rsi, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_freadb_serve_byte_x");                        // continue serving
    emitter.label("__rt_freadb_serve_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // publish the reserved window
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // for exactly the served count
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // state pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}]")); // bytes held
    emitter.instruction(&format!("mov r10, QWORD PTR [rcx + {STREAM_FILTERED_BUF_POS_OFFSET}]")); // bytes already served
    emitter.instruction("add r10, QWORD PTR [rbp - 32]");                       // advance the read cursor
    emitter.instruction("cmp r10, r9");                                         // did that drain the buffer?
    emitter.instruction("jne __rt_freadb_keep_x");                              // a remainder stays for the next read
    emitter.instruction("xor r10, r10");                                        // drained: rewind the cursor
    emitter.instruction(&format!("mov QWORD PTR [rcx + {STREAM_FILTERED_BUF_LEN_OFFSET}], 0")); // and reuse the buffer from its base
    emitter.label("__rt_freadb_keep_x");
    emitter.instruction(&format!("mov QWORD PTR [rcx + {STREAM_FILTERED_BUF_POS_OFFSET}], r10")); // publish the cursor
    // See the AArch64 counterpart: php's `ftell()` counts the bytes handed to the caller, and the
    // cursor above rewinds every time the buffer drains, so it cannot stand in for that number.
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // the bytes just served
    emitter.instruction(&format!(
        "add QWORD PTR [rcx + {STREAM_FILTERED_POS_OFFSET}], r9"
    ));                                                                         // advance the position php reports
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the caller's storage
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // and the served count
    emitter.instruction("mov ecx, 1");                                          // served bytes are a real result
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the filtered bytes

    emitter.label("__rt_freadb_empty_x");
    abi::emit_symbol_address(emitter, "rax", "_stream_filter_buf");             // a readable pointer for a zero-length result
    emitter.instruction("xor edx, edx");                                        // the stream is exhausted
    emitter.instruction("mov ecx, 1");                                          // exhausted is not failed: php answers "" here
    // ...unless a filter REFUSED the data — see the AArch64 arm.
    abi::emit_symbol_address(emitter, "r9", "_user_filter_last_psfs");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // the code the last dispatch published
    emitter.instruction("xor r10d, r10d");
    emitter.instruction("cmp r9, 0");                                           // PSFS_ERR_FATAL
    emitter.instruction("cmove rcx, r10");                                      // a refused read is php's false
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty read

    emitter.label("__rt_freadb_passthrough_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested byte count
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_fread_raw");                                  // unfiltered streams keep the raw path
}
