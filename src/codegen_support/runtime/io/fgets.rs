//! Purpose:
//! Emits the `__rt_fgets`, `__rt_fgets_fd_ok` runtime helper assembly for fgets.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native line reads resolve their descriptor from an opaque stream handle
//!   and publish exhaustion on the corresponding `StreamState`.
//! - `fgets()` has no caller-supplied length bound, so the line accumulates into a
//!   `__rt_concat_reserve` window that doubles through `__rt_concat_grow` whenever it fills.
//!   A line longer than the 64 KiB concat scratch therefore produces owned heap storage
//!   instead of running past `_concat_buf` into the adjacent BSS globals.
//! - A stream carrying a read-filter chain sources its bytes from `__rt_fread` instead of the
//!   descriptor. A filter is attached to the STREAM, so every reader must pull through it;
//!   reading the descriptor handed back the RAW bytes, and left the chain without the closing
//!   dispatch that `__rt_stream_eof_get` waits on before it will report `feof()` true.
//!   Only the byte SOURCE changes: the newline scan, the caller's bound and the growth of the
//!   line window are the same code on both paths, so the two cannot drift apart.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;
use crate::codegen_support::runtime::resources::layout::{
    STREAM_PENDING_LEN_OFFSET, STREAM_PENDING_POS_OFFSET, STREAM_PENDING_PTR_OFFSET,
    STREAM_READ_FILTER_HEAD_OFFSET,
};

/// Initial reserved line capacity, in bytes. Long enough that ordinary text lines never grow,
/// small enough that a `while (fgets($f))` loop still stays inside the shared concat scratch.
const INITIAL_LINE_CAPACITY: usize = 4096;

/// Reads one line from an opaque stream handle into the reserved line window.
/// dispatches to `emit_fgets_linux_x86_64` on x86_64; falls through to ARM64
/// path otherwise.
///
/// ABI contract:
///   - input:  x0 = opaque stream handle, x1 = caller's length bound (0 = read a whole line)
///     Both inputs are REQUIRED. `x1` was added with `fgets($length)`, and the two callers that
///     read a whole line — `readline()` and `fscanf()` — kept passing only the handle, so the
///     bound was whatever the register held: on linux-x86_64 `readline()` returned the first two
///     bytes of its line.
///   - output: x1 = pointer to line start (concat scratch or owned heap storage),
///             x2 = line length (includes trailing `\n` if one was present before EOF)
///   - side effect: sets state-owned EOF when the stream is exhausted
///   - the reservation is published with the final line length, so the shared concat
///     offset only moves for scratch-backed lines; a line may be partial if EOF or a
///     read error interrupts the stream
pub fn emit_fgets(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fgets_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    // -- set up stack frame --
    // Frame: [0]=opaque stream handle [8]=line length [16]=line buffer pointer
    //        [24]=line capacity [32]=wrapper byte scratch [40]=backend descriptor
    //        [48]=caller's length bound (0 = unbounded) [56]=read-filter chain head
    //        [64]=this iteration's destination, held across the pending-bytes probe
    emitter.instruction("sub sp, sp, #112");                                    // allocate the frame plus the bound and destination slots
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish new frame pointer
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the caller's length bound

    // -- save the handle and resolve the backend descriptor --
    // Resolution is a call, so it has to happen inside the frame; an invalid stream
    // therefore leaves through the framed epilogue rather than a bare `ret`.
    emitter.instruction("str x0, [sp, #0]");                                    // save the opaque stream handle
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor through StreamState
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the resolved backend descriptor

    // -- does the stream carry a read-filter chain? --
    // Probed once, up front: the answer picks the byte source for every iteration of the
    // loop below, and re-resolving the state per byte would put a call in the hot path.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable stream state
    emitter.instruction("cbz x0, __rt_fgets_unfiltered");                       // no state: nothing can be attached to it
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_READ_FILTER_HEAD_OFFSET}]")); // read-direction chain head
    emitter.instruction("str x9, [sp, #56]");                                   // remember whether the chain exists
    emitter.instruction("b __rt_fgets_filter_probed");                          // continue with the descriptor check
    emitter.label("__rt_fgets_unfiltered");
    emitter.instruction("str xzr, [sp, #56]");                                  // an unfiltered stream keeps the descriptor path
    emitter.label("__rt_fgets_filter_probed");

    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the resolved backend descriptor
    emitter.instruction("cmp x0, #0");                                          // did descriptor resolution succeed?
    emitter.instruction("b.ge __rt_fgets_fd_ok");                               // continue for a valid backend descriptor
    emitter.instruction("mov x1, #0");                                          // return empty string: null pointer
    emitter.instruction("mov x2, #0");                                          // return empty string: zero length
    emitter.instruction("b __rt_fgets_return");                                 // leave through the framed epilogue

    // -- reserve an initial line window --
    emitter.label("__rt_fgets_fd_ok");
    emitter.instruction(&format!("mov x9, #{}", INITIAL_LINE_CAPACITY));        // start from the initial line capacity
    emitter.instruction("str x9, [sp, #24]");                                   // save the current line capacity
    emitter.instruction("mov x0, x9");                                          // request the initial capacity from the reservation front end
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the line
    emitter.instruction("str x0, [sp, #16]");                                   // save start pointer for return value
    emitter.instruction("mov x1, x0");                                          // publish the reservation start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // publish the full reserved capacity
    emitter.instruction("bl __rt_concat_publish");                              // claim the whole window so nested reads append after it
    emitter.instruction("str xzr, [sp, #8]");                                   // the line starts empty

    // -- a read-filter chain outranks the backend: __rt_fread pulls through it either way --
    // `__rt_fread_raw` resolves the wrapper range itself, so the filtered branch below serves
    // a userspace-wrapper stream exactly as it serves a native one.
    emitter.instruction("ldr x9, [sp, #56]");                                   // the read-direction chain head
    emitter.instruction("cbnz x9, __rt_fgets_loop");                            // filtered: the loop reads through the chain

    // -- user-wrapper fd: read the line through stream_read instead of read() --
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the backend descriptor
    emitter.instruction("mov w9, #0x4000");                                     // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w9, w9, #16");                                     // form 0x40000000 in w9
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.lo __rt_fgets_loop");                                // native descriptors use the byte-read loop below
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("add x10, x9, x10");                                    // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp x0, x10");                                         // is the backend above the wrapper range?
    emitter.instruction("b.lo __rt_fgets_wrapper_entry");                       // wrappers read via the feof-gated stream_read loop below

    // -- read loop: one byte at a time until \n or EOF --
    emitter.label("__rt_fgets_loop");

    // -- the held bytes are scanned WHERE THEY LIE, and the line is taken in ONE call --
    //
    // The loop below advances ONE BYTE per iteration, and each of those bytes costs a call that
    // resolves the stream state before it copies anything. That is what a line costs: `fgets()`
    // over a 900 KB file measured 420 ms against php's 4 ms — a hundredfold, on the most ordinary
    // way there is to read a file. Nothing about the ANSWER changes here: the same newline scan
    // runs over the same bytes in the same order, and stops after the same one.
    //
    // The byte path below is untouched and still does every job this cannot: it REFILLS the
    // holding area, grows the line window when a line outruns it, and serves a filtered stream.
    emitter.instruction("ldr x9, [sp, #56]");                                   // the read-direction chain head
    emitter.instruction("cbnz x9, __rt_fgets_loop_byte");                       // a filtered stream pulls through the chain
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");
    emitter.instruction("cbz x0, __rt_fgets_loop_byte");                        // no state: nothing can be held
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_fgets_loop_byte");                        // nothing held: the byte path refills
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("subs x12, x10, x11");                                  // bytes still held
    emitter.instruction("b.le __rt_fgets_loop_byte");                           // drained: the byte path refills
    emitter.instruction("add x9, x9, x11");                                     // the first byte still held
    emitter.instruction("ldr x13, [sp, #8]");                                   // the line so far
    emitter.instruction("ldr x14, [sp, #24]");                                  // the window already reserved
    emitter.instruction("subs x14, x14, x13");                                  // room without growing it
    emitter.instruction("b.le __rt_fgets_loop_byte");                           // full: the byte path grows the window
    emitter.instruction("cmp x12, x14");
    emitter.instruction("csel x12, x14, x12, gt");                              // take at most what fits
    emitter.instruction("ldr x14, [sp, #48]");                                  // the caller's bound (0 = unbounded)
    emitter.instruction("cbz x14, __rt_fgets_bulk_room_ok");
    emitter.instruction("sub x14, x14, #1");                                    // php fills `$length - 1` bytes
    emitter.instruction("subs x14, x14, x13");                                  // room left under the bound
    emitter.instruction("b.le __rt_fgets_done");                                // bound reached: hand back what we have
    emitter.instruction("cmp x12, x14");
    emitter.instruction("csel x12, x14, x12, gt");
    emitter.label("__rt_fgets_bulk_room_ok");
    emitter.instruction("ldr x15, [sp, #16]");                                  // the line window
    emitter.instruction("add x15, x15, x13");                                   // where this batch lands
    emitter.instruction("mov x16, #0");                                         // bytes taken this batch
    emitter.label("__rt_fgets_bulk_scan");
    emitter.instruction("cmp x16, x12");
    emitter.instruction("b.ge __rt_fgets_bulk_copied");                         // the batch is exhausted
    emitter.instruction("ldrb w17, [x9, x16]");                                 // one held byte
    emitter.instruction("strb w17, [x15, x16]");                                // appended to the line
    emitter.instruction("add x16, x16, #1");
    emitter.instruction("cmp w17, #0x0A");                                      // php keeps the newline and stops AFTER it
    emitter.instruction("b.ne __rt_fgets_bulk_scan");
    emitter.label("__rt_fgets_bulk_copied");
    emitter.instruction("cbz x16, __rt_fgets_loop_byte");                       // nothing taken after all
    emitter.instruction("str x16, [sp, #80]");                                  // the count must survive the consume call
    emitter.instruction("sub x17, x16, #1");
    emitter.instruction("ldrb w17, [x9, x17]");                                 // the last byte taken
    emitter.instruction("cmp w17, #0x0A");
    emitter.instruction("cset x17, eq");                                        // did the line end here?
    emitter.instruction("str x17, [sp, #88]");
    emitter.instruction("ldr x13, [sp, #8]");
    emitter.instruction("add x13, x13, x16");                                   // the line grew by the batch
    emitter.instruction("str x13, [sp, #8]");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("ldr x1, [sp, #80]");                                   // and what this batch consumed
    emitter.instruction("bl __rt_stream_pending_consume");
    emitter.instruction("ldr x17, [sp, #88]");
    emitter.instruction("cbnz x17, __rt_fgets_done");                           // the newline ends the line
    emitter.instruction("b __rt_fgets_loop");

    emitter.label("__rt_fgets_loop_byte");

    // -- stop before reading once the caller's bound is reached --
    // PHP reads at most `$length - 1` bytes, so a bound of 1 returns nothing and `fgets()`
    // reports false. Zero means the caller passed no bound.
    emitter.instruction("ldr x9, [sp, #48]");                                   // the caller's bound
    emitter.instruction("cbz x9, __rt_fgets_unbounded");                        // no bound: read to newline or EOF
    emitter.instruction("sub x9, x9, #1");                                      // at most bound - 1 bytes
    emitter.instruction("ldr x10, [sp, #8]");                                   // bytes accumulated so far
    emitter.instruction("cmp x10, x9");                                         // is the line already at the bound?
    emitter.instruction("b.hs __rt_fgets_done");                                // hand back what we have
    emitter.label("__rt_fgets_unbounded");

    // -- make room for one more byte before reading it --
    emitter.instruction("ldr x9, [sp, #8]");                                    // current line length
    emitter.instruction("add x9, x9, #1");                                      // capacity needed once the next byte lands
    emitter.instruction("ldr x10, [sp, #24]");                                  // current line capacity
    emitter.instruction("cmp x9, x10");                                         // does the next byte still fit the reservation?
    emitter.instruction("b.ls __rt_fgets_have_room");                           // no growth needed for this iteration
    emitter.instruction("lsl x10, x10, #1");                                    // double the line capacity
    emitter.instruction("str x10, [sp, #24]");                                  // save the grown line capacity
    emitter.instruction("ldr x1, [sp, #16]");                                   // old line buffer pointer
    emitter.instruction("mov x2, #0");                                          // release the whole claimed window
    emitter.instruction("bl __rt_concat_publish");                              // hand the old scratch window back before moving to heap storage
    emitter.instruction("ldr x0, [sp, #16]");                                   // old line buffer pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // bytes accumulated so far must survive the move
    emitter.instruction("ldr x2, [sp, #24]");                                   // grown line capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the accumulated line into a larger owned buffer
    emitter.instruction("str x0, [sp, #16]");                                   // save the grown line buffer pointer
    emitter.label("__rt_fgets_have_room");
    emitter.instruction("ldr x9, [sp, #56]");                                   // the read-direction chain head
    emitter.instruction("cbnz x9, __rt_fgets_filtered_byte");                   // filtered: take the byte from the chain
    emitter.instruction("ldr x1, [sp, #16]");                                   // line buffer base pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // current line length
    emitter.instruction("add x1, x1, x10");                                     // buf pointer for read syscall

    // -- a refused `stream_get_line()` may have left bytes ON this stream --
    //
    // They are taken ONE AT A TIME, through the same slot the syscall writes, so the newline scan
    // below sees them exactly as it sees a byte off the descriptor. A bulk copy would skip that
    // scan, and those bytes CAN contain a newline: `stream_get_line($h, 100, "|")` refuses on its
    // own delimiter, not on `\n`. A stream that never hit the refusal holds nothing, so this is
    // one call that returns 0 on its first load.
    emitter.instruction("str x1, [sp, #64]");                                   // the destination this iteration writes
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("mov x2, #1");                                          // one held byte
    emitter.instruction("bl __rt_stream_pending_take");                         // x0 = 1 when one came back
    emitter.instruction("cbnz x0, __rt_fgets_byte_ready");                      // join the shared newline scan

    // -- nothing held: fill the holding area a CHUNK at a time, the way php does --
    //
    // A line reader cannot ask for the bytes it wants, because it does not know how many there
    // are until it finds the newline. So this loop asked for ONE, and paid a `read(2)` for it:
    // 538 ms over a 900 KB file of 100 000 lines, where php takes 7 ms. Filling the stream's own
    // holding area is what php does with every read, and it is what makes the scan above find a
    // whole line in memory instead of a byte on a descriptor.
    //
    // A fill that buffers nothing — a pipe, a failed read, end of file — falls straight through
    // to the single-byte read below, which keeps every EOF and errno answer exactly where it was.
    emitter.instruction("ldr x0, [sp, #40]");                                   // the backend descriptor
    emitter.instruction("bl __rt_stream_fd_is_bufferable");                     // a file, a socket or a fifo: php buffers all three
    emitter.instruction("cbz x0, __rt_fgets_read_one");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_pending_fill");                         // x0 = bytes now held
    emitter.instruction("cbnz x0, __rt_fgets_loop");                            // the scan above takes the line out of them

    emitter.label("__rt_fgets_read_one");
    emitter.instruction("ldr x1, [sp, #64]");                                   // the destination again

    // -- read 1 byte via syscall --
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the backend descriptor for the read syscall
    emitter.instruction("mov x2, #1");                                          // read exactly 1 byte
    emitter.syscall(3);

    // -- check if read failed or returned 0 (EOF) --
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: check return value
        emitter.instruction("b.eq __rt_fgets_eof");                             // zero-byte read means EOF
        emitter.instruction("b.lt __rt_fgets_read_failed");                     // negative result: inspect errno before setting EOF
    } else {
        emitter.instruction("b.cs __rt_fgets_read_failed");                     // macOS: if carry set, inspect errno before setting EOF
        emitter.instruction("cbz x0, __rt_fgets_eof");                          // if 0 bytes read, we hit EOF
    }
    emitter.instruction("b __rt_fgets_byte_ready");                             // join the shared newline scan

    // -- filtered byte source: one byte at a time out of the chain's buffered output --
    // `__rt_fread` is the only helper that runs the chain, caps the result at the requested
    // length and keeps the remainder on the stream, so asking it for a single byte is what
    // makes this loop see filtered bytes at all. The reservation it hands back is released
    // immediately: the byte is copied into the line window first, exactly as the userspace
    // wrapper loop below does with its own chunks.
    emitter.label("__rt_fgets_filtered_byte");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle, not the descriptor
    emitter.instruction("mov x1, #1");                                          // one filtered byte
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_fgets_eof");                              // the chain is drained and flushed: EOF
    emitter.instruction("ldrb w13, [x1]");                                      // the filtered byte
    emitter.instruction("ldr x11, [sp, #16]");                                  // line buffer base pointer
    emitter.instruction("ldr x12, [sp, #8]");                                   // current line length
    emitter.instruction("strb w13, [x11, x12]");                                // append it to the line
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand this byte's scratch window back before the next read
    emitter.instruction("mov x0, x1");                                          // chunk ptr for release
    emitter.instruction("bl __rt_decref_any");                                  // release the chunk before continuing the line

    // -- count the byte just appended to the line --
    emitter.label("__rt_fgets_byte_ready");
    emitter.instruction("ldr x11, [sp, #16]");                                  // line buffer base pointer
    emitter.instruction("ldr x13, [sp, #8]");                                   // offset of byte just read
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the byte we just read
    emitter.instruction("add x13, x13, #1");                                    // advance the line length by 1 byte
    emitter.instruction("str x13, [sp, #8]");                                   // store the updated line length

    // -- check if the byte we just read is \n --
    emitter.instruction("cmp w14, #0x0A");                                      // compare with newline character
    emitter.instruction("b.eq __rt_fgets_done");                                // if newline, line is complete
    emitter.instruction("b __rt_fgets_loop");                                   // otherwise continue reading

    // -- user-wrapper line read: feof-gated stream_read, one byte at a time.
    //    feof is checked BEFORE each read so the loop never makes the EOF read
    //    whose empty result would cross the wrapper-method boundary. Bytes are
    //    appended to _user_wrapper_drain_buf — NOT _concat_buf, because each
    //    __rt_fread result may itself occupy _concat_buf and clobber the line.
    //    [sp,#8] tracks the accumulated line length; the line is returned
    //    directly (this path does not reuse the _concat_buf-based done label).
    emitter.label("__rt_fgets_wrapper_entry");
    emitter.instruction("ldr x1, [sp, #16]");                                   // the wrapper path accumulates elsewhere, so give the reservation back
    emitter.instruction("mov x2, #0");                                          // release the whole claimed window
    emitter.instruction("bl __rt_concat_publish");                              // hand the unused line window back to the shared scratch buffer
    emitter.instruction("str xzr, [sp, #8]");                                   // line length = 0
    // php reads a wrapper in CHUNKS and buffers what the line does not need: a wrapper serving 100
    // bytes at a chunk size of 17 receives SIX `stream_read()` calls however many `fgets()` calls
    // consume it. elephc asked for one byte per iteration, so the same read took a HUNDRED calls
    // into user code. Measured on `php -n` 8.5.6.
    //
    // The leftovers go into the same holding area a refused `stream_get_line()` uses, which is
    // php's stream read buffer: they survive the call, and `fread()` sees them too.
    emitter.label("__rt_fgets_wrapper_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("add x1, sp, #32");                                     // the byte scratch this loop already owns
    emitter.instruction("mov x2, #1");                                          // one held byte
    emitter.instruction("bl __rt_stream_pending_take");
    emitter.instruction("cbnz x0, __rt_fgets_wrapper_have_byte");               // a buffered byte needs no wrapper call

    // php's `fgets()` asks the class NOTHING: it fills, and reads what it already knows. See
    // `emit_stream_eof_known` — asking here put a question to the class that php never puts.
    // It takes the HANDLE: driven by the descriptor it resolves no state and can never say yes.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_eof_known");
    emitter.instruction("cbnz x0, __rt_fgets_wrapper_done");                    // at EOF: return the bytes gathered so far
    emitter.instruction("ldr x0, [sp, #0]");                                    // the handle carries the chunk size
    emitter.instruction("bl __rt_stream_chunk_size");                           // x0 = how much php would ask for
    emitter.instruction("mov x1, x0");                                          // that is the read length
    // The HANDLE, not the descriptor: the read posts the wrapper's end-of-file answer on the
    // stream, and only the handle reaches it. Driven by the fd that answer landed nowhere, and
    // the loop read an extra empty chunk at every line that did not end in a newline.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_fgets_wrapper_done");                     // an empty read ends the line
    emitter.instruction("stp x1, x2, [sp, #64]");                               // the chunk outlives the release calls
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_pending_put");                          // the whole chunk belongs to the stream
    emitter.instruction("ldr x1, [sp, #64]");                                   // the chunk pointer
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand the scratch window back before the next read
    emitter.instruction("ldr x0, [sp, #64]");                                   // chunk ptr for release
    emitter.instruction("bl __rt_decref_any");                                  // the copy on the stream is the one that lives
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("add x1, sp, #32");                                     // the byte scratch
    emitter.instruction("mov x2, #1");                                          // take the first buffered byte
    emitter.instruction("bl __rt_stream_pending_take");
    emitter.instruction("cbz x0, __rt_fgets_wrapper_done");                     // defensive: nothing came back

    emitter.label("__rt_fgets_wrapper_have_byte");
    emitter.instruction("ldrb w13, [sp, #32]");                                 // the byte this iteration consumes
    emitter.instruction("ldr x10, [sp, #8]");                                   // current line length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_user_wrapper_drain_buf");
    emitter.instruction("strb w13, [x12, x10]");                                // append the byte to the line buffer
    emitter.instruction("add x10, x10, #1");                                    // advance the line length
    emitter.instruction("str x10, [sp, #8]");                                   // store the updated line length
    emitter.instruction("cmp w13, #0x0A");                                      // is the byte a newline?
    emitter.instruction("b.eq __rt_fgets_wrapper_done");                        // newline: finish the line
    // php's OTHER stopping condition, which this loop did not have: it fills at most `$length - 1`
    // bytes. Without it `fgets($h, 6)` on a wrapper answered the whole line — a wrong VALUE, and a
    // silent one, since the call still returns a string.
    emitter.instruction("ldr x11, [sp, #48]");                                  // the caller's bound (0 = unbounded)
    emitter.instruction("cbz x11, __rt_fgets_wrapper_loop");                    // unbounded: only the newline stops this
    emitter.instruction("sub x11, x11, #1");                                    // php fills `$length - 1` bytes
    emitter.instruction("cmp x10, x11");                                        // is the line already that long?
    emitter.instruction("b.ge __rt_fgets_wrapper_done");                        // bound reached: finish the line
    emitter.instruction("b __rt_fgets_wrapper_loop");                           // read the next byte
    emitter.label("__rt_fgets_wrapper_done");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_user_wrapper_drain_buf"); // line pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // line length
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return the wrapper line (ptr/len)

    // -- nonblocking read miss: return accumulated bytes without EOF --
    emitter.label("__rt_fgets_read_failed");
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", emitter.platform.would_block_errno())); // Linux: compare read result with -EAGAIN/-EWOULDBLOCK
    } else {
        emitter.instruction(&format!("cmp x0, #{}", emitter.platform.would_block_errno())); // macOS: compare errno with EAGAIN/EWOULDBLOCK
    }
    emitter.instruction("b.eq __rt_fgets_done");                                // would-block returns the partial line, not EOF

    // -- EOF reached: publish state-owned EOF for this stream --
    emitter.label("__rt_fgets_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("mov x1, #1");                                          // publish the EOF state
    emitter.instruction("bl __rt_stream_eof_set");                              // update only this stream's stable state

    // -- return result string --
    emitter.label("__rt_fgets_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the accumulated line length
    emitter.instruction("bl __rt_concat_publish");                              // shrink the claimed window down to the bytes actually read

    // -- `php://temp` calls a drained line read the end of the stream --
    // AFTER the publish, never before: the probe fills the holding area, and that fill claims a
    // concat window of its own. Run while this line still held one, it would build its chunk on
    // top of the line and hand back bytes that had moved.
    emitter.instruction("stp x1, x2, [sp, #72]");                               // the line survives the probe's own calls
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_temp_eof_probe");                       // one extra read, for the one wrapper that takes it
    emitter.instruction("ldp x1, x2, [sp, #72]");                               // restore the line pointer and length

    // -- restore frame and return --
    emitter.label("__rt_fgets_return");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64-specific fgets: mirrors the ARM64 path using x86_64 System V ABI.
/// Input:  rdi = opaque stream handle
/// Output: rax = line start pointer (concat scratch or owned heap storage), rdx = line length
/// Side effect: sets state-owned EOF when the stream is exhausted
fn emit_fgets_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    // Frame: [rbp-8]=handle [rbp-16]=line length [rbp-24]=line pointer [rbp-32]=wrapper
    //        chunk pointer [rbp-40]=line capacity [rbp-48]=byte scratch [rbp-56]=descriptor
    //        [rbp-64]=caller's length bound (0 = unbounded) [rbp-72]=read-filter chain head
    //        [rbp-80]=this iteration's destination, held across the pending-bytes probe
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fgets() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved handle and line metadata
    emitter.instruction("sub rsp, 96");                                         // reserve the read-loop temporaries plus the bound and destination slots

    // Resolution is a call, so it has to happen inside the frame; an invalid stream
    // therefore leaves through the framed epilogue rather than a bare `ret`.
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 64], rsi");                       // preserve the caller's length bound
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor through StreamState
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the resolved backend descriptor

    // -- does the stream carry a read-filter chain? --
    // See the AArch64 counterpart: probed once, because the answer picks the byte source for
    // every iteration of the loop and re-resolving the state per byte would cost a call.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // rax = the stable stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fgets_unfiltered_x86");                        // no state: nothing can be attached to it
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_READ_FILTER_HEAD_OFFSET}]")); // read-direction chain head
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // remember whether the chain exists
    emitter.instruction("jmp __rt_fgets_filter_probed_x86");                    // continue with the descriptor check
    emitter.label("__rt_fgets_unfiltered_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // an unfiltered stream keeps the descriptor path
    emitter.label("__rt_fgets_filter_probed_x86");

    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the resolved backend descriptor
    emitter.instruction("test rax, rax");                                       // did descriptor resolution succeed?
    emitter.instruction("jns __rt_fgets_fd_ok_x86");                            // continue for a valid backend descriptor
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer for an invalid stream
    emitter.instruction("xor edx, edx");                                        // return an empty string length for an invalid stream
    emitter.instruction("jmp __rt_fgets_return_x86");                           // leave through the framed epilogue

    emitter.label("__rt_fgets_fd_ok_x86");
    emitter.instruction(&format!("mov QWORD PTR [rbp - 40], {}", INITIAL_LINE_CAPACITY)); // start from the initial line capacity
    emitter.instruction(&format!("mov rax, {}", INITIAL_LINE_CAPACITY));        // request the initial capacity from the reservation front end
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the line
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the line start pointer for the final elephc string result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // publish the full reserved capacity
    emitter.instruction("call __rt_concat_publish");                            // claim the whole window so nested reads append after it
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // the line starts empty

    // -- a read-filter chain outranks the backend: __rt_fread pulls through it either way --
    // See the AArch64 counterpart: `__rt_fread_raw` resolves the wrapper range itself.
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // the read-direction chain head
    emitter.instruction("jne __rt_fgets_loop_x86");                             // filtered: the loop reads through the chain

    // -- user-wrapper fd: read the line through stream_read instead of read() --
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the backend descriptor
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jb __rt_fgets_loop_x86");                              // native descriptors use the byte-read loop below
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("add r10, r9");                                         // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp rax, r10");                                        // is the backend above the wrapper range?
    emitter.instruction("jb __rt_fgets_wrapper_entry_x86");                     // wrappers read via the feof-gated stream_read loop below

    emitter.label("__rt_fgets_loop_x86");

    // -- stop before reading once the caller's bound is reached --
    // See the AArch64 counterpart: PHP reads at most `$length - 1` bytes, and zero means
    // the caller passed no bound at all.
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // the caller's bound
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_fgets_unbounded_x86");                         // no bound: read to newline or EOF
    emitter.instruction("sub r9, 1");                                           // at most bound - 1 bytes
    emitter.instruction("cmp QWORD PTR [rbp - 16], r9");                        // is the line already at the bound?
    emitter.instruction("jae __rt_fgets_done_x86");                             // hand back what we have
    emitter.label("__rt_fgets_unbounded_x86");

    // -- make room for one more byte before reading it --
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // current line length
    emitter.instruction("add r8, 1");                                           // capacity needed once the next byte lands
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // current line capacity
    emitter.instruction("cmp r8, r9");                                          // does the next byte still fit the reservation?
    emitter.instruction("jbe __rt_fgets_have_room_x86");                        // no growth needed for this iteration
    emitter.instruction("add r9, r9");                                          // double the line capacity
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the grown line capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // old line buffer pointer
    emitter.instruction("xor edx, edx");                                        // release the whole claimed window
    emitter.instruction("call __rt_concat_publish");                            // hand the old scratch window back before moving to heap storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // old line buffer pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // bytes accumulated so far must survive the move
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // grown line capacity
    emitter.instruction("call __rt_concat_grow");                               // move the accumulated line into a larger owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the grown line buffer pointer
    emitter.label("__rt_fgets_have_room_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // the read-direction chain head
    emitter.instruction("jne __rt_fgets_filtered_byte_x86");                    // filtered: take the byte from the chain
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // line buffer base pointer
    emitter.instruction("add rsi, QWORD PTR [rbp - 16]");                       // compute the address where libc read() should append the next byte

    // -- a refused `stream_get_line()` may have left bytes ON this stream --
    // See the AArch64 counterpart: taken ONE AT A TIME, through the slot libc read() writes, so
    // the newline scan below sees them exactly as it sees a byte off the descriptor.
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // the destination this iteration writes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov edx, 1");                                          // one held byte
    emitter.instruction("call __rt_stream_pending_take");                       // rax = 1 when one came back
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_fgets_read_ok_x86");                          // join the shared newline scan
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // the destination again

    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // pass the resolved backend descriptor as the first libc read() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one byte so fgets() can stop on the first newline
    emitter.instruction("call read");                                           // read one byte from the stream through libc read() into the concat buffer
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as a byte, EOF, or failure
    emitter.instruction("jg __rt_fgets_read_ok_x86");                           // positive byte count: publish the appended byte
    emitter.instruction("jl __rt_fgets_read_failed_x86");                       // negative result: inspect errno before setting EOF
    emitter.instruction("jmp __rt_fgets_eof_x86");                              // zero-byte read means real EOF

    // -- filtered byte source: one byte at a time out of the chain's buffered output --
    // See the AArch64 counterpart: `__rt_fread` is the only helper that runs the chain and
    // keeps the remainder on the stream, so a one-byte request is what makes this loop see
    // filtered bytes. The byte is copied into the line window before the chunk is released.
    emitter.label("__rt_fgets_filtered_byte_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle, not the descriptor
    emitter.instruction("mov esi, 1");                                          // one filtered byte
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_fgets_eof_x86");                               // the chain is drained and flushed: EOF
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the chunk ptr across the release calls
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // the filtered byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // line buffer base pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // the byte's destination inside the line
    emitter.instruction("mov BYTE PTR [r10], cl");                              // append it to the line
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand this byte's scratch window back before the next read
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // chunk ptr for release
    emitter.instruction("call __rt_decref_any");                                // release the chunk before continuing the line

    emitter.label("__rt_fgets_read_ok_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // line buffer base pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // offset of the byte just read
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // load the byte that was just appended to the line
    emitter.instruction("add r10, 1");                                          // advance the line length by the one byte that libc read() appended
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // store the updated line length
    emitter.instruction("cmp cl, 0x0A");                                        // did the newly appended byte terminate the line with a newline?
    emitter.instruction("jne __rt_fgets_loop_x86");                             // keep reading until fgets() hits newline, EOF, or read failure

    emitter.label("__rt_fgets_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the reserved start pointer for the fgets() line
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the accumulated line length in the x86_64 elephc string-length result register
    emitter.instruction("call __rt_concat_publish");                            // shrink the claimed window down to the bytes actually read

    // -- `php://temp` calls a drained line read the end of the stream --
    // See the AArch64 counterpart: after the publish, because the probe's own fill claims a
    // concat window and would otherwise build it on top of this line.
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // the line survives the probe's own calls
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_temp_eof_probe");                     // one extra read, for the one wrapper that takes it
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // restore the line pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // restore the line length
    emitter.label("__rt_fgets_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 fgets() helper completes
    emitter.instruction("ret");                                                 // return the fgets() line slice to the caller

    // -- user-wrapper line read: feof-gated stream_read, one byte at a time.
    //    feof is checked BEFORE each read so the loop never makes the EOF read
    //    whose empty result would cross the wrapper-method boundary. Bytes are
    //    appended to _user_wrapper_drain_buf — NOT _concat_buf, because each
    //    __rt_fread result may itself occupy _concat_buf and clobber the line.
    //    [rbp-16] tracks the accumulated line length; the line is returned
    //    directly (this path does not reuse the _concat_buf-based done label).
    emitter.label("__rt_fgets_wrapper_entry_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the wrapper path accumulates elsewhere, so give the reservation back
    emitter.instruction("xor edx, edx");                                        // release the whole claimed window
    emitter.instruction("call __rt_concat_publish");                            // hand the unused line window back to the shared scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // line length = 0
    // See the AArch64 counterpart: php reads a wrapper in CHUNKS and buffers what the line does not
    // need, so 100 bytes at a chunk size of 17 cost SIX `stream_read()` calls, not a hundred.
    emitter.label("__rt_fgets_wrapper_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("lea rsi, [rbp - 48]");                                 // the byte scratch this loop already owns
    emitter.instruction("mov rdx, 1");                                          // one held byte
    emitter.instruction("call __rt_stream_pending_take");
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_fgets_wrapper_have_byte_x86");                // a buffered byte needs no wrapper call

    // See the AArch64 arm: it takes the HANDLE, not the descriptor.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_eof_known");
    emitter.instruction("test rax, rax");                                       // at EOF?
    emitter.instruction("jnz __rt_fgets_wrapper_done_x86");                     // at EOF: return the bytes gathered so far
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the handle carries the chunk size
    emitter.instruction("call __rt_stream_chunk_size");                         // rax = how much php would ask for
    emitter.instruction("mov rsi, rax");                                        // that is the read length
    // See the AArch64 arm: the HANDLE, not the descriptor.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");                                       // zero-length read?
    emitter.instruction("jz __rt_fgets_wrapper_done_x86");                      // an empty read ends the line
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // the chunk outlives the release calls
    emitter.instruction("mov rsi, rax");                                        // the chunk bytes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_pending_put");                        // the whole chunk belongs to the stream
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the chunk pointer
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand the scratch window back before the next read
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // chunk ptr for release
    emitter.instruction("call __rt_decref_any");                                // the copy on the stream is the one that lives
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("lea rsi, [rbp - 48]");                                 // the byte scratch
    emitter.instruction("mov rdx, 1");                                          // take the first buffered byte
    emitter.instruction("call __rt_stream_pending_take");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fgets_wrapper_done_x86");                      // defensive: nothing came back

    emitter.label("__rt_fgets_wrapper_have_byte_x86");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 48]");                      // the byte this iteration consumes
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // current line length
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_drain_buf");        // line buffer base
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // append the byte to the line buffer
    emitter.instruction("add r10, 1");                                          // advance the line length
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // store the updated line length
    emitter.instruction("cmp cl, 0x0A");                                        // is the byte a newline?
    emitter.instruction("je __rt_fgets_wrapper_done_x86");                      // newline: finish the line
    // Same bound as the AArch64 arm above: php fills at most `$length - 1` bytes.
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // the caller's bound (0 = unbounded)
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_fgets_wrapper_loop_x86");                      // unbounded: only the newline stops this
    emitter.instruction("sub r9, 1");                                           // php fills `$length - 1` bytes
    emitter.instruction("cmp r10, r9");                                         // is the line already that long?
    emitter.instruction("jge __rt_fgets_wrapper_done_x86");                     // bound reached: finish the line
    emitter.instruction("jmp __rt_fgets_wrapper_loop_x86");                     // read the next byte
    emitter.label("__rt_fgets_wrapper_done_x86");
    abi::emit_symbol_address(emitter, "rax", "_user_wrapper_drain_buf");        // line pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // line length
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper line (ptr/len)

    emitter.label("__rt_fgets_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_fgets_done_x86");                              // would-block returns the partial line without setting EOF

    emitter.label("__rt_fgets_eof_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov esi, 1");                                          // publish the EOF state
    emitter.instruction("call __rt_stream_eof_set");                            // update only this stream's stable state
    emitter.instruction("jmp __rt_fgets_done_x86");                             // return the possibly empty slice accumulated before EOF or read failure
}
