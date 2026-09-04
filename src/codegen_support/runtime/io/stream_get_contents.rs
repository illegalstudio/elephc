//! Purpose:
//! Emits the `__rt_stream_get_contents` runtime helper assembly for stream_get_contents.
//! Reads every remaining byte from a stream through the same fread path used by
//! bounded reads.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The result never lives in the 64 KiB `_concat_buf`: AArch64 accumulates into an owned
//!   heap buffer and AArch64/x86_64 both grow on demand, so a stream larger than the shared
//!   scratch produces a complete string instead of running off the end of `_concat_buf` into
//!   the adjacent BSS globals.
//! - The read-all loop uses `__rt_fread` so TLS sessions, filters, and wrapper
//!   reads share one I/O dispatch path.

use crate::codegen_support::abi::emit_symbol_address;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Initial capacity of the AArch64 read-all accumulator; it doubles as needed.
const ACCUMULATOR_INITIAL_BYTES: u64 = 65536;
/// Initial x86_64 accumulation capacity, in bytes. Two read chunks wide so the first
/// `__rt_fread` never has to grow, and small enough that short streams stay in concat scratch.
const INITIAL_CAPACITY: usize = 8192;

/// Emits the read-all stream helper.
///
/// Input: `x0 = opaque stream handle`, `x1 = state-owned chunk size`.
/// Output: `x1 = string pointer`, `x2 = total bytes read`.
/// The helper loops through `__rt_fread`, copies each returned chunk into an accumulation
/// buffer that grows on demand, and stops when EOF or an empty read is produced.
pub fn emit_stream_get_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_contents_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    // -- set up stack frame --
    // Frame: [0]=handle [8]=accumulator capacity [16]=accumulator pointer
    //        [24]=total [32]=chunk ptr [40]=chunk len [48]=chunk size
    //        [56]=saved _concat_off [64]=growth scratch [96]=x29/x30
    emitter.instruction("sub sp, sp, #112");                                    // allocate locals plus saved frame pointer and return address
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the opaque stream handle
    emitter.instruction("str x1, [sp, #48]");                                   // save the state-owned read-loop chunk size

    // -- accumulate into a growable heap buffer, not the shared concat buffer --
    // The result used to be built inside `_concat_buf`, which caps at 64 KiB, so
    // reading a larger stream truncated it. Each chunk still lands in the shared
    // buffer (bounded by __rt_fread) and is copied straight out, so the shared
    // offset is restored to where it started.
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // current shared-buffer offset
    emitter.instruction("str x10, [sp, #56]");                                  // remember it for every chunk and for restore
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the running byte total to zero
    emitter.instruction(&format!("mov x0, #{ACCUMULATOR_INITIAL_BYTES}"));      // initial accumulator capacity
    emitter.instruction("str x0, [sp, #8]");                                    // publish the capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the accumulator
    emitter.instruction("str x0, [sp, #16]");                                   // publish the accumulator pointer
    emitter.instruction("cbz x0, __rt_stream_get_contents_done");               // heap exhausted: return an empty result

    // -- read 4096-byte chunks through fread until EOF --
    emitter.label("__rt_stream_get_contents_loop");
    // The slot holds the opaque handle: __rt_fread and __rt_feof both need it, and
    // passing a descriptor here silently defeated the filter chain because
    // __rt_stream_state could not resolve it. Only this range probe wants the
    // descriptor, so resolve it here rather than storing one.
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor for the range probe
    emitter.instruction("mov x14, x0");                                         // keep the descriptor for the comparison
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the handle for __rt_feof
    emitter.instruction("mov w11, #0x4000");                                    // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w11, w11, #16");                                   // form 0x40000000
    emitter.instruction("cmp x14, x11");                                        // synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_stream_get_contents_after_feof");            // normal fd: skip wrapper EOF dispatch
    // php does NOT gate the read on `stream_eof()` here: it keeps calling `stream_read()` until one
    // answers an EMPTY string. Measured on `php -n` 8.5.6, a wrapper serving 100 bytes at a chunk
    // size of 17 receives SEVEN calls — the seventh returning "" — and one whose `stream_eof()`
    // never answers true still stops after the read that comes back empty. Gating here made the
    // last call disappear, which a wrapper that counts its reads observes.
    emitter.label("__rt_stream_get_contents_after_feof");
    // Each chunk is copied out immediately, so every read may reuse the same
    // shared-buffer position instead of walking the offset forward.
    emitter.instruction("ldr x12, [sp, #56]");                                  // the position this helper started from
    emit_symbol_address(emitter, "x13", "_concat_off");
    emitter.instruction("str x12, [x13]");                                      // make __rt_fread write there
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle for __rt_fread
    // -- read with the chunk size carried by the authoritative StreamState --
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the state-owned read-loop chunk size
    emitter.instruction("cbnz x1, __rt_stream_get_contents_chunk_loaded");      // preserve an explicitly configured chunk size
    emitter.label("__rt_stream_get_contents_chunk_default");
    emitter.instruction("mov x1, #4096");                                       // keep a defensive default for direct runtime callers
    emitter.label("__rt_stream_get_contents_chunk_loaded");
    emitter.instruction("bl __rt_fread");                                       // x1=chunk ptr, x2=chunk len
    emitter.instruction("cbz x2, __rt_stream_get_contents_release_done");       // empty read stops the read-all loop
    emitter.instruction("str x1, [sp, #32]");                                   // save chunk pointer across the copy
    emitter.instruction("str x2, [sp, #40]");                                   // save chunk length across the copy
    // -- grow the accumulator until this chunk fits --
    emitter.instruction("ldr x9, [sp, #24]");                                   // running total
    emitter.instruction("ldr x10, [sp, #40]");                                  // chunk length
    emitter.instruction("add x9, x9, x10");                                     // bytes needed after this chunk
    emitter.instruction("ldr x11, [sp, #8]");                                   // current capacity
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.le __rt_stream_get_contents_have_cap");              // it already fits
    emitter.label("__rt_stream_get_contents_grow");
    emitter.instruction("lsl x11, x11, #1");                                    // double the capacity
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.gt __rt_stream_get_contents_grow");                  // keep doubling until it fits
    emitter.instruction("str x11, [sp, #8]");                                   // publish the new capacity
    emitter.instruction("mov x0, x11");
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the larger accumulator
    emitter.instruction("cbz x0, __rt_stream_get_contents_done");               // heap exhausted: return what was read
    emitter.instruction("str x0, [sp, #64]");                                   // stash the new accumulator
    emitter.instruction("ldr x12, [sp, #16]");                                  // old accumulator
    emitter.instruction("ldr x13, [sp, #24]");                                  // bytes already accumulated
    emitter.instruction("mov x14, #0");                                         // copy cursor
    emitter.label("__rt_stream_get_contents_regrow_copy");
    emitter.instruction("cmp x14, x13");
    emitter.instruction("b.ge __rt_stream_get_contents_regrown");
    emitter.instruction("ldrb w15, [x12, x14]");
    emitter.instruction("strb w15, [x0, x14]");
    emitter.instruction("add x14, x14, #1");
    emitter.instruction("b __rt_stream_get_contents_regrow_copy");
    emitter.label("__rt_stream_get_contents_regrown");
    emitter.instruction("ldr x0, [sp, #16]");                                   // old accumulator
    emitter.instruction("bl __rt_heap_free");                                   // it never escaped this helper
    emitter.instruction("ldr x0, [sp, #64]");                                   // the grown accumulator
    emitter.instruction("str x0, [sp, #16]");                                   // becomes the live one
    emitter.label("__rt_stream_get_contents_have_cap");

    emitter.instruction("ldr x1, [sp, #32]");                                   // chunk pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // chunk length
    emitter.instruction("ldr x9, [sp, #24]");                                   // running total
    emitter.instruction("ldr x11, [sp, #16]");                                  // accumulator base
    emitter.instruction("add x11, x11, x9");                                    // destination = accumulator + total
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
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release owned wrapper/filter chunks; concat slices are ignored
    emitter.instruction("b __rt_stream_get_contents_loop");                     // read the next chunk

    // -- release the terminal empty chunk and return the accumulated string --
    emitter.label("__rt_stream_get_contents_release_done");
    emitter.instruction("mov x0, x1");                                          // final empty chunk pointer
    emitter.instruction("bl __rt_decref_any");                                  // release it if it is heap-backed
    emitter.label("__rt_stream_get_contents_done");
    // Restore the shared offset this helper borrowed, then hand back an owned
    // copy so the accumulator can be released here.
    emitter.instruction("ldr x12, [sp, #56]");
    emit_symbol_address(emitter, "x13", "_concat_off");
    emitter.instruction("str x12, [x13]");                                      // shared buffer is left as it was found
    emitter.instruction("ldr x1, [sp, #16]");                                   // accumulator pointer
    emitter.instruction("cbz x1, __rt_stream_get_contents_empty");              // allocation failed: return an empty slice
    emitter.instruction("ldr x2, [sp, #24]");                                   // accumulated length
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned copy of the result
    emitter.instruction("str x1, [sp, #64]");                                   // hold it across the free
    emitter.instruction("ldr x0, [sp, #16]");                                   // the accumulator itself
    emitter.instruction("bl __rt_heap_free");                                   // release the scratch accumulation buffer
    emitter.instruction("ldr x1, [sp, #64]");                                   // owned result pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // owned result length
    emitter.instruction("b __rt_stream_get_contents_ret");
    emitter.label("__rt_stream_get_contents_empty");
    emitter.instruction("mov x1, #0");
    emitter.instruction("mov x2, #0");
    emitter.label("__rt_stream_get_contents_ret");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the accumulated string

    emit_stream_get_contents_bounded_aarch64(emitter);
}

/// Emits the AArch64 bounded stream_get_contents helper.
///
/// Input: `x0 = opaque stream handle`, `x1 = max bytes`, `x2 = state-owned chunk size`.
/// Output: `x1 = ptr`, `x2 = len`.
/// The loop calls `__rt_fread` repeatedly, copies each returned chunk into a reserved
/// accumulation buffer that grows on demand (never beyond the requested cap), and stops
/// at the requested byte count or EOF.
///
/// Frame: [0]=handle [8]=byte cap [24]=accumulation pointer [32]=total [40]=chunk ptr
///        [48]=chunk len [56]=accumulation capacity [64]=resolved chunk size [80]=x29/x30.
fn emit_stream_get_contents_bounded_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents_bounded ---");
    emitter.label_global("__rt_stream_get_contents_bounded");

    emitter.instruction("sub sp, sp, #96");                                     // allocate locals plus chunk state and saved caller frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the opaque stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested byte cap

    // -- resolve the read-loop chunk size once, before it is needed twice per iteration --
    emitter.instruction("cbnz x2, __rt_stream_get_contents_bounded_chunk_loaded"); // preserve an explicitly configured chunk size
    emitter.label("__rt_stream_get_contents_bounded_chunk_default");
    emitter.instruction("mov x2, #4096");                                       // keep a defensive default for direct runtime callers
    emitter.label("__rt_stream_get_contents_bounded_chunk_loaded");
    emitter.instruction("str x2, [sp, #64]");                                   // save the state-owned read-loop chunk size

    // -- reserve min(cap, initial capacity) and claim the whole window --
    emitter.instruction(&format!("mov x9, #{INITIAL_CAPACITY}"));               // start from the initial accumulation capacity
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
    // The slot holds the opaque handle; only this range probe wants a descriptor.
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor for the range probe
    emitter.instruction("mov x14, x0");                                         // keep the descriptor for the comparison
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the handle for __rt_feof
    emitter.instruction("mov w11, #0x4000");                                    // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w11, w11, #16");                                   // form 0x40000000
    emitter.instruction("cmp x14, x11");                                        // synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_stream_get_contents_bounded_after_feof");    // normal fd: skip wrapper EOF dispatch
    super::feof::emit_feof_call(emitter, true);                                 // elephc's own probe: never warns, the read does
    emitter.instruction("cbnz x0, __rt_stream_get_contents_bounded_done");      // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_bounded_after_feof");

    // -- make room for one more (cap-clamped) chunk before asking fread for it --
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction("ldr x12, [sp, #64]");                                  // state-owned read-loop chunk size
    emitter.instruction("add x9, x9, x12");                                     // capacity needed once the next chunk lands
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

    // php asks its source for a WHOLE chunk even when the cap needs fewer bytes, and keeps the
    // surplus on the stream: `stream_get_contents($h, 30)` on a wrapper whose chunk size is 17
    // calls `stream_read(17)` TWICE, not 17 then 13. elephc trims the request instead, which that
    // hook can see. Matching php here means the surplus must survive as buffered stream state that
    // `ftell()`, a later read AND a SEEK all agree about — a seek has to drop it, or a
    // `stream_get_contents($h, $len, $offset)` prepends bytes from before the seek. Left trimmed
    // until that accounting exists; the divergence is the request SIZE, never the bytes delivered.
    emitter.instruction("ldr x9, [sp, #32]");                                   // running result length
    emitter.instruction("ldr x10, [sp, #8]");                                   // requested byte cap
    emitter.instruction("sub x1, x10, x9");                                     // remaining bytes needed
    emitter.instruction("ldr x11, [sp, #64]");                                  // state-owned read-loop chunk size
    emitter.instruction("cmp x1, x11");                                         // is the remaining cap smaller than the chunk size?
    emitter.instruction("csel x1, x1, x11, lt");                                // request min(remaining, chunk size)
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle for __rt_fread
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
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the opaque stream handle

    // -- resolve the read-loop chunk size once, before it is needed twice per iteration --
    emitter.instruction("test rsi, rsi");                                       // was a custom chunk size supplied?
    emitter.instruction("jnz __rt_stream_get_contents_chunk_loaded_x86");       // preserve the supplied chunk size
    emitter.label("__rt_stream_get_contents_chunk_default_x86");
    emitter.instruction("mov rsi, 4096");                                       // keep a defensive default for direct runtime callers
    emitter.label("__rt_stream_get_contents_chunk_loaded_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], rsi");                       // save the state-owned read-loop chunk size

    // -- reserve the accumulation buffer and claim its whole window --
    // The result used to be built inside `_concat_buf`, which caps at 64 KiB, so reading a
    // larger stream ran off the end into the adjacent BSS globals. Claiming the reservation
    // up front makes each chunk's own `__rt_fread` reservation land after the accumulated
    // bytes; handing the chunk window straight back keeps the scratch cursor from creeping.
    emitter.instruction(&format!("mov QWORD PTR [rbp - 56], {INITIAL_CAPACITY}")); // start from the initial accumulation capacity
    emitter.instruction(&format!("mov rax, {INITIAL_CAPACITY}"));               // request the initial capacity from the reservation front end
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the accumulated result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the accumulation buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // publish the full reserved capacity
    emitter.instruction("call __rt_concat_publish");                            // claim the whole window so each chunk read appends after it
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the running byte total to zero

    emitter.label("__rt_stream_get_contents_loop_x86");
    // The slot holds the opaque handle: __rt_fread and __rt_feof both need it, and
    // passing a descriptor here silently defeated the filter chain because
    // __rt_stream_state could not resolve it. Only this range probe wants the
    // descriptor, so resolve it here rather than storing one.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor for the range probe
    emitter.instruction("mov r14, rax");                                        // keep the descriptor for the comparison
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the handle for __rt_feof
    emitter.instruction("mov r10d, 0x40000000");                                // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp r14, r10");                                        // synthetic user-wrapper fd?
    emitter.instruction("jl __rt_stream_get_contents_after_feof_x86");          // normal fd: skip wrapper EOF dispatch
    // See the AArch64 counterpart: php does NOT gate the read on `stream_eof()` here, it keeps
    // calling `stream_read()` until one answers an EMPTY string.
    emitter.label("__rt_stream_get_contents_after_feof_x86");

    // -- make room for one more chunk before asking fread for it --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // running result length
    emitter.instruction("add r8, QWORD PTR [rbp - 64]");                        // capacity needed once the next chunk lands
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

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle for __rt_fread
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // read with the chunk size carried by the authoritative StreamState
    emitter.instruction("call __rt_fread");                                     // rax=chunk ptr, rdx=chunk len
    emitter.instruction("test rdx, rdx");                                       // empty chunk?
    emitter.instruction("jz __rt_stream_get_contents_release_done_x86");        // empty read stops the read-all loop
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save chunk pointer across the copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save chunk length across the copy
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // accumulation buffer base pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 32]");                       // destination = accumulation base + total
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
    emitter.instruction("ret");                                                 // return the accumulated string slice

    emit_stream_get_contents_bounded_linux_x86_64(emitter);
}

/// Emits the x86_64 bounded stream_get_contents helper.
///
/// Input: `rdi = opaque stream handle`, `rsi = max bytes`, `rdx = state-owned chunk size`.
/// Output: `rax = ptr`, `rdx = len`.
/// The helper copies each `__rt_fread` chunk into a reserved accumulation buffer that grows
/// on demand but never past the requested cap, so filters or wrappers that return separate
/// buffers still produce one contiguous result.
///
/// Frame: [rbp-8]=handle [rbp-16]=byte cap [rbp-32]=accumulation pointer [rbp-40]=total
///        [rbp-48]=chunk ptr [rbp-56]=chunk len [rbp-64]=accumulation capacity
///        [rbp-72]=resolved chunk size.
fn emit_stream_get_contents_bounded_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents_bounded ---");
    emitter.label_global("__rt_stream_get_contents_bounded");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve aligned locals for bounded accumulation
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested byte cap

    // -- resolve the read-loop chunk size once, before it is needed twice per iteration --
    emitter.instruction("test rdx, rdx");                                       // was a custom chunk size supplied?
    emitter.instruction("jnz __rt_stream_get_contents_bounded_chunk_loaded_x86"); // preserve the supplied chunk size
    emitter.label("__rt_stream_get_contents_bounded_chunk_default_x86");
    emitter.instruction("mov rdx, 4096");                                       // keep a defensive default for direct runtime callers
    emitter.label("__rt_stream_get_contents_bounded_chunk_loaded_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the state-owned read-loop chunk size

    // -- reserve min(cap, initial capacity) and claim the whole window --
    emitter.instruction(&format!("mov rax, {INITIAL_CAPACITY}"));               // start from the initial accumulation capacity
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
    // The slot holds the opaque handle; only this range probe wants a descriptor.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor for the range probe
    emitter.instruction("mov r14, rax");                                        // keep the descriptor for the comparison
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the handle for __rt_feof
    emitter.instruction("mov r10d, 0x40000000");                                // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp r14, r10");                                        // synthetic user-wrapper fd?
    emitter.instruction("jl __rt_stream_get_contents_bounded_after_feof_x86");  // normal fd: skip wrapper EOF dispatch
    super::feof::emit_feof_call(emitter, true);                                 // elephc's own probe: never warns, the read does
    emitter.instruction("test rax, rax");                                       // did stream_eof report true?
    emitter.instruction("jnz __rt_stream_get_contents_bounded_done_x86");       // wrapper EOF means no extra stream_read call
    emitter.label("__rt_stream_get_contents_bounded_after_feof_x86");

    // -- make room for one more (cap-clamped) chunk before asking fread for it --
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // running result length
    emitter.instruction("add r8, QWORD PTR [rbp - 72]");                        // capacity needed once the next chunk lands
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
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // state-owned read-loop chunk size
    emitter.instruction("cmp rsi, r10");                                        // is the remaining cap bigger than one chunk?
    emitter.instruction("cmovg rsi, r10");                                      // request min(remaining, chunk size)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle for __rt_fread
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
    emitter.instruction("ret");                                                 // return the bounded string slice
}
