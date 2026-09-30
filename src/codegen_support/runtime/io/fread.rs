//! Purpose:
//! Emits `__rt_fread_raw`, the unbuffered read behind `fread()`.
//!
//! `__rt_fread` itself lives in `fread_filtered`: it wraps this helper with the per-stream
//! filtered-read buffer PHP keeps, and tail-calls straight here when the stream has no read
//! filter chain. This helper therefore does NOT apply that chain — the wrapper does, because it
//! is the only place that can re-read when a filter withholds its output.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native reads resolve their descriptor from an opaque stream handle and
//!   publish EOF on the corresponding `StreamState`, including seekable short reads.
//! - The destination window is reserved through `__rt_concat_reserve` for the FULL requested
//!   read length before the syscall, so an attacker-sized `fread($f, 100000)` lands in owned
//!   heap storage instead of running past the 64 KiB concat scratch into the stream-handle,
//!   exception and heap globals that follow it in BSS.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;
use crate::codegen_support::runtime::resources::layout::{
    STREAM_PENDING_LEN_OFFSET, STREAM_PENDING_POS_OFFSET,
};

/// Emits the `__rt_fread` runtime helper for reading bytes from a stream handle.
///
/// On ARM64: reads into storage reserved by `__rt_concat_reserve`, publishes the bytes read
/// through `__rt_concat_publish`, sets StreamState EOF, and returns (pointer, byte_count)
/// in x1:x2.
///
/// On x86_64: same semantics but uses libc `read()` and returns (pointer, byte_count) in rax:rdx.
///
/// # Inputs
/// - x0/rdi: opaque stream handle
/// - x1/rsi: number of bytes to read
///
/// # Outputs
/// - x1/x86_64 rax: pointer to the bytes read. Concat-scratch-backed (borrowed) when the
///   requested length still fits the shared 64 KiB buffer, heap-backed otherwise.
/// - x2/rdx: actual bytes read (0 on EOF/error)
///
/// # Side effects
/// - Advances `_concat_off` by the actual bytes read, but only for scratch-backed results.
/// - Sets state-owned EOF on zero-byte reads and seekable short reads.
pub fn emit_fread(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fread_linux_x86_64(emitter);
        emit_wrapper_chunked_read(emitter);
        emit_uw_fill_one_chunk(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread_raw");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stream, descriptor, and read-result spill slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- save handle and requested length, then resolve the backend descriptor --
    emitter.instruction("str x0, [sp, #0]");                                    // save the opaque stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length
    // A FAILED read and a legitimate zero-byte read both end with length 0, so length alone
    // cannot tell PHP's `false` from its `""`. Slot 40 carries that distinction and leaves in
    // x0, beside the x1/x2 string pair.
    emitter.instruction("mov x9, #1");
    emitter.instruction("str x9, [sp, #40]");                                   // "this is a real result" — cleared only by an actual read failure
    // Slot 48 is how many of the caller's bytes came out of the stream's own holding area. A
    // request that outruns what is held is topped up FROM THE DESCRIPTOR into the same window,
    // because php serves one `fread()` from its buffer AND the source when the buffer runs out —
    // measured: with 8190 bytes consumed of a 12190-byte file, `fread($h, 100)` answers 100.
    emitter.instruction("str xzr, [sp, #48]");                                  // nothing served from the holding area yet
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor through StreamState
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the resolved backend descriptor
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is the backend below the wrapper range?
    emitter.instruction("b.lo __rt_fread_real_fd");                             // native descriptors continue to the syscall path
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("add x10, x9, x10");                                    // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp x0, x10");                                         // is the backend above the wrapper range?
    emitter.instruction("b.hs __rt_fread_real_fd");                             // non-wrapper synthetic backends stay on the native path
    // -- a wrapper stream's OWN read buffer answers before `stream_read` is called again --
    //
    // php's read buffer is shared by every reader, so an `fgets()` that stopped at its length
    // bound leaves the rest of the chunk for the next `fread()`. The drain below the native path
    // never runs here, because this branch tail-calls away first — so a wrapper stream answered
    // `""` for bytes it was holding. The state is probed rather than drained blind: two loads,
    // and nothing is reserved unless there is something to move.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // x0 = stable stream state, 0 when none
    emitter.instruction("cbz x0, __rt_fread_wrapper_call");                     // no state: nothing can be held
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_LEN_OFFSET}]")); // held byte count
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_POS_OFFSET}]")); // how many were already handed out
    emitter.instruction("subs x9, x9, x10");                                    // what remains
    emitter.instruction("b.le __rt_fread_wrapper_call");                        // nothing held: ask the wrapper
    emitter.instruction("ldr x1, [sp, #8]");                                    // the requested byte count
    emitter.instruction("cmp x1, #1");                                          // a non-positive request moves nothing
    emitter.instruction("b.lt __rt_fread_wrapper_call");

    // -- php tops the holding area up before serving from it --
    //
    // Answering whatever happens to be held turns `fread($h, 4)` into a two-byte answer while the
    // source still has plenty: MEASURED against a wrapper handing back 6 bytes at a time, php says
    // 'abcd', 'efgh', 'ijkl' where this said 'abcd', 'ef', 'ghij'. A short read that looks like
    // data. This runs BEFORE the destination is reserved, because filling reserves a window of its
    // own and holding one across that would be a bet on what the allocator does with the first.
    emitter.label("__rt_fread_held_topup");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");
    emitter.instruction("cbz x0, __rt_fread_held_ready");                       // no state: serve what there is
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("sub x9, x9, x10");                                     // what the area still holds
    emitter.instruction("ldr x10, [sp, #8]");                                   // what the caller asked for
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ge __rt_fread_held_ready");                          // enough held: serve it
    // A chunk of 1 means php does not FILL A CHUNK AT A TIME: php-src's own
    // `stream_set_chunk_size.phpt` says the buffer is skipped, and the measurement agrees — one
    // `fread(10000)` makes ONE wrapper call for the shortfall, where filling by the chunk made
    // 2409. `held` is still in x9 and the request in x10 from the compare just above.
    emitter.instruction("sub x11, x10, x9");                                    // the shortfall, if the chunk is 1
    emitter.instruction("str x11, [sp, #56]");                                  // across the chunk-size call, in the frame's free slot
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("bl __rt_stream_chunk_size");
    emitter.instruction("cmp x0, #1");
    emitter.instruction("ldr x1, [sp, #56]");                                   // the shortfall
    emitter.instruction("mov x9, #0");                                          // 0 = "ask for one chunk"
    emitter.instruction("csel x1, x1, x9, eq");                                 // chunk 1: ask for exactly the shortfall
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("bl __rt_uw_fill_one_chunk");                           // x0 = bytes added
    // php stops filling on a SHORT read, it does not keep asking until satisfied: MEASURED, a
    // source handing back 3 bytes at a time answers `fread($h, 5)` with FOUR — one leftover plus
    // one chunk — not five. Looping here answered five, which no php ever would.
    emitter.instruction("str x0, [sp, #24]");                                   // what that fill added
    emitter.instruction("cbz x0, __rt_fread_held_ready");                       // the source is spent
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("bl __rt_stream_chunk_size");                           // what it was asked for
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("cmp x9, x0");
    emitter.instruction("b.ge __rt_fread_held_topup");                          // a FULL chunk: there may be more
    emitter.label("__rt_fread_held_ready");

    emitter.instruction("ldr x1, [sp, #8]");                                    // the requested byte count
    emitter.instruction("mov x0, x1");                                          // storage for at most the requested count
    emitter.instruction("bl __rt_concat_reserve");
    emitter.instruction("str x0, [sp, #16]");                                   // the destination, and the returned pointer
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("ldr x1, [sp, #16]");                                   // the destination
    emitter.instruction("ldr x2, [sp, #8]");                                    // at most the requested count
    emitter.instruction("bl __rt_stream_pending_take");                         // x0 = how many came back
    emitter.instruction("cbnz x0, __rt_fread_held_ok");                         // the held bytes are the whole result
    emitter.instruction("ldr x1, [sp, #16]");                                   // nothing came back after all: give the
    emitter.instruction("mov x2, #0");                                          // window back before asking the wrapper
    emitter.instruction("bl __rt_concat_publish");
    emitter.instruction("b __rt_fread_wrapper_call");
    emitter.label("__rt_fread_held_ok");
    emitter.instruction("str x0, [sp, #24]");                                   // the result length
    emitter.instruction("ldr x1, [sp, #16]");
    emitter.instruction("mov x2, x0");
    emitter.instruction("bl __rt_concat_publish");                              // claim the window they occupy
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the pair directly: the shared
    emitter.instruction("ldr x2, [sp, #24]");                                   // exit indexes the filter table BY
    emitter.instruction("b __rt_fread_ret");                                    // DESCRIPTOR, which a wrapper fd overruns

    emitter.label("__rt_fread_wrapper_call");
    // php asks its source for a WHOLE CHUNK and keeps what the call did not need, so a request
    // SMALLER than a chunk goes through the chunked reader instead of asking the wrapper for
    // exactly five bytes. A request of a chunk or more already matches php and stays direct —
    // which is also what stops the chunked reader, which re-enters here, from recursing.
    // Only a stream with a STATE can hold the surplus. Callers that drive this helper with the
    // synthetic fd rather than the opaque handle — `readfile()`'s loop does — resolve no state, so
    // chunking would read 8192 bytes, hand them to nothing, and answer none of them.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // x0 = stable stream state, 0 when none
    emitter.instruction("cbz x0, __rt_fread_wrapper_direct");                   // no buffer to keep a surplus in
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_chunk_size");                           // x0 = what php would ask for
    emitter.instruction("ldr x1, [sp, #8]");                                    // the caller's request
    emitter.instruction("cmp x1, x0");
    emitter.instruction("b.ge __rt_fread_wrapper_direct");                      // at least a chunk: ask for exactly it
    emitter.instruction("ldr x0, [sp, #0]");                                    // the handle drives the chunked reader
    emitter.instruction("ldr x1, [sp, #8]");                                    // and the caller's request
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame before tail dispatch
    emitter.instruction("add sp, sp, #80");                                     // release native-read scratch storage
    emitter.instruction("b __rt_uw_fread_chunked");
    emitter.label("__rt_fread_wrapper_direct");
    // php asks its source for ONE CHUNK and no more, however much the caller wanted: with a chunk
    // of 100, `fread($h, 250)` elicits "read with size: 100" and answers 100. The exception is a
    // chunk of 1, where the buffer is skipped and the caller's own count goes straight through.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_chunk_size");                           // x0 = one chunk
    emitter.instruction("ldr x1, [sp, #8]");                                    // the caller's request
    emitter.instruction("cmp x0, #1");
    emitter.instruction("b.eq __rt_fread_wrapper_size_ready");                  // chunk 1: ask for the whole request
    emitter.instruction("cmp x1, x0");
    emitter.instruction("csel x1, x0, x1, gt");                                 // otherwise ask for at most one chunk
    emitter.label("__rt_fread_wrapper_size_ready");
    emitter.instruction("str x1, [sp, #8]");                                    // the size this read really asks for
    emitter.instruction("ldr x0, [sp, #32]");                                   // the wrapper descriptor the probe clobbered
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the requested byte count for stream_read
    emitter.instruction("bl __rt_user_wrapper_fread");                          // x0 = verdict, x1/x2 = the bytes
    // php asks the wrapper whether THAT read reached the end, and remembers the answer; see
    // `emit_uw_post_read_eof`. The tail call becomes a call so the question can be asked after it.
    emitter.instruction("str x0, [sp, #40]");                                   // the read's verdict outlives the question
    emitter.instruction("stp x1, x2, [sp, #48]");                               // and so do its bytes
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("ldr x1, [sp, #32]");                                   // the wrapper descriptor
    emitter.instruction("bl __rt_uw_post_read_eof");
    emitter.instruction("ldr x0, [sp, #40]");
    emitter.instruction("ldp x1, x2, [sp, #48]");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame
    emitter.instruction("add sp, sp, #80");                                     // release native-read scratch storage
    emitter.instruction("ret");                                                 // return the read's own answer
    emitter.label("__rt_fread_real_fd");

    // -- reserve a destination sized for the whole requested read --
    // `__rt_stream_fd` clobbered x1, so the requested length comes back off the stack.
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the requested byte count
    emitter.instruction("cmp x1, #1");                                          // does the caller actually request at least one byte?
    emitter.instruction("b.lt __rt_fread_dest_scratch");                        // non-positive requests write nothing, so keep the current scratch tail
    emitter.instruction("mov x0, x1");                                          // request storage for the full requested read length
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the incoming bytes
    emitter.instruction("mov x12, x0");                                         // destination pointer for the read
    emitter.instruction("b __rt_fread_dest_ready");                             // the destination window is reserved
    emitter.label("__rt_fread_dest_scratch");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.label("__rt_fread_dest_ready");
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- hand back what a refused `stream_get_line()` held, before touching the descriptor --
    //
    // php keeps those bytes in the stream's own read buffer, which every read function shares: a
    // `stream_get_line()` that answered `false` is followed by an `fread()` that sees them.
    // Measured on `php -n` 8.5.6 over a non-blocking socket pair, "abc" with no newline is refused
    // by `stream_get_line()` and then read by `fread($h, 10)`. A stream that never hit that
    // refusal holds nothing, so this costs one call that returns 0 on its first load.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("ldr x1, [sp, #16]");                                   // the reserved destination
    emitter.instruction("ldr x2, [sp, #8]");                                    // at most the requested count
    emitter.instruction("bl __rt_stream_pending_take");                         // x0 = how many came back
    emitter.instruction("cbz x0, __rt_fread_no_pending");
    emitter.instruction("str x0, [sp, #48]");                                   // this much came out of the holding area
    emitter.instruction("ldr x9, [sp, #8]");                                    // the caller's request
    emitter.instruction("cmp x0, x9");
    emitter.instruction("b.lt __rt_fread_no_pending");                          // short: top up from the descriptor
    emitter.instruction("str x0, [sp, #24]");                                   // satisfied in full: they are the result
    emitter.instruction("ldr x1, [sp, #16]");
    emitter.instruction("mov x2, x0");
    emitter.instruction("bl __rt_concat_publish");                              // claim the window they occupy
    emitter.instruction("b __rt_fread_done");
    emitter.label("__rt_fread_no_pending");

    // -- TLS dispatch: the session hangs off the StreamState, so it is keyed by
    //    the generation-checked handle rather than by a reusable descriptor. --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_tls_session");                          // x0 = attached session, zero when plain
    emitter.instruction("cbz x0, __rt_fread_do_syscall");                       // no TLS attached → fall through to read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // buf ptr: x12 is caller-saved and the call clobbered it
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load elephc_tls_read entry pointer
    emitter.instruction("blr x9");                                              // x0 = bytes read (>=0) or -1
    emitter.instruction("cmp x0, #0");                                          // value-based check after the TLS call
    emitter.instruction("b.ge __rt_fread_read_ok");                             // continue when TLS read returned >= 0
    emitter.instruction("str xzr, [sp, #24]");                                  // TLS error: zero-length result
    emitter.instruction("str xzr, [sp, #40]");                                  // a failed TLS read is a failed read
    emitter.instruction("b __rt_fread_done");                                   // and does not exhaust the stream either

    emitter.label("__rt_fread_do_syscall");
    // -- php reads a WHOLE CHUNK from a regular file and keeps what the call did not need --
    //
    // Without this, `fgetc()` is one `read(2)` per byte: MEASURED at 499 ms for 900 000 bytes
    // where php takes 14 ms, and `stream_get_meta_data()['unread_bytes']` was always 0 where php
    // reports what its buffer still holds. The chunked reader below is the SAME one the wrapper
    // path already uses — it reads a chunk through `__rt_fread`, hands the whole thing to the
    // stream's holding area, and takes the caller's request back out of it.
    //
    // Restricted to REGULAR files on purpose. Reading ahead on a socket or a pipe changes when a
    // read blocks and what `stream_select()` sees next, which is a separate question from this
    // one; `S_ISREG` is the same probe `stream_get_meta_data()` uses for `seekable`.
    //
    // The recursion stops itself: the chunked reader asks for exactly one chunk, and a request of
    // a chunk or more takes the syscall below.
    emitter.instruction("ldr x0, [sp, #8]");                                    // the caller's request
    emitter.instruction("cmp x0, #1");
    emitter.instruction("b.lt __rt_fread_syscall_now");                         // a non-positive request buffers nothing
    // A top-up already holds part of the answer in the window; the chunked reader starts from
    // nothing and would lose it, so this read finishes on the descriptor.
    emitter.instruction("ldr x9, [sp, #48]");                                   // bytes already served from the holding area
    emitter.instruction("cbnz x9, __rt_fread_syscall_now");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // only a stream with a STATE can hold a surplus
    emitter.instruction("cbz x0, __rt_fread_syscall_now");
    emitter.instruction("ldr x0, [sp, #32]");                                   // the resolved backend descriptor
    emitter.instruction("bl __rt_stream_fd_is_bufferable");                     // a file, a socket or a fifo: php buffers all three
    emitter.instruction("cbz x0, __rt_fread_syscall_now");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_chunk_size");                           // x0 = what php would ask for
    emitter.instruction("ldr x1, [sp, #8]");                                    // the caller's request
    emitter.instruction("cmp x1, x0");
    emitter.instruction("b.ge __rt_fread_syscall_now");                         // at least a chunk: read exactly it
    emitter.instruction("ldr x1, [sp, #16]");                                   // hand the reserved window back before
    emitter.instruction("mov x2, #0");                                          // the chunked reader reserves its own
    emitter.instruction("bl __rt_concat_publish");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the handle drives the chunked reader
    emitter.instruction("ldr x1, [sp, #8]");                                    // and the caller's request
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame before tail dispatch
    emitter.instruction("add sp, sp, #80");                                     // release native-read scratch storage
    emitter.instruction("b __rt_uw_fread_chunked");

    emitter.label("__rt_fread_syscall_now");
    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #32]");                                   // fd for read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // buffer pointer: the TLS probe clobbers caller-saved x12
    emitter.instruction("ldr x2, [sp, #8]");                                    // number of bytes to read
    emitter.instruction("ldr x9, [sp, #48]");                                   // what the holding area already supplied
    emitter.instruction("add x1, x1, x9");                                      // append after it
    emitter.instruction("sub x2, x2, x9");                                      // and ask only for the remainder
    emitter.syscall(3);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: negative read result means failure
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fread_read_ok")); // continue only when the read syscall succeeded
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", emitter.platform.would_block_errno())); // Linux: is this -EAGAIN/-EWOULDBLOCK from a nonblocking fd?
    } else {
        emitter.instruction(&format!("cmp x0, #{}", emitter.platform.would_block_errno())); // macOS: is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    }
    emitter.instruction("b.eq __rt_fread_would_block");                         // a transient nonblocking miss is not EOF
    // php says a failed read out loud, naming the function, the CHUNK size it asks its source
    // for, the errno and the system's own text for it. A cold-path frame holds the errno across
    // the two calls; x30 is free to clobber because every exit reloads it from the entry frame.
    emitter.instruction("sub sp, sp, #16");                                     // ⚠️ every offset below is +16 for the length of this block
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("neg x0, x0");                                      // Linux answers -errno
    }
    emitter.instruction("str x0, [sp, #0]");                                    // park the errno
    // `__rt_itoa` formats at `_concat_buf + _concat_off` without advancing it, and the held bytes
    // this read still answers with begin exactly there. Push the offset past them and restore it.
    emitter.instruction("ldr x9, [sp, #64]");                                   // what the holding area supplied (was [sp, #48])
    emitter.instruction("str x9, [sp, #8]");                                    // remember how far the offset is pushed
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("ldr x11, [x10]");
    emitter.instruction("add x11, x11, x9");
    emitter.instruction("str x11, [x10]");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the opaque stream handle (was [sp, #0])
    emitter.instruction("bl __rt_stream_chunk_size");                           // the count php names, not the syscall's
    emitter.instruction("ldr x1, [sp, #0]");                                    // the errno
    emitter.instruction("bl __rt_read_failed_notice");
    emitter.instruction("ldr x9, [sp, #8]");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("ldr x11, [x10]");
    emitter.instruction("sub x11, x11, x9");                                    // put the write offset back where it stood
    emitter.instruction("str x11, [x10]");
    emitter.instruction("add sp, sp, #16");                                     // offsets below are the frame's own again
    // A read that FAILED after the holding area already supplied bytes still answers those
    // bytes: php loses nothing it has already served out of its buffer.
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("str x9, [sp, #24]");                                   // failed reads carry only what was held
    emitter.instruction("cbnz x9, __rt_fread_publish_total");                   // publish what the buffer gave
    emitter.instruction("str xzr, [sp, #40]");                                  // php answers false for a read that fails
    emitter.instruction("b __rt_fread_done");                                   // a FAILED read does not exhaust the stream: php keeps feof() false
    emitter.label("__rt_fread_read_ok");

    // -- publish the bytes actually read into the reserved destination --
    emitter.instruction("ldr x9, [sp, #48]");                                   // what the holding area already supplied
    emitter.instruction("add x0, x0, x9");                                      // the answer is both halves
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    emitter.label("__rt_fread_publish_total");
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the reserved destination pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the number of bytes actually read
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed reads

    // -- set EOF when the read returned fewer bytes than requested --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested byte count
    emitter.instruction("cmp x0, x9");                                          // did the backend satisfy the complete request?
    emitter.instruction("b.ge __rt_fread_done");                                // a full read does not prove EOF
    emitter.instruction("cbz x0, __rt_fread_mark_eof");                         // a zero-byte read is EOF for every blocking backend
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the backend descriptor for a seekability probe
    emitter.instruction("mov x1, #0");                                          // probe the current position without moving it
    emitter.instruction("mov x2, #1");                                          // select SEEK_CUR for the non-mutating probe
    emitter.syscall(199);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux reports a non-seekable descriptor as a negative result
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fread_mark_eof")); // only seekable short reads prove EOF
    emitter.instruction("b __rt_fread_done");                                   // sockets and pipes may legally return partial data
    emitter.label("__rt_fread_mark_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("mov x1, #1");                                          // publish the EOF state
    emitter.instruction("bl __rt_stream_eof_set");                              // update only this stream's stable state
    emitter.instruction("b __rt_fread_done");                                   // preserve a successful short-read result

    emitter.label("__rt_fread_would_block");
    emitter.instruction("ldr x9, [sp, #48]");                                   // whatever the holding area supplied stands
    emitter.instruction("str x9, [sp, #24]");                                   // an empty read otherwise, without setting EOF for EAGAIN/EWOULDBLOCK

    // -- return pointer and length --
    emitter.label("__rt_fread_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return actual bytes read as length

    // Legacy per-descriptor slots still serve the filter families not yet moved
    // onto the chain (user filters, zlib/bzip2/iconv). A filter lives in exactly
    // one mechanism, so running both is correct for the duration of the migration.
    // -- apply attached read filters to the bytes just read (2-slot chain) --
    //    Slot 0 = _stream_read_filters[fd], slot 1 = _stream_read_filters[fd+256]
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the file descriptor
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    // -- slot 0 --
    emitter.instruction("ldrb w3, [x9, x0]");                                   // read filter id for slot 0
    emitter.instruction("cbz w3, __rt_fread_slot1");                            // skip slot 0 when empty
    emitter.instruction("cmp w3, #128");                                        // user-filter id range?
    emitter.instruction("b.lt __rt_fread_builtin_slot0");                       // built-in filter
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read)
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← transformed
    emitter.instruction("b __rt_fread_slot1");                                  // proceed to slot 1
    emitter.label("__rt_fread_builtin_slot0");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform in place
    // -- slot 1 --
    emitter.label("__rt_fread_slot1");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload fd
    emitter.instruction("add x10, x0, #256");                                   // fd+256 (slot 1 index)
    emitter.instruction("ldrb w3, [x9, x10]");                                  // read filter id for slot 1
    emitter.instruction("cbz w3, __rt_fread_ret");                              // skip slot 1 when empty
    emitter.instruction("cmp w3, #128");                                        // user-filter id range?
    emitter.instruction("b.lt __rt_fread_builtin_slot1");                       // built-in filter
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read)
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← transformed
    emitter.instruction("b __rt_fread_ret");                                    // common epilogue
    emitter.label("__rt_fread_builtin_slot1");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform in place


    // -- restore frame and return --
    emitter.label("__rt_fread_ret");
    // The flag rides out in x0, beside the x1/x2 string pair, exactly as
    // `__rt_stream_get_line` reports its own. The result pointer is deliberately NOT
    // nulled: x86_64 already returns a null pointer for an ordinary EOF, so the pointer
    // cannot mean "failed" on both arches, and every caller here tests the length first.
    emitter.instruction("ldr x0, [sp, #40]");                                   // x0 = 0 only when the read failed
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emit_wrapper_chunked_read(emitter);
    emit_uw_fill_one_chunk(emitter);
}

/// Emits `__rt_uw_fill_one_chunk(handle) -> bytes added`.
///
/// Asks the stream's source for one chunk and puts the whole thing in the holding area, which is
/// php's read buffer. Answers how many bytes it added, so a caller topping the area up knows when
/// the source is spent.
///
/// It asks the SOURCE, not `__rt_fread`: the topping-up that calls this runs while the holding
/// area still holds something, so going through `__rt_fread` took the held path, topped up again,
/// and recursed until the stack ran out. php's `stream_eof()` question is asked here instead, so
/// the conversation with the class is the same either way.
fn emit_uw_fill_one_chunk(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: uw_fill_one_chunk ---");
            emitter.label_global("__rt_uw_fill_one_chunk");
            emitter.instruction("sub sp, sp, #64");
            emitter.instruction("stp x29, x30, [sp, #48]");
            emitter.instruction("add x29, sp, #48");
            emitter.instruction("str x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("str x1, [sp, #40]");                           // an explicit size, or 0 for "one chunk"
            emitter.instruction("bl __rt_stream_chunk_size");                   // what php would ask for
            emitter.instruction("ldr x9, [sp, #40]");                           // the caller's explicit size
            emitter.instruction("cmp x9, #0");
            emitter.instruction("csel x0, x9, x0, ne");                         // a size of 0 keeps the chunk
            emitter.instruction("str x0, [sp, #24]");                           // across the descriptor lookup
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("bl __rt_stream_fd");                           // the source behind this stream
            emitter.instruction("str x0, [sp, #8]");                            // the descriptor, for the question below
            emitter.instruction("ldr x1, [sp, #24]");                           // one chunk
            emitter.instruction("bl __rt_user_wrapper_fread");                  // x1/x2 = the chunk
            emitter.instruction("stp x1, x2, [sp, #24]");                       // it outlives the question
            emitter.instruction("ldr x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("ldr x1, [sp, #8]");                            // the descriptor
            emitter.instruction("bl __rt_uw_post_read_eof");                    // php asks after every read
            emitter.instruction("ldp x1, x2, [sp, #24]");
            emitter.instruction("cbz x2, __rt_uwfoc_none");                     // the source had nothing left
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("ldr x1, [sp, #24]");
            emitter.instruction("ldr x2, [sp, #32]");
            emitter.instruction("bl __rt_stream_pending_append");               // ADDED to what is held, not put over it
            emitter.instruction("ldr x1, [sp, #24]");
            emitter.instruction("mov x2, #0");
            emitter.instruction("bl __rt_concat_publish");                      // hand the scratch window back
            emitter.instruction("ldr x0, [sp, #24]");
            emitter.instruction("bl __rt_decref_any");                          // the copy on the stream is the one that lives
            emitter.instruction("ldr x0, [sp, #32]");                           // how many were added
            emitter.instruction("b __rt_uwfoc_done");
            emitter.label("__rt_uwfoc_none");
            emitter.instruction("mov x0, #0");
            emitter.label("__rt_uwfoc_done");
            emitter.instruction("ldp x29, x30, [sp, #48]");
            emitter.instruction("add sp, sp, #64");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: uw_fill_one_chunk ---");
            emitter.label_global("__rt_uw_fill_one_chunk");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 32");
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the opaque stream handle
            emitter.instruction("call __rt_stream_chunk_size");                 // what php would ask for
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // across the descriptor lookup
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("call __rt_stream_fd");                         // the source behind this stream
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // the descriptor, for the question below
            emitter.instruction("mov rdi, rax");
            emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");               // one chunk
            emitter.instruction("call __rt_user_wrapper_fread");                // rax/rdx = the chunk
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // it outlives the question
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // the opaque stream handle
            emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");               // the descriptor
            emitter.instruction("call __rt_uw_post_read_eof");                  // php asks after every read
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
            emitter.instruction("test rdx, rdx");
            emitter.instruction("jz __rt_uwfoc_none_x86");                      // the source had nothing left
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
            emitter.instruction("call __rt_stream_pending_append");             // ADDED to what is held, not put over it
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // publish reads RAX/RDX
            emitter.instruction("xor edx, edx");
            emitter.instruction("call __rt_concat_publish");                    // hand the scratch window back
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // decref reads RAX
            emitter.instruction("call __rt_decref_any");                        // the copy on the stream is the one that lives
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // how many were added
            emitter.instruction("jmp __rt_uwfoc_done_x86");
            emitter.label("__rt_uwfoc_none_x86");
            emitter.instruction("xor eax, eax");
            emitter.label("__rt_uwfoc_done_x86");
            emitter.instruction("add rsp, 32");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_uw_fread_chunked(handle, requested) -> (flag, ptr, len)`.
///
/// php never asks a stream's source for exactly what the caller wants: it asks for a whole chunk
/// and keeps the surplus in the stream's own read buffer, where every later reader finds it. This
/// is that rule for a user-registered wrapper — read one chunk, hand the whole thing to the
/// stream, then take back only what the call asked for.
///
/// It re-enters `__rt_fread` for the chunk. That terminates because the request is EXACTLY the
/// chunk size, and the caller's `>=` test then routes it down the ordinary direct path.
///
/// The surplus is put on the stream BEFORE anything is handed back, so the answer and the buffer
/// can never disagree about which bytes were consumed.
fn emit_wrapper_chunked_read(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: chunked wrapper read ---");
            emitter.label_global("__rt_uw_fread_chunked");
            // Frame: [0]=handle [8]=requested [16]=chunk ptr [24]=chunk len [32]=destination
            //        [40]=answered length
            emitter.instruction("sub sp, sp, #64");
            emitter.instruction("stp x29, x30, [sp, #48]");
            emitter.instruction("add x29, sp, #48");
            emitter.instruction("str x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("str x1, [sp, #8]");                            // what the caller asked for

            emitter.instruction("bl __rt_stream_chunk_size");                   // x0 = the chunk php would ask for
            emitter.instruction("mov x1, x0");                                  // read exactly one chunk
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("bl __rt_fread");                               // x0 = flag, x1 = ptr, x2 = len
            emitter.instruction("str x0, [sp, #40]");                           // the read's OWN verdict travels out
            // A REFUSED read keeps none of its bytes. php discards them — a wrapper with no
            // `stream_eof` reads, warns, and answers false — so they must not reach the stream's
            // holding area, where the next reader would serve them as if the read had worked.
            emitter.instruction("cbz x0, __rt_uwfc_failed");                    // refused: give the window back, answer false
            emitter.instruction("cbz x2, __rt_uwfc_empty");                     // the source had nothing
            emitter.instruction("stp x1, x2, [sp, #16]");                       // the chunk outlives the calls below

            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("ldr x1, [sp, #16]");
            emitter.instruction("ldr x2, [sp, #24]");
            emitter.instruction("bl __rt_stream_pending_put");                  // the WHOLE chunk belongs to the stream
            emitter.instruction("ldr x1, [sp, #16]");
            emitter.instruction("mov x2, #0");
            emitter.instruction("bl __rt_concat_publish");                      // hand the scratch window back
            emitter.instruction("ldr x0, [sp, #16]");
            emitter.instruction("bl __rt_decref_any");                          // the copy on the stream is the one that lives

            emitter.instruction("ldr x0, [sp, #8]");                            // storage for the caller's request
            emitter.instruction("bl __rt_concat_reserve");
            emitter.instruction("str x0, [sp, #32]");
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("ldr x1, [sp, #32]");
            emitter.instruction("ldr x2, [sp, #8]");
            emitter.instruction("bl __rt_stream_pending_take");                 // x0 = how many came back
            emitter.instruction("str x0, [sp, #40]");
            emitter.instruction("ldr x1, [sp, #32]");
            emitter.instruction("mov x2, x0");
            emitter.instruction("bl __rt_concat_publish");                      // claim the window they occupy
            // -- the short-read judgement, for backends that cannot be ASKED --
            //
            // A class answers for itself: `__rt_fread` puts its `stream_eof()` on the stream right
            // after the read, and guessing over that reports end of file for a wrapper that simply
            // hands back small pieces. Every other backend has only this judgement, and needs it:
            // MEASURED on `php://memory`, a 3-byte stream read with `fread($h, 3)` leaves `feof()`
            // FALSE and a 1-byte stream read with `fread($h, 2)` leaves it TRUE.
            //
            // …and it only holds for a SEEKABLE backend. A pipe or a socket may legally hand back
            // less than was asked without being anywhere near its end, which is why `__rt_fread`
            // above probes `lseek` before drawing the same conclusion. This judgement did not, and
            // it runs AFTER that one, so it overrode a correct answer with a wrong one: MEASURED
            // on `php -n` 8.5.6, `fread(popen("echo here is some output", "rb"), 100)` returns 20
            // bytes and leaves `eof` FALSE, where elephc answered TRUE. php sets the flag from the
            // read op — for a pipe that means a read of ZERO — never from a short count.
            emitter.instruction("ldr x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("bl __rt_stream_fd");                           // its backend descriptor
            emitter.instruction("mov w9, #0x4000");                             // USER_WRAPPER_FD_BASE, high half
            emitter.instruction("lsl w9, w9, #16");
            emitter.instruction("cmp x0, x9");
            emitter.instruction("b.lo __rt_uwfc_guess");                        // a native descriptor: judge it
            super::emit_load_handles_cap(emitter, "x10");
            emitter.instruction("add x10, x9, x10");                            // the wrapper range ends at the handle capacity
            emitter.instruction("cmp x0, x10");
            emitter.instruction("b.lo __rt_uwfc_no_guess");                     // a wrapper: it already answered
            emitter.label("__rt_uwfc_guess");
            emitter.instruction("mov x1, #0");                                  // probe the position without moving it
            emitter.instruction("mov x2, #1");                                  // SEEK_CUR
            emitter.syscall(199);
            if emitter.platform.needs_cmp_before_error_branch() {
                emitter.instruction("cmp x0, #0");                              // Linux reports a non-seekable descriptor as a negative result
            }
            emitter.instruction(
                &emitter.platform.branch_on_syscall_success("__rt_uwfc_seekable"),
            );
            emitter.instruction("b __rt_uwfc_no_guess");                        // a pipe or a socket: the read's own verdict stands
            emitter.label("__rt_uwfc_seekable");
            emitter.instruction("ldr x9, [sp, #8]");                            // the caller's request
            emitter.instruction("ldr x10, [sp, #40]");                          // what it received
            emitter.instruction("cmp x10, x9");
            emitter.instruction("b.lt __rt_uwfc_short");                        // less than asked: the source is spent
            emitter.instruction("mov x1, #0");                                  // satisfied in full: not at end
            emitter.instruction("b __rt_uwfc_eof_set");
            emitter.label("__rt_uwfc_short");
            emitter.instruction("mov x1, #1");                                  // at end
            emitter.label("__rt_uwfc_eof_set");
            emitter.instruction("ldr x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("bl __rt_stream_eof_set");
            emitter.label("__rt_uwfc_no_guess");
            emitter.instruction("mov x0, #1");                                  // a real result, not a failed read
            emitter.instruction("ldr x1, [sp, #32]");
            emitter.instruction("ldr x2, [sp, #40]");
            emitter.instruction("ldp x29, x30, [sp, #48]");
            emitter.instruction("add sp, sp, #64");
            emitter.instruction("ret");

            emitter.label("__rt_uwfc_failed");
            emitter.instruction("cbz x1, __rt_uwfc_failed_empty");              // the read already gave its window back
            emitter.instruction("stp x1, x2, [sp, #16]");                       // the refused chunk, to be released
            emitter.instruction("mov x2, #0");                                  // release the whole claimed window
            emitter.instruction("bl __rt_concat_publish");                      // hand the scratch window back
            emitter.instruction("ldr x0, [sp, #16]");
            emitter.instruction("bl __rt_decref_any");                          // and release the chunk itself
            emitter.label("__rt_uwfc_failed_empty");
            emitter.instruction("mov x0, #0");                                  // the refusal is the answer
            emitter.instruction("mov x1, #0");
            emitter.instruction("mov x2, #0");
            emitter.instruction("ldp x29, x30, [sp, #48]");
            emitter.instruction("add sp, sp, #64");
            emitter.instruction("ret");

            emitter.label("__rt_uwfc_empty");
            // NOT a hardcoded success: a wrapper with no `stream_read` answers php `false`, and
            // that verdict is the flag, not the length. Returning 1 here turned that false into
            // `""` — a successful empty read.
            emitter.instruction("ldr x0, [sp, #40]");                           // the read's own verdict
            emitter.instruction("mov x1, #0");
            emitter.instruction("mov x2, #0");
            emitter.instruction("ldp x29, x30, [sp, #48]");
            emitter.instruction("add sp, sp, #64");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: chunked wrapper read ---");
            emitter.label_global("__rt_uw_fread_chunked");
            // Frame: [rbp-8]=handle [rbp-16]=requested [rbp-24]=chunk ptr [rbp-32]=chunk len
            //        [rbp-40]=destination [rbp-48]=answered length
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 48");
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the opaque stream handle
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // what the caller asked for

            emitter.instruction("call __rt_stream_chunk_size");                 // rax = the chunk php would ask for
            emitter.instruction("mov rsi, rax");                                // read exactly one chunk
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("call __rt_fread");                             // rax = ptr, rdx = len, rcx = flag
            emitter.instruction("mov QWORD PTR [rbp - 48], rcx");               // the read's OWN verdict travels out
            // See the AArch64 counterpart: a refused read keeps none of its bytes.
            emitter.instruction("test rcx, rcx");
            emitter.instruction("jz __rt_uwfc_failed_x86");                     // refused: give the window back, answer false
            emitter.instruction("test rdx, rdx");
            emitter.instruction("jz __rt_uwfc_empty_x86");                      // the source had nothing
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");

            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
            emitter.instruction("call __rt_stream_pending_put");                // the WHOLE chunk belongs to the stream
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // publish reads RAX/RDX
            emitter.instruction("xor edx, edx");
            emitter.instruction("call __rt_concat_publish");                    // hand the scratch window back
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // decref reads RAX, not rdi
            emitter.instruction("call __rt_decref_any");                        // the copy on the stream is the one that lives

            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // reserve reads RAX
            emitter.instruction("call __rt_concat_reserve");
            emitter.instruction("mov QWORD PTR [rbp - 40], rax");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
            emitter.instruction("call __rt_stream_pending_take");               // rax = how many came back
            emitter.instruction("mov QWORD PTR [rbp - 48], rax");
            emitter.instruction("mov rdx, rax");                                // publish takes the length in RDX
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // and the pointer in RAX
            emitter.instruction("call __rt_concat_publish");                    // claim the window they occupy
            // See the AArch64 counterpart: the judgement is for backends that cannot be asked.
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // the opaque stream handle
            emitter.instruction("call __rt_stream_fd");                         // its backend descriptor
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");
            emitter.instruction("jb __rt_uwfc_guess_x86");                      // a native descriptor: judge it
            super::emit_load_handles_cap(emitter, "r10");
            emitter.instruction("add r10, r9");                                 // the wrapper range ends at the handle capacity
            emitter.instruction("cmp rax, r10");
            emitter.instruction("jb __rt_uwfc_no_guess_x86");                   // a wrapper: it already answered
            emitter.label("__rt_uwfc_guess_x86");
            // See the AArch64 counterpart: the judgement only holds for a SEEKABLE backend.
            emitter.instruction("mov rdi, rax");                                // the descriptor the lookup just resolved
            emitter.instruction("xor esi, esi");                                // probe the position without moving it
            emitter.instruction("mov edx, 1");                                  // SEEK_CUR
            emitter.instruction("call lseek");
            emitter.instruction("test rax, rax");                               // did the position probe fail?
            emitter.instruction("js __rt_uwfc_no_guess_x86");                   // a pipe or a socket: the read's own verdict stands
            emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                // what the caller received
            emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                // against what it asked for
            emitter.instruction("jl __rt_uwfc_short_x86");                      // less than asked: the source is spent
            emitter.instruction("xor esi, esi");                                // satisfied in full: not at end
            emitter.instruction("jmp __rt_uwfc_eof_set_x86");
            emitter.label("__rt_uwfc_short_x86");
            emitter.instruction("mov esi, 1");                                  // at end
            emitter.label("__rt_uwfc_eof_set_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // the opaque stream handle
            emitter.instruction("call __rt_stream_eof_set");
            emitter.label("__rt_uwfc_no_guess_x86");
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");
            emitter.instruction("mov rcx, 1");                                  // a real result, not a failed read
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");

            emitter.label("__rt_uwfc_failed_x86");
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_uwfc_failed_empty_x86");               // the read already gave its window back
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // the refused chunk, to be released
            emitter.instruction("xor edx, edx");                                // release the whole claimed window
            emitter.instruction("call __rt_concat_publish");                    // hand the scratch window back
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // decref reads RAX, not rdi
            emitter.instruction("call __rt_decref_any");                        // and release the chunk itself
            emitter.label("__rt_uwfc_failed_empty_x86");
            emitter.instruction("xor eax, eax");                                // the refusal is the answer
            emitter.instruction("xor edx, edx");
            emitter.instruction("xor ecx, ecx");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");

            emitter.label("__rt_uwfc_empty_x86");
            // See the AArch64 counterpart: the verdict is the flag, not the length.
            emitter.instruction("xor eax, eax");
            emitter.instruction("xor edx, edx");
            emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");               // the read's own verdict
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits the x86_64 Linux variant of `__rt_fread` using libc `read()`.
fn emit_fread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread_raw");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fread() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor, length, and concat-buffer start pointer
    emitter.instruction("sub rsp, 64");                                         // reserve aligned stream, descriptor, and read-result spill slots

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested byte count across the concat-buffer address computation and libc read() call
    // See the AArch64 half: a failed read and an empty one both end with length 0, so the
    // difference travels in its own slot and leaves in rcx.
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // "this is a real result" — cleared only by an actual read failure
    // See the AArch64 half: slot 56 is how many of the caller's bytes came out of the holding
    // area, so a request that outruns it is topped up from the descriptor into the same window.
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // nothing served from the holding area yet
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor through StreamState
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the resolved backend descriptor
    emitter.instruction("mov r9d, 0x40000000");                                 // materialize USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is the backend below the wrapper range?
    emitter.instruction("jb __rt_fread_real_fd_x86");                           // native descriptors continue to libc read
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("add r10, r9");                                         // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp rax, r10");                                        // is the backend above the wrapper range?
    emitter.instruction("jae __rt_fread_real_fd_x86");                          // non-wrapper synthetic backends stay on the native path
    // Same as the AArch64 arm: the wrapper stream's own read buffer answers first, and leaves
    // through the epilogue that skips the descriptor-indexed filter table.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // rax = stable stream state, 0 when none
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_wrapper_call_x86");                      // no state: nothing can be held
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]")); // held byte count
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]")); // already handed out
    emitter.instruction("sub r9, r10");                                         // what remains
    emitter.instruction("cmp r9, 0");
    emitter.instruction("jle __rt_fread_wrapper_call_x86");                     // nothing held: ask the wrapper
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the requested byte count
    emitter.instruction("cmp rax, 1");                                          // a non-positive request moves nothing
    emitter.instruction("jl __rt_fread_wrapper_call_x86");
    emitter.instruction("call __rt_concat_reserve");                            // reserve reads RAX, not rdi; rax = destination
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the destination, and the returned pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the destination
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // at most the requested count
    emitter.instruction("call __rt_stream_pending_take");                       // rax = how many came back
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_fread_held_ok_x86");                          // the held bytes are the whole result
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // nothing came back after all: give the
    emitter.instruction("xor edx, edx");                                        // window back (publish reads RAX/RDX)
    emitter.instruction("call __rt_concat_publish");
    emitter.instruction("jmp __rt_fread_wrapper_call_x86");
    emitter.label("__rt_fread_held_ok_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // the result length
    emitter.instruction("mov rdx, rax");                                        // publish takes the length in RDX
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // and the pointer in RAX
    emitter.instruction("call __rt_concat_publish");                            // claim the window they occupy
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the pair directly
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");
    emitter.instruction("jmp __rt_fread_ret_x86");

    emitter.label("__rt_fread_wrapper_call_x86");
    // See the AArch64 counterpart: a request smaller than a chunk takes the chunked reader.
    // See the AArch64 counterpart: no state means nowhere to keep a surplus.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // rax = stable stream state, 0 when none
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_wrapper_direct_x86");                    // no buffer to keep a surplus in
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_chunk_size");                         // rax = what php would ask for
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // the caller's request
    emitter.instruction("cmp r9, rax");
    emitter.instruction("jge __rt_fread_wrapper_direct_x86");                   // at least a chunk: ask for exactly it
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the handle drives the chunked reader
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // and the caller's request
    emitter.instruction("add rsp, 64");                                         // release native-read scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_uw_fread_chunked");
    emitter.label("__rt_fread_wrapper_direct_x86");
    // See the AArch64 counterpart: php asks its source for ONE CHUNK and no more, however much the
    // caller wanted — with a chunk of 100, `fread($h, 250)` elicits "read with size: 100" and
    // answers 100. A chunk of 1 is the exception, where the buffer is skipped and the caller's own
    // count goes straight through.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_chunk_size");                         // rax = one chunk
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the caller's request
    emitter.instruction("cmp rax, 1");
    emitter.instruction("je __rt_fread_wrapper_size_ready_x86");                // chunk 1: ask for the whole request
    emitter.instruction("cmp rsi, rax");
    emitter.instruction("cmovg rsi, rax");                                      // otherwise ask for at most one chunk
    emitter.label("__rt_fread_wrapper_size_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the size this read really asks for
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the wrapper descriptor the probe clobbered
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested byte count
    emitter.instruction("call __rt_user_wrapper_fread");                        // rax/rdx = the bytes, rcx = verdict
    // php asks the wrapper whether THAT read reached the end, and remembers the answer; see
    // `emit_uw_post_read_eof`. The tail call becomes a call so the question can be asked after it.
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // the read's verdict outlives the question
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // and so do its bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // the wrapper descriptor
    emitter.instruction("call __rt_uw_post_read_eof");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");
    emitter.instruction("add rsp, 64");                                         // release native-read scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the read's own answer
    emitter.label("__rt_fread_real_fd_x86");
    emitter.instruction("cmp rax, 0");                                          // did descriptor resolution produce a valid backend?
    emitter.instruction("jge __rt_fread_fd_ok_x86");                            // continue to the normal read path for non-negative descriptors
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer for an invalid stream
    emitter.instruction("xor edx, edx");                                        // return an empty string length for an invalid stream
    emitter.instruction("add rsp, 64");                                         // release native-read scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // skip the read path for invalid stream handles

    emitter.label("__rt_fread_fd_ok_x86");
    // -- reserve a destination sized for the whole requested read --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested byte count after the descriptor resolution
    emitter.instruction("cmp rsi, 1");                                          // does the caller actually request at least one byte?
    emitter.instruction("jl __rt_fread_dest_scratch_x86");                      // non-positive requests write nothing, so keep the current scratch tail
    emitter.instruction("mov rax, rsi");                                        // request storage for the full requested read length (reserve reads rax)
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the incoming bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the reserved destination pointer for the final elephc string result
    emitter.instruction("jmp __rt_fread_dest_ready_x86");                       // the destination window is reserved
    emitter.label("__rt_fread_dest_scratch_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // load the current concat-buffer absolute offset before appending the fread() result
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");                    // materialize the concat-buffer base address once for the x86_64 fread() helper
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the start pointer for the bytes that libc read() will append
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the concat-buffer start pointer for the final elephc string result
    emitter.label("__rt_fread_dest_ready_x86");

    // -- hand back what a refused `stream_get_line()` held, before touching the descriptor --
    // See the AArch64 counterpart: php keeps those bytes in the stream's shared read buffer.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the reserved destination
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // at most the requested count
    emitter.instruction("call __rt_stream_pending_take");                       // rax = how many came back
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_no_pending_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // this much came out of the holding area
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // against the caller's request
    emitter.instruction("jl __rt_fread_no_pending_x86");                        // short: top up from the descriptor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // satisfied in full: they are the result
    emitter.instruction("mov rdx, rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_concat_publish");                            // claim the window they occupy
    emitter.instruction("jmp __rt_fread_publish_x86");
    emitter.label("__rt_fread_no_pending_x86");

    // -- TLS dispatch: the session hangs off the StreamState, so it is keyed by
    //    the generation-checked handle rather than by a reusable descriptor. --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_tls_session");                        // rax = attached session, zero when plain
    emitter.instruction("test rax, rax");                                       // is a TLS session attached?
    emitter.instruction("jz __rt_fread_do_syscall_x86");                        // no TLS attached → use libc read
    emitter.instruction("mov rdi, rax");                                        // handle as first arg
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // buf ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_read_fn", 0);      // prepare SysV call argument
    emitter.instruction("call r9");                                             // rax = bytes read (>=0) or -1
    emitter.instruction("cmp rax, 0");                                          // did the TLS bridge return bytes?
    emitter.instruction("jl __rt_fread_tls_failed_x86");                        // strictly negative is an error, not an exhausted stream
    emitter.instruction("jz __rt_fread_eof_x86");                               // a zero-byte TLS read is an ordinary EOF
    emitter.instruction("jmp __rt_fread_read_ok_x86");                          // publish the successful TLS read
    emitter.label("__rt_fread_tls_failed_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // a failed TLS read is a failed read
    emitter.instruction("jmp __rt_fread_failed_x86");                           // and does not exhaust the stream either
    emitter.label("__rt_fread_do_syscall_x86");
    // See the AArch64 counterpart: php reads a WHOLE CHUNK from a regular file and keeps the
    // surplus, which is what makes `fgetc()` one syscall per CHUNK instead of one per byte and
    // what `unread_bytes` reports. Sockets, pipes and ttys keep the unbuffered path.
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the caller's request
    emitter.instruction("cmp rax, 1");
    emitter.instruction("jl __rt_fread_syscall_now_x86");                       // a non-positive request buffers nothing
    // A top-up already holds part of the answer in the window; the chunked reader starts from
    // nothing and would lose it.
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // bytes already served from the holding area
    emitter.instruction("jne __rt_fread_syscall_now_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // only a stream with a STATE can hold a surplus
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_syscall_now_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the resolved backend descriptor
    emitter.instruction("call __rt_stream_fd_is_bufferable");                   // a file, a socket or a fifo: php buffers all three
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_syscall_now_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_chunk_size");                         // rax = what php would ask for
    emitter.instruction("cmp QWORD PTR [rbp - 16], rax");                       // the caller's request against it
    emitter.instruction("jge __rt_fread_syscall_now_x86");                      // at least a chunk: read exactly it
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // hand the reserved window back before
    emitter.instruction("xor edx, edx");                                        // the chunked reader reserves its own
    emitter.instruction("call __rt_concat_publish");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the handle drives the chunked reader
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // and the caller's request
    emitter.instruction("mov rsp, rbp");                                        // restore the caller frame before tail dispatch
    emitter.instruction("pop rbp");
    emitter.instruction("jmp __rt_uw_fread_chunked");

    emitter.label("__rt_fread_syscall_now_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc read() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the concat-buffer write pointer as the second libc read() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the requested byte count as the third libc read() argument
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // what the holding area already supplied
    emitter.instruction("add rsi, r9");                                         // append after it
    emitter.instruction("sub rdx, r9");                                         // and ask only for the remainder
    emitter.instruction("call read");                                           // read the requested bytes into the concat-buffer append window through libc read()
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as bytes, EOF, or failure
    emitter.instruction("jg __rt_fread_read_ok_x86");                           // positive byte count: publish the successful read
    emitter.instruction("jl __rt_fread_read_failed_x86");                       // negative result: inspect errno before treating it as EOF
    emitter.instruction("jmp __rt_fread_eof_x86");                              // zero-byte read means real EOF

    emitter.label("__rt_fread_read_ok_x86");
    emitter.instruction("add rax, QWORD PTR [rbp - 56]");                       // the answer is the holding area's bytes and the read's
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the actual byte count across EOF publication
    emitter.label("__rt_fread_publish_total_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // pass the number of bytes actually read
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass the reserved destination pointer
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed reads
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the byte count publish left in rdx for the EOF classification below
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // did the backend satisfy the complete request?
    emitter.instruction("jge __rt_fread_publish_x86");                          // a full read does not prove EOF
    emitter.instruction("test rax, rax");                                       // was this a universal zero-byte EOF read?
    emitter.instruction("jz __rt_fread_mark_eof_x86");                          // zero bytes mark every blocking backend exhausted
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the backend descriptor for a seekability probe
    emitter.instruction("xor esi, esi");                                        // probe the current position without moving it
    emitter.instruction("mov edx, 1");                                          // select SEEK_CUR for the non-mutating probe
    emitter.instruction("call lseek");                                          // seekable short reads prove the regular stream was exhausted
    emitter.instruction("test rax, rax");                                       // did the position probe fail?
    emitter.instruction("js __rt_fread_publish_x86");                           // sockets and pipes may legally return partial data
    emitter.label("__rt_fread_mark_eof_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov esi, 1");                                          // publish the EOF state
    emitter.instruction("call __rt_stream_eof_set");                            // update only this stream's stable state
    emitter.label("__rt_fread_publish_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the successful byte count in the string-length register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer in the x86_64 elephc string-pointer result register
    // Legacy per-descriptor slots still serve the filter families not yet moved
    // onto the chain (user filters, zlib/bzip2/iconv). A filter lives in exactly
    // one mechanism, so running both is correct for the duration of the migration.
    // -- apply attached read filters (2-slot chain) --
    //    Slot 0 = _stream_read_filters[fd], slot 1 = _stream_read_filters[fd+256]
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the file descriptor
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // materialize the read-filter table base
    // -- slot 0 --
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // read filter id for slot 0
    emitter.instruction("test rcx, rcx");                                       // is slot 0 empty?
    emitter.instruction("jz __rt_fread_slot1_x86");                             // skip slot 0 when empty
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range?
    emitter.instruction("jl __rt_fread_builtin_slot0_x86");                     // built-in filter
    emitter.instruction("mov rdi, r10");                                        // fd
    emitter.instruction("mov rsi, rax");                                        // buf ptr
    // rdx already holds the byte count
    emitter.instruction("xor ecx, ecx");                                        // direction = 0 (read)
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← transformed
    emitter.instruction("jmp __rt_fread_slot1_x86");                            // proceed to slot 1
    emitter.label("__rt_fread_builtin_slot0_x86");
    emitter.instruction("call __rt_apply_stream_filter");                       // transform in place
    // -- slot 1 --
    emitter.label("__rt_fread_slot1_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload fd
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // materialize the read-filter table base
    emitter.instruction("lea rcx, [r10 + 256]");                                // fd+256 (slot 1 index)
    emitter.instruction("movzx ecx, BYTE PTR [r11 + rcx]");                     // read filter id for slot 1
    emitter.instruction("test rcx, rcx");                                       // is slot 1 empty?
    emitter.instruction("jz __rt_fread_ret_x86");                               // skip slot 1 when empty
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range?
    emitter.instruction("jl __rt_fread_builtin_slot1_x86");                     // built-in filter
    emitter.instruction("mov rdi, r10");                                        // fd
    emitter.instruction("mov rsi, rax");                                        // buf ptr (post slot-0)
    // rdx already holds the byte count (post slot-0)
    emitter.instruction("xor ecx, ecx");                                        // direction = 0 (read)
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← transformed
    emitter.instruction("jmp __rt_fread_ret_x86");                              // common epilogue
    emitter.label("__rt_fread_builtin_slot1_x86");
    emitter.instruction("call __rt_apply_stream_filter");                       // transform in place
    emitter.label("__rt_fread_ret_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // rcx = 0 only when the read failed
    emitter.instruction("add rsp, 64");                                         // release the fread() spill slots before returning the successful string slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the successful fread() path
    emitter.instruction("ret");                                                 // return the borrowed concat-buffer string slice to the caller

    emitter.label("__rt_fread_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_fread_would_block_x86");                       // transient nonblocking miss returns empty without EOF
    // See the AArch64 counterpart: php names the function, the chunk size, the errno and its
    // text. The 16 bytes keep the two calls on SysV's boundary; the slots are rbp-relative, so
    // nothing below shifts. This arm holds back no bytes, so the concat offset needs no nudge.
    emitter.instruction("movsxd r10, r10d");                                    // widen the errno before it outlives r10d
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // park the errno across the calls
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_chunk_size");                         // the count php names, not the syscall's
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, QWORD PTR [rsp]");                            // the errno
    emitter.instruction("call __rt_read_failed_notice");
    emitter.instruction("add rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // php answers false for a read that fails
    emitter.instruction("jmp __rt_fread_failed_x86");                           // a FAILED read does not exhaust the stream

    emitter.label("__rt_fread_would_block_x86");
    // Whatever the holding area already supplied stands: php does not lose bytes it has served
    // out of its buffer because the descriptor then had nothing more to give.
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_would_block_empty_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("jmp __rt_fread_publish_total_x86");
    emitter.label("__rt_fread_would_block_empty_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer for an empty transient read
    emitter.instruction("xor edx, edx");                                        // return a zero-length read result without setting EOF
    emitter.instruction("mov ecx, 1");                                          // a would-block is an empty READ, not a failure
    emitter.instruction("add rsp, 64");                                         // release the fread() spill slots before returning the empty string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the would-block fread() path
    emitter.instruction("ret");                                                 // return the empty non-EOF read result

    // A failed read: the same empty result as EOF, but the stream is NOT marked exhausted.
    emitter.label("__rt_fread_failed_x86");
    // See the would-block path: bytes already served from the holding area are still the answer.
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_failed_empty_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // a partial answer is a real result, not php false
    emitter.instruction("jmp __rt_fread_publish_total_x86");
    emitter.label("__rt_fread_failed_empty_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the reserved destination, so the pointer stays valid
    emitter.instruction("xor edx, edx");                                        // no bytes were read
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // rcx = 0: the caller boxes PHP false
    emitter.instruction("add rsp, 64");                                         // release the fread() spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failed-read result

    emitter.label("__rt_fread_eof_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov esi, 1");                                          // publish the EOF state
    emitter.instruction("call __rt_stream_eof_set");                            // mark this stream exhausted after the zero-byte or failed read
    // The descriptor is spent, but the holding area may have already answered part of this very
    // read: those bytes ARE the result, and the stream is at its end behind them.
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_eof_empty_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("jmp __rt_fread_publish_total_x86");
    emitter.label("__rt_fread_eof_empty_x86");
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when libc read() reports EOF or failure
    emitter.instruction("xor edx, edx");                                        // return an empty string length when libc read() reports EOF or failure
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // EOF and failure share this path; the slot tells them apart
    emitter.instruction("add rsp, 64");                                         // release the fread() spill slots before returning the empty-string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the EOF/error fread() path
    emitter.instruction("ret");                                                 // return the empty string result for the exhausted or failed stream read
}
