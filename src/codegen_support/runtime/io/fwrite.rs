//! Purpose:
//! Emits the `__rt_fwrite` runtime helper, which writes a buffer to a
//! descriptor after applying any attached write-direction stream filter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `fwrite` builtin emitter.
//!
//! Key details:
//! - When a write filter is attached the payload is copied into the dedicated
//!   `_stream_filter_buf` scratch, transformed in place, and written from there
//!   so the caller's string is never mutated.
//! - A payload larger than the 64 KiB scratch is written unfiltered; v1 stream
//!   filters target the common small-write case.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_CHUNK_SIZE_OFFSET,
    STREAM_APPEND_SKIP_OFFSET, STREAM_BACKEND_KIND_OFFSET, STREAM_BACKEND_USER_WRAPPER,
    STREAM_MODE_LEN_OFFSET, STREAM_MODE_PTR_OFFSET, STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET,
    STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, platform::Platform};

const FILTER_BUF_SIZE: i64 = 65536;

/// Wrapper id 6 is `php://`, whose sub-wrappers each answer writes their own way.
const WRAPPER_ID_PHP: u64 = 6;

/// Classification stored in the frame: an ordinary stream, whose recorded mode gates the write.
const PHP_WRITE_ORDINARY: i64 = 0;

/// `php://output`: php sends these bytes down the OUTPUT-BUFFER stack, not to a descriptor.
///
/// php-src gives the target its own `php_stream_output_ops`, whose write is `php_output_write` —
/// the same sink `echo` uses — so `ob_start()` captures it and `ob_get_clean()` hands it back.
/// Measured on `php -n` 8.5.6:
///   `ob_start(); $h=fopen("php://output","w"); fwrite($h,"X"); var_dump(ob_get_clean());`
///   answers `string(1) "X"`, and the same script with `php://stdout` answers `string(0) ""`
///   after printing `X` — the two targets are NOT aliases, which is exactly what elephc's
///   `dup(1)` made them.
const PHP_WRITE_OUTPUT_BUFFER: i64 = 1;

/// A descriptor-backed `php://` target: `stdin`, `stdout`, `stderr`, `fd/N`.
///
/// php-src's `_php_stream_write` refuses a write only when the stream's ops have NO write
/// function; it never reads the mode string back. So `fopen("php://stdout","r")` writes happily,
/// and so does `php://fd/1` opened `"rb"` — measured on `php -n` 8.5.6, both answer `2` for a
/// two-byte `fwrite`. Only the descriptor itself can refuse. The in-memory targets are a
/// different story and keep the gate: php builds them read-only when the mode names none of
/// `w`, `a`, `+`, and `__rt_stream_record_mode` already normalises their recorded mode to match.
const PHP_WRITE_DESCRIPTOR: i64 = 2;

/// fwrite: write a payload to a descriptor, applying a write filter if present.
/// Input:  AArch64 x0 = fd, x1 = pointer, x2 = length
///         x86_64  rdi = fd, rsi = pointer, rdx = length
/// Output: the number of bytes written.
pub fn emit_fwrite(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fwrite_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fwrite ---");
    emitter.label_global("__rt_fwrite");

    // Frame (80 bytes): [0]=fd [8]=pointer [16]=length [24]=handle [32]=session
    //                   [40]=append cursor [48]=byte count [64]=x29 [72]=x30.
    //
    // [40] holds -1 for every stream but an append one. For an append stream it holds where the
    // logical cursor was before the write, because `O_APPEND` is about to move the descriptor to
    // the end and `ftell()` must not report that. [48] holds the byte count across the two calls
    // the accounting makes afterwards.
    //
    // The frame is established before the synthetic-descriptor range checks
    // because x0 arrives as an opaque stream handle, not as a descriptor. The
    // TLS session has to be read while the handle is still in hand: it lives on
    // the StreamState now, and the descriptor cannot reach it.
    //
    // Raw descriptors still work unchanged. `__rt_stream_fd` passes a
    // non-handle through untouched and `__rt_stream_tls_session` reports no
    // session for it, which is exactly the plain-write path the internal
    // callers want.
    emitter.instruction("sub sp, sp, #80");                                     // frame for the saved write state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #24]");                                   // save the incoming handle or raw descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the payload pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the payload length
    emitter.instruction("mov x9, #-1");                                         // no append accounting unless the mode says otherwise
    emitter.instruction("str x9, [sp, #40]");
    emitter.instruction(&format!("mov x9, #{PHP_WRITE_ORDINARY}"));             // until the URI says otherwise, the mode gates this write
    emitter.instruction("str x9, [sp, #56]");

    // -- a read-only stream refuses the write, before anything is attempted --
    emitter.instruction("bl __rt_stream_state");                                // x0 = the owning state, zero for a raw descriptor
    emitter.instruction("cbz x0, __rt_fwrite_mode_ok");                         // no state: keep the descriptor-only behaviour
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_BACKEND_KIND_OFFSET}]")); // which backend owns this stream
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_USER_WRAPPER}"));
    emitter.instruction("b.eq __rt_fwrite_mode_ok");                            // a user wrapper's stream_write() decides for itself
    emit_php_sub_wrapper_classify_aarch64(emitter);
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_MODE_PTR_OFFSET}]"));   // the recorded mode string
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_MODE_LEN_OFFSET}]"));  // and its length
    emitter.instruction("cbz x9, __rt_fwrite_mode_ok");                         // no mode recorded: nothing to refuse on
    emitter.instruction("cbz x10, __rt_fwrite_mode_ok");
    emitter.instruction("ldrb w11, [x9]");                                      // the access letter
    emitter.instruction("cmp w11, #97");                                        // 'a' — the append opener
    emitter.instruction("b.eq __rt_fwrite_mode_append");                        // its position is PHP's, not the descriptor's
    emitter.instruction("cmp w11, #114");                                       // 'r' — the only read-only opener
    emitter.instruction("b.ne __rt_fwrite_mode_ok");                            // 'w'/'x'/'c' all write
    emitter.instruction("mov x12, #0");
    emitter.label("__rt_fwrite_mode_scan");
    emitter.instruction("cmp x12, x10");
    emitter.instruction("b.hs __rt_fwrite_mode_refuse");                        // scanned it all with no '+': read-only
    emitter.instruction("ldrb w13, [x9, x12]");
    emitter.instruction("cmp w13, #43");                                        // '+' anywhere grants write access
    emitter.instruction("b.eq __rt_fwrite_mode_ok");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_fwrite_mode_scan");
    emitter.label("__rt_fwrite_mode_refuse");
    emitter.instruction("mov x0, #-1");                                         // negative: the caller boxes PHP false
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");
    emitter.label("__rt_fwrite_mode_append");
    emitter.instruction("str xzr, [sp, #40]");                                  // the slot now carries a position, not the -1 sentinel
    emitter.instruction("b __rt_fwrite_mode_ok");
    emitter.label("__rt_fwrite_mode_ok");
    emitter.instruction("ldr x0, [sp, #24]");                                   // the session lookup below still needs the handle

    emitter.instruction("bl __rt_stream_tls_session");                          // resolve the session while the handle is available
    emitter.instruction("str x0, [sp, #32]");                                   // save the attached TLS session, zero when plain
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the handle for descriptor resolution
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor through StreamState
    emitter.instruction("str x0, [sp, #0]");                                    // save the resolved file descriptor

    // -- phar:// write stream synthetic fd range (0x50000000..0x50000020) --
    emitter.instruction("mov w10, #0x5000");                                    // low half of the phar-write descriptor base
    emitter.instruction("lsl w10, w10, #16");                                   // form the full 0x50000000 phar-write descriptor base
    emitter.instruction("cmp x0, x10");                                         // is the descriptor below the phar-write range?
    emitter.instruction("b.lt __rt_fwrite_not_phar");                           // below the range: use normal stream dispatch
    emitter.instruction("add x11, x10, #32");                                   // upper bound for the 32 buffered PHAR write descriptors
    emitter.instruction("cmp x0, x11");                                         // is this inside the phar-write descriptor range?
    emitter.instruction("b.ge __rt_fwrite_not_phar");                           // above the phar-write range: use normal stream dispatch
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the payload pointer for the tail call
    emitter.instruction("ldr x2, [sp, #16]");                                   // restore the payload length for the tail call
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame before the tail call
    emitter.instruction("b __rt_phar_write_append");                            // in range: append to the phar buffer (uncond → cross-atom safe)
    emitter.label("__rt_fwrite_not_phar");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_fwrite_real_fd");                            // not a wrapper fd → issue the real write syscall path

    // -- a wrapper's `stream_write()` sees CHUNKS, not the whole payload --
    //
    // php hands a userspace wrapper at most `chunk_size` bytes per call, so writing 70 bytes to a
    // stream whose chunk size is 42 calls `stream_write()` twice, with 42 then 28. elephc made one
    // call with all 70, which a wrapper that counts or frames its writes observes directly.
    // Measured on `php -n` 8.5.6.
    //
    // The loop lives here rather than in `__rt_user_wrapper_fwrite` because the chunk size hangs
    // off the StreamState and only the HANDLE reaches it — the wrapper helper receives the
    // synthetic descriptor, which cannot. A short write ends the loop: php stops handing over
    // chunks as soon as the wrapper takes fewer bytes than it was given.
    emitter.instruction("str x0, [sp, #0]");                                    // the synthetic descriptor every chunk goes to
    emitter.instruction("ldr x0, [sp, #24]");                                   // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // the size lives on the state
    emitter.instruction("cbz x0, __rt_uw_chunk_default");
    emitter.instruction(&format!("ldr x0, [x0, #{STREAM_CHUNK_SIZE_OFFSET}]"));
    emitter.instruction("cbnz x0, __rt_uw_chunk_sized");                        // an explicitly configured size
    emitter.label("__rt_uw_chunk_default");
    // php's default here is 8192, which is also what `stream_set_chunk_size()` reports as the
    // previous value — NOT the 4096 `__rt_stream_chunk_size` answers, which is a read-loop
    // fallback. Measured on `php -n` 8.5.6: 9000 bytes to an unconfigured wrapper arrive as 8192
    // then 808.
    emitter.instruction("mov x0, #8192");
    emitter.label("__rt_uw_chunk_sized");
    emitter.instruction("str x0, [sp, #32]");                                   // the TLS slot is unused on this path
    emitter.instruction("str xzr, [sp, #48]");                                  // bytes the wrapper has taken so far
    emitter.label("__rt_uw_chunk_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // what it has taken
    emitter.instruction("ldr x10, [sp, #16]");                                  // the whole payload length
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ge __rt_uw_chunk_done");                             // every byte has been handed over
    emitter.instruction("sub x11, x10, x9");                                    // what is left
    emitter.instruction("ldr x12, [sp, #32]");                                  // the chunk size
    emitter.instruction("cmp x11, x12");
    emitter.instruction("csel x11, x12, x11, gt");                              // hand over MIN(remaining, chunk)
    emitter.instruction("ldr x1, [sp, #8]");                                    // the payload base
    emitter.instruction("add x1, x1, x9");                                      // this chunk starts after what was taken
    emitter.instruction("mov x2, x11");                                         // and is this long
    emitter.instruction("str x11, [sp, #40]");                                  // remember what was offered, to spot a short write
    emitter.instruction("ldr x0, [sp, #0]");                                    // the synthetic descriptor
    emitter.instruction("bl __rt_user_wrapper_fwrite");                         // stream_write($chunk)
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_uw_chunk_failed");                           // a missing hook reports failure, not a byte count
    emitter.instruction("cbz x0, __rt_uw_chunk_done");                          // it took nothing: stop rather than spin
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("add x9, x9, x0");                                      // count what it took
    emitter.instruction("str x9, [sp, #48]");
    // A SHORT write is not the end. php re-offers from the new position, so a wrapper that
    // accepts four bytes of every ten still receives the whole payload: measured on `php -n`
    // 8.5.6, 30 bytes at chunk 10 with such a wrapper arrive as 10,10,10,10,10,10,6,2 and
    // `fwrite()` answers 30.
    emitter.instruction("b __rt_uw_chunk_loop");
    emitter.label("__rt_uw_chunk_failed");
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("cbnz x9, __rt_uw_chunk_done");                         // some chunks landed: report those bytes
    emitter.instruction("mov x9, #-1");                                         // nothing landed: the caller sees php false
    emitter.instruction("str x9, [sp, #48]");
    emitter.label("__rt_uw_chunk_done");
    emitter.instruction("ldr x0, [sp, #48]");                                   // the total the wrapper accepted
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");
    emitter.label("__rt_fwrite_real_fd");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the payload pointer clobbered by resolution
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the payload length clobbered by resolution

    // -- look up the write filter for this descriptor --
    abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
    emitter.instruction("ldrb w3, [x9, x0]");                                   // write filter id for this descriptor
    emitter.instruction("cbz w3, __rt_fwrite_direct");                          // no filter: write the payload directly
    emitter.instruction("cmp w3, #128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("b.ge __rt_fwrite_user_filter");                        // dispatch through the user filter method, then write the result
    emitter.instruction("cmp w3, #4");                                          // is the zlib.deflate write filter attached?
    emitter.instruction("b.eq __rt_fwrite_zlib");                               // hand the payload to the zlib deflate helper
    emitter.instruction("cmp w3, #10");                                         // is the bzip2.compress write filter attached?
    emitter.instruction("b.eq __rt_fwrite_bz2");                                // hand the payload to the bzip2 compress helper
    emitter.instruction("cmp w3, #12");                                         // is the convert.iconv write filter attached?
    emitter.instruction("b.eq __rt_fwrite_iconv");                              // hand the payload to the iconv write helper
    emitter.instruction(&format!("mov x9, #{}", FILTER_BUF_SIZE));              // filter scratch capacity
    emitter.instruction("cmp x2, x9");                                          // is the payload larger than the scratch?
    emitter.instruction("b.gt __rt_fwrite_direct");                             // oversized payloads are written unfiltered

    // -- copy the payload into the filter scratch --
    abi::emit_symbol_address(emitter, "x4", "_stream_filter_buf");
    emitter.instruction("mov x5, #0");                                          // copy index
    emitter.label("__rt_fwrite_copy");
    emitter.instruction("cmp x5, x2");                                          // copied every byte?
    emitter.instruction("b.ge __rt_fwrite_copy_done");                          // the payload is fully copied
    emitter.instruction("ldrb w6, [x1, x5]");                                   // load a payload byte
    emitter.instruction("strb w6, [x4, x5]");                                   // store it into the filter scratch
    emitter.instruction("add x5, x5, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fwrite_copy");                                  // continue copying
    emitter.label("__rt_fwrite_copy_done");

    // -- transform the scratch copy and write from it --
    emitter.instruction("mov x1, x4");                                          // filter target = the scratch buffer
    emitter.instruction("str x1, [sp, #8]");                                    // the write reads the filtered scratch
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform the scratch copy in place; x2 = (possibly compacted) length
    emitter.instruction("str x2, [sp, #16]");                                   // commit the post-filter length so strip_tags / similar shrinking filters write the compacted bytes only
    emitter.instruction("b __rt_fwrite_direct");                                // continue with the standard direct write

    // -- zlib.deflate filter: deflate-compress the payload into the stream --
    emitter.label("__rt_fwrite_zlib");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd argument for the zlib deflate helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer argument
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length argument
    abi::emit_symbol_address(emitter, "x9", "_zlib_fwrite_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the deflate fwrite helper pointer
    emitter.instruction("blr x9");                                              // deflate-compress the payload, x0 = bytes consumed
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- bzip2.compress filter: bzip2-compress the payload into the stream --
    emitter.label("__rt_fwrite_bz2");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd argument for the bzip2 compress helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer argument
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length argument
    abi::emit_symbol_address(emitter, "x9", "_bz2_fwrite_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bzip2 compress fwrite helper pointer
    emitter.instruction("blr x9");                                              // bzip2-compress the payload, x0 = bytes consumed
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- convert.iconv write filter: transcode the payload into the stream --
    emitter.label("__rt_fwrite_iconv");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd argument for the iconv write helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer argument
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length argument
    abi::emit_symbol_address(emitter, "x9", "_iconv_fwrite_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the iconv write helper pointer
    emitter.instruction("blr x9");                                              // transcode the payload, x0 = bytes consumed
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- user filter: dispatch through filter(string), then write the result --
    emitter.label("__rt_fwrite_user_filter");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd into the user-filter dispatcher's first arg
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload ptr → second arg
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload len → third arg
    emitter.instruction("mov x3, #1");                                          // direction = 1 (write)
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← user filter's transformed payload
    emitter.instruction("str x1, [sp, #8]");                                    // overwrite the payload-ptr slot with the filter result
    emitter.instruction("str x2, [sp, #16]");                                   // overwrite the payload-len slot with the filter result
    emitter.instruction("b __rt_fwrite_direct");                                // fall through to the standard direct-write path

    emitter.label("__rt_fwrite_direct");
    emitter.instruction("ldr x0, [sp, #0]");                                    // file descriptor
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer (original or filtered)
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length
    // -- TLS dispatch: route through elephc_tls_write when fd has an
    //    attached session (Phase 11 B3). --
    emitter.instruction("ldr x14, [sp, #32]");                                  // the session resolved from the handle at entry
    emitter.instruction("cbz x14, __rt_fwrite_syscall");                        // no TLS attached → write syscall
    emitter.instruction("mov x0, x14");                                         // handle as first arg
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_write_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load runtime value
    emitter.instruction("blr x9");                                              // x0 = bytes written or -1
    emitter.instruction("b __rt_fwrite_return");                                // continue at target label
    emitter.label("__rt_fwrite_syscall");
    // -- an append stream records where its logical cursor was, before O_APPEND moves the
    //    descriptor to the end of the file --
    emitter.instruction("ldr x9, [sp, #40]");                                   // -1 for every other stream
    emitter.instruction("cmn x9, #1");
    emitter.instruction("b.eq __rt_fwrite_write_now");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the descriptor
    emitter.instruction("mov x1, #0");
    emitter.instruction("mov x2, #1");                                          // SEEK_CUR
    emitter.syscall(199);
    emitter.instruction("str x0, [sp, #40]");                                   // where PHP's position stood
    emitter.label("__rt_fwrite_write_now");
    // -- php://output writes to the OUTPUT-BUFFER stack, not to a descriptor --
    // The bytes take the same road `echo` takes, so an enclosing `ob_start()` captures them and
    // a user output handler sees them. Everything upstream still ran: an attached write filter
    // has already transformed the payload in place, and this reads the filtered slots.
    emitter.instruction("ldr x9, [sp, #56]");                                   // the php:// classification made at entry
    emitter.instruction(&format!("cmp x9, #{PHP_WRITE_OUTPUT_BUFFER}"));
    emitter.instruction("b.ne __rt_fwrite_write_syscall");                      // every other stream writes to its descriptor
    emitter.instruction("ldr x0, [sp, #8]");                                    // payload pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // payload length
    emitter.instruction("bl __rt_stdout_write");                                // print_r capture, ob stack, --web capture, then fd 1
    emitter.instruction("ldr x0, [sp, #16]");                                   // php answers the whole byte count for this sink
    emitter.instruction("b __rt_fwrite_wrote");
    emitter.label("__rt_fwrite_write_syscall");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the write arguments: the probe clobbered them
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("ldr x2, [sp, #16]");
    emitter.syscall(4);
    // macOS reports a failed write by setting the carry flag and leaving the POSITIVE
    // errno in x0, which is indistinguishable from a byte count to every caller: writing
    // to a read-only stream answered 9 (EBADF) instead of false. Linux already returns a
    // negative, so normalise macOS onto the same shape.
    if emitter.platform == Platform::MacOS {
        emitter.instruction("b.cc __rt_fwrite_wrote");                          // carry clear: x0 really is a byte count
        emitter.instruction("mov x0, #-1");                                     // a failed write reports failure, not its errno
        emitter.instruction("b __rt_fwrite_return");                            // a failed write moved nothing to account for
    } else {
        emitter.instruction("cmp x0, #0");
        emitter.instruction("b.lt __rt_fwrite_return");                         // a failed write moved nothing to account for
    }
    emitter.label("__rt_fwrite_wrote");
    emit_append_skip_update_aarch64(emitter);
    emitter.label("__rt_fwrite_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the byte count from write
}

/// Classifies the `php://` sub-wrapper backing this stream, parking the answer at `[sp, #56]`.
///
/// Takes the resolved StreamState in `x0` and LEAVES IT THERE: the read-only gate that follows
/// still needs it. Clobbers `x9`-`x12` only, which the gate reloads anyway.
///
/// The sub-wrapper is told apart by the byte after `php://`, exactly as `__rt_stream_type_name`
/// does. `fd` is matched in full rather than by its initial, because `filter` shares it and a
/// `php://filter/...` URL must keep whatever rule its INNER resource has.
fn emit_php_sub_wrapper_classify_aarch64(emitter: &mut Emitter) {
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_PHP}"));
    emitter.instruction("b.ne __rt_fwrite_php_classified");                     // every other wrapper keeps the mode gate
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]"));   // the recorded URI
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]"));   // and its length
    emitter.instruction("cbz x10, __rt_fwrite_php_classified");                 // no URI: nothing to classify on
    emitter.instruction("cmp x11, #7");                                         // "php://" plus the byte that names the sub-wrapper
    emitter.instruction("b.lt __rt_fwrite_php_classified");
    emitter.instruction("ldrb w12, [x10, #6]");                                 // the first byte of the php:// sub-wrapper name
    emitter.instruction("cmp w12, #0x6F");                                      // 'o' as in output
    emitter.instruction("b.eq __rt_fwrite_php_output_sink");
    emitter.instruction("cmp w12, #0x73");                                      // 's' as in stdin, stdout, stderr
    emitter.instruction("b.eq __rt_fwrite_php_descriptor_sink");
    emitter.instruction("cmp w12, #0x66");                                      // 'f' — either "fd/" or "filter"
    emitter.instruction("b.ne __rt_fwrite_php_classified");
    emitter.instruction("cmp x11, #9");                                         // "php://fd/" is the shortest spelling
    emitter.instruction("b.lt __rt_fwrite_php_classified");
    emitter.instruction("ldrb w12, [x10, #7]");
    emitter.instruction("cmp w12, #0x64");                                      // 'd' — "filter" has 'i' here
    emitter.instruction("b.ne __rt_fwrite_php_classified");
    emitter.instruction("ldrb w12, [x10, #8]");
    emitter.instruction("cmp w12, #0x2F");                                      // '/' closes "php://fd/"
    emitter.instruction("b.ne __rt_fwrite_php_classified");
    emitter.label("__rt_fwrite_php_descriptor_sink");
    emitter.instruction(&format!("mov x9, #{PHP_WRITE_DESCRIPTOR}"));           // the descriptor decides, never the mode string
    emitter.instruction("str x9, [sp, #56]");
    emitter.instruction("b __rt_fwrite_mode_ok");                               // skip the read-only gate entirely
    emitter.label("__rt_fwrite_php_output_sink");
    emitter.instruction(&format!("mov x9, #{PHP_WRITE_OUTPUT_BUFFER}"));        // the write travels the output-buffer stack
    emitter.instruction("str x9, [sp, #56]");
    emitter.instruction("b __rt_fwrite_mode_ok");                               // php://output is always writable
    emitter.label("__rt_fwrite_php_classified");
}

/// Adds what `O_APPEND` jumped over to the stream's running total, so `ftell()` can subtract it.
///
/// PHP's position for an append stream advances by the bytes written, wherever they land. The
/// descriptor's does not: it is at the end of the file afterwards. The difference between the two
/// is `(position after the write) - (bytes written) - (position before it)`, which is the number
/// of bytes that were already past the cursor when the write happened.
///
/// Reads deliberately have no counterpart: a read moves the descriptor and PHP's position by the
/// same amount, so it leaves this total alone and `ftell()` stays right through an `a+` read.
fn emit_append_skip_update_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x9, [sp, #40]");                                   // -1 unless this is an append stream
    emitter.instruction("cmn x9, #1");
    emitter.instruction("b.eq __rt_fwrite_return");
    emitter.instruction("str x0, [sp, #48]");                                   // hold the byte count across the calls below
    emitter.instruction("ldr x0, [sp, #0]");                                    // the descriptor
    emitter.instruction("mov x1, #0");
    emitter.instruction("mov x2, #1");                                          // SEEK_CUR
    emitter.syscall(199);
    emitter.instruction("ldr x9, [sp, #48]");                                   // the bytes just written
    emitter.instruction("sub x0, x0, x9");                                      // where the file ended before them
    emitter.instruction("ldr x9, [sp, #40]");                                   // where PHP's position stood
    emitter.instruction("sub x0, x0, x9");                                      // the bytes O_APPEND jumped over
    emitter.instruction("str x0, [sp, #40]");                                   // hold the delta across the state lookup
    emitter.instruction("ldr x0, [sp, #24]");                                   // the handle the caller passed
    emitter.instruction("bl __rt_stream_state");                                // x0 = the owning state, zero for a raw descriptor
    emitter.instruction("cbz x0, __rt_fwrite_skip_done");
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_APPEND_SKIP_OFFSET}]"));
    emitter.instruction("ldr x10, [sp, #40]");
    emitter.instruction("add x9, x9, x10");                                     // accumulate: several writes each jump their own gap
    emitter.instruction(&format!("str x9, [x0, #{STREAM_APPEND_SKIP_OFFSET}]"));
    emitter.label("__rt_fwrite_skip_done");
    emitter.instruction("ldr x0, [sp, #48]");                                   // the byte count the caller is owed
}

/// Emits the Linux x86_64 stream runtime helper for fwrite.
fn emit_fwrite_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fwrite ---");
    emitter.label_global("__rt_fwrite");

    // Frame (rbp-relative): [-8]=fd [-16]=pointer [-24]=length [-32]=handle
    //                        [-40]=session [-48]=append cursor [-56]=byte count
    //                        [-72]=chunk size [-80]=bytes the wrapper took [-88]=bytes offered.
    //
    // See the AArch64 counterpart on the last two: [-48] holds -1 for every stream but an append
    // one, and where its logical cursor stood otherwise.
    //
    // The frame is established before the synthetic-descriptor range checks
    // because rdi arrives as an opaque stream handle, not as a descriptor. The
    // TLS session has to be read while the handle is still in hand: it lives on
    // the StreamState now, and the descriptor cannot reach it.
    //
    // Raw descriptors still work unchanged. `__rt_stream_fd` passes a
    // non-handle through untouched and `__rt_stream_tls_session` reports no
    // session for it, which is exactly the plain-write path the internal
    // callers want.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 96");                                         // frame for the saved write state and the wrapper chunking
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the incoming handle or raw descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the payload length
    emitter.instruction("mov QWORD PTR [rbp - 48], -1");                        // no append accounting unless the mode says otherwise
    emitter.instruction(&format!("mov QWORD PTR [rbp - 64], {PHP_WRITE_ORDINARY}")); // until the URI says otherwise, the mode gates this write

    // -- a read-only stream refuses the write, before anything is attempted --
    emitter.instruction("call __rt_stream_state");                              // rax = the owning state, zero for a raw descriptor
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fwrite_mode_ok_x86");                          // no state: keep the descriptor-only behaviour
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_BACKEND_KIND_OFFSET}]")); // which backend owns this stream
    emitter.instruction(&format!("cmp r9, {STREAM_BACKEND_USER_WRAPPER}"));
    emitter.instruction("je __rt_fwrite_mode_ok_x86");                          // a user wrapper's stream_write() decides for itself
    emit_php_sub_wrapper_classify_x86_64(emitter);
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_MODE_PTR_OFFSET}]")); // the recorded mode string
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {STREAM_MODE_LEN_OFFSET}]")); // and its length
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_fwrite_mode_ok_x86");                          // no mode recorded: nothing to refuse on
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_fwrite_mode_ok_x86");
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // the access letter
    emitter.instruction("cmp r11d, 97");                                        // 'a' — the append opener
    emitter.instruction("je __rt_fwrite_mode_append_x86");                      // its position is PHP's, not the descriptor's
    emitter.instruction("cmp r11d, 114");                                       // 'r' — the only read-only opener
    emitter.instruction("jne __rt_fwrite_mode_ok_x86");                         // 'w'/'x'/'c' all write
    emitter.instruction("xor r11, r11");
    emitter.label("__rt_fwrite_mode_scan_x86");
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jae __rt_fwrite_mode_refuse_x86");                     // scanned it all with no '+': read-only
    emitter.instruction("movzx eax, BYTE PTR [r9 + r11]");
    emitter.instruction("cmp eax, 43");                                         // '+' anywhere grants write access
    emitter.instruction("je __rt_fwrite_mode_ok_x86");
    emitter.instruction("add r11, 1");
    emitter.instruction("jmp __rt_fwrite_mode_scan_x86");
    emitter.label("__rt_fwrite_mode_refuse_x86");
    emitter.instruction("mov rax, -1");                                         // negative: the caller boxes PHP false
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
    emitter.label("__rt_fwrite_mode_append_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // the slot now carries a position, not the -1 sentinel
    emitter.instruction("jmp __rt_fwrite_mode_ok_x86");
    emitter.label("__rt_fwrite_mode_ok_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the session lookup below still needs the handle

    emitter.instruction("call __rt_stream_tls_session");                        // resolve the session while the handle is available
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the attached TLS session, zero when plain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the handle for descriptor resolution
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor through StreamState
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the resolved file descriptor
    emitter.instruction("mov rdi, rax");                                        // the range checks below classify the descriptor

    // -- phar:// write stream synthetic fd range (0x50000000..0x50000020) --
    emitter.instruction("mov r10d, 0x50000000");                                // the phar-write synthetic descriptor base
    emitter.instruction("cmp rdi, r10");                                        // is the descriptor below the phar-write range?
    emitter.instruction("jl __rt_fwrite_not_phar_x86");                         // below the range: use normal stream dispatch
    emitter.instruction("lea r11, [r10 + 32]");                                 // upper bound for the 32 buffered PHAR write descriptors
    emitter.instruction("cmp rdi, r11");                                        // is this inside the phar-write descriptor range?
    emitter.instruction("jge __rt_fwrite_not_phar_x86");                        // above the phar-write range: use normal stream dispatch
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the payload pointer for the tail call
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the payload length for the tail call
    emitter.instruction("mov rsp, rbp");                                        // discard the helper frame before the tail call
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_phar_write_append");                          // append the payload to the selected phar buffer
    emitter.label("__rt_fwrite_not_phar_x86");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jl __rt_fwrite_real_fd_x86");                          // not a wrapper fd → issue the real write syscall path

    // -- a wrapper's `stream_write()` sees CHUNKS, not the whole payload --
    // See the AArch64 counterpart for the rule and its measurements.
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the synthetic descriptor every chunk goes to
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // the size lives on the state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_uw_chunk_default_x86");
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {STREAM_CHUNK_SIZE_OFFSET}]"
    ));
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_uw_chunk_sized_x86");                         // an explicitly configured size
    emitter.label("__rt_uw_chunk_default_x86");
    emitter.instruction("mov rax, 8192");                                       // php's wrapper default, not the read-loop's 4096
    emitter.label("__rt_uw_chunk_sized_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // the chunk size
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // bytes the wrapper has taken so far
    emitter.label("__rt_uw_chunk_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // what it has taken
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // the whole payload length
    emitter.instruction("cmp r9, r10");
    emitter.instruction("jge __rt_uw_chunk_done_x86");                          // every byte has been handed over
    emitter.instruction("mov r11, r10");
    emitter.instruction("sub r11, r9");                                         // what is left
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // the chunk size
    emitter.instruction("cmp r11, rax");
    emitter.instruction("cmovg r11, rax");                                      // hand over MIN(remaining, chunk)
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // remember what was offered
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the payload base
    emitter.instruction("add rsi, r9");                                         // this chunk starts after what was taken
    emitter.instruction("mov rdx, r11");                                        // and is this long
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the synthetic descriptor
    emitter.instruction("call __rt_user_wrapper_fwrite");                       // stream_write($chunk)
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_uw_chunk_failed_x86");                         // a missing hook reports failure, not a byte count
    emitter.instruction("jz __rt_uw_chunk_done_x86");                           // it took nothing: stop rather than spin
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");
    emitter.instruction("add r9, rax");                                         // count what it took
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");
    emitter.instruction("jmp __rt_uw_chunk_loop_x86");                          // a short write is not the end: php re-offers
    emitter.label("__rt_uw_chunk_failed_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jnz __rt_uw_chunk_done_x86");                          // some chunks landed: report those bytes
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // nothing landed: the caller sees php false
    emitter.label("__rt_uw_chunk_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // the total the wrapper accepted
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
    emitter.label("__rt_fwrite_real_fd_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the payload pointer clobbered by resolution
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the payload length clobbered by resolution

    // -- look up the write filter for this descriptor --
    abi::emit_symbol_address(emitter, "r9", "_stream_write_filters");           // write-filter table base
    emitter.instruction("movzx ecx, BYTE PTR [r9 + rdi]");                      // write filter id for this descriptor
    emitter.instruction("test rcx, rcx");                                       // is a write filter attached?
    emitter.instruction("jz __rt_fwrite_direct_x86");                           // no filter: write the payload directly
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("jge __rt_fwrite_user_filter_x86");                     // dispatch through the user filter method, then write the result
    emitter.instruction("cmp rcx, 4");                                          // is the zlib.deflate write filter attached?
    emitter.instruction("je __rt_fwrite_zlib_x86");                             // hand the payload to the zlib deflate helper
    emitter.instruction("cmp rcx, 10");                                         // is the bzip2.compress write filter attached?
    emitter.instruction("je __rt_fwrite_bz2_x86");                              // hand the payload to the bzip2 compress helper
    emitter.instruction("cmp rcx, 12");                                         // is the convert.iconv write filter attached?
    emitter.instruction("je __rt_fwrite_iconv_x86");                            // hand the payload to the iconv write helper
    emitter.instruction(&format!("cmp rdx, {}", FILTER_BUF_SIZE));              // is the payload larger than the scratch?
    emitter.instruction("jg __rt_fwrite_direct_x86");                           // oversized payloads are written unfiltered

    // -- copy the payload into the filter scratch --
    abi::emit_symbol_address(emitter, "r8", "_stream_filter_buf");              // filter scratch base
    emitter.instruction("xor r9, r9");                                          // copy index
    emitter.label("__rt_fwrite_copy_x86");
    emitter.instruction("cmp r9, rdx");                                         // copied every byte?
    emitter.instruction("jge __rt_fwrite_copy_done_x86");                       // the payload is fully copied
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r9]");                     // load a payload byte
    emitter.instruction("mov BYTE PTR [r8 + r9], r10b");                        // store it into the filter scratch
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp __rt_fwrite_copy_x86");                            // continue copying
    emitter.label("__rt_fwrite_copy_done_x86");

    // -- transform the scratch copy and write from it --
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // the write reads the filtered scratch
    emitter.instruction("mov rax, r8");                                         // filter target = the scratch buffer
    emitter.instruction("call __rt_apply_stream_filter");                       // transform the scratch copy in place; rdx = (possibly compacted) length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // commit the post-filter length so strip_tags / similar shrinking filters write only the compacted bytes
    emitter.instruction("jmp __rt_fwrite_direct_x86");                          // continue with the standard direct write

    // -- zlib.deflate filter: deflate-compress the payload into the stream --
    emitter.label("__rt_fwrite_zlib_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd argument for the zlib deflate helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length argument
    abi::emit_load_symbol_to_reg(emitter, "r9", "_zlib_fwrite_fn", 0);          // load the deflate fwrite helper pointer
    emitter.instruction("call r9");                                             // deflate-compress the payload, rax = bytes consumed
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- bzip2.compress filter: bzip2-compress the payload into the stream --
    emitter.label("__rt_fwrite_bz2_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd argument for the bzip2 compress helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length argument
    abi::emit_load_symbol_to_reg(emitter, "r9", "_bz2_fwrite_fn", 0);           // load the bzip2 compress fwrite helper pointer
    emitter.instruction("call r9");                                             // bzip2-compress the payload, rax = bytes consumed
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- convert.iconv write filter: transcode the payload into the stream --
    emitter.label("__rt_fwrite_iconv_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd argument for the iconv write helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length argument
    abi::emit_load_symbol_to_reg(emitter, "r9", "_iconv_fwrite_fn", 0);         // load the iconv write helper pointer
    emitter.instruction("call r9");                                             // transcode the payload, rax = bytes consumed
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- user filter: dispatch through filter(string), then write the result --
    emitter.label("__rt_fwrite_user_filter_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd into the user-filter dispatcher's first arg
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload ptr → second arg
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload len → third arg
    emitter.instruction("mov ecx, 1");                                          // direction = 1 (write)
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← user filter's transformed payload
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // overwrite the payload-ptr slot with the filter result
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // overwrite the payload-len slot with the filter result
    emitter.instruction("jmp __rt_fwrite_direct_x86");                          // fall through to the standard direct-write path

    emitter.label("__rt_fwrite_direct_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // file descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer (original or filtered)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length
    // -- TLS dispatch (Phase 11 B3) --
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // the session resolved from the handle at entry
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_fwrite_syscall_x86");                          // plain TCP → libc write
    emitter.instruction("mov rdi, r11");                                        // handle as first arg
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_write_fn", 0);     // prepare SysV call argument
    emitter.instruction("call r9");                                             // rax = bytes written or -1
    emitter.instruction("jmp __rt_fwrite_return_x86");                          // continue at target label
    emitter.label("__rt_fwrite_syscall_x86");
    // -- an append stream records where its logical cursor was, before O_APPEND moves the
    //    descriptor to the end of the file --
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // -1 for every other stream
    emitter.instruction("cmp r10, -1");
    emitter.instruction("je __rt_fwrite_write_now_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the descriptor
    emitter.instruction("xor esi, esi");
    emitter.instruction("mov edx, 1");                                          // SEEK_CUR
    emitter.instruction("call lseek");                                          // rax = where PHP's position stood
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");
    emitter.label("__rt_fwrite_write_now_x86");
    // See the AArch64 counterpart: php://output writes to the OUTPUT-BUFFER stack, not to a
    // descriptor, so the bytes take the road `echo` takes and `ob_start()` captures them.
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // the php:// classification made at entry
    emitter.instruction(&format!("cmp r10, {PHP_WRITE_OUTPUT_BUFFER}"));
    emitter.instruction("jne __rt_fwrite_write_syscall_x86");                   // every other stream writes to its descriptor
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // payload pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // payload length
    emitter.instruction("call __rt_stdout_write");                              // print_r capture, ob stack, --web capture, then fd 1
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // php answers the whole byte count for this sink
    emitter.instruction("jmp __rt_fwrite_wrote_x86");
    emitter.label("__rt_fwrite_write_syscall_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the write arguments: the probe clobbered them
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");
    emitter.instruction("call write");                                          // write the payload through libc write()
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_fwrite_return_x86");                           // a failed write moved nothing to account for
    emitter.label("__rt_fwrite_wrote_x86");
    emit_append_skip_update_x86_64(emitter);
    emitter.label("__rt_fwrite_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte count from write
}

/// The x86_64 counterpart of [`emit_php_sub_wrapper_classify_aarch64`].
///
/// Takes the resolved StreamState in `rax` and LEAVES IT THERE. Clobbers `r9`-`r11` only.
fn emit_php_sub_wrapper_classify_x86_64(emitter: &mut Emitter) {
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r9, {WRAPPER_ID_PHP}"));
    emitter.instruction("jne __rt_fwrite_php_classified_x86");                  // every other wrapper keeps the mode gate
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_fwrite_php_classified_x86");                   // no URI: nothing to classify on
    emitter.instruction("cmp r11, 7");                                          // "php://" plus the byte that names the sub-wrapper
    emitter.instruction("jl __rt_fwrite_php_classified_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 6]");                       // the first byte of the php:// sub-wrapper name
    emitter.instruction("cmp r9b, 0x6F");                                       // 'o' as in output
    emitter.instruction("je __rt_fwrite_php_output_sink_x86");
    emitter.instruction("cmp r9b, 0x73");                                       // 's' as in stdin, stdout, stderr
    emitter.instruction("je __rt_fwrite_php_descriptor_sink_x86");
    emitter.instruction("cmp r9b, 0x66");                                       // 'f' — either "fd/" or "filter"
    emitter.instruction("jne __rt_fwrite_php_classified_x86");
    emitter.instruction("cmp r11, 9");                                          // "php://fd/" is the shortest spelling
    emitter.instruction("jl __rt_fwrite_php_classified_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 7]");
    emitter.instruction("cmp r9b, 0x64");                                       // 'd' — "filter" has 'i' here
    emitter.instruction("jne __rt_fwrite_php_classified_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 8]");
    emitter.instruction("cmp r9b, 0x2F");                                       // '/' closes "php://fd/"
    emitter.instruction("jne __rt_fwrite_php_classified_x86");
    emitter.label("__rt_fwrite_php_descriptor_sink_x86");
    emitter.instruction(&format!(
        "mov QWORD PTR [rbp - 64], {PHP_WRITE_DESCRIPTOR}"
    ));                                                                         // the descriptor decides, never the mode string
    emitter.instruction("jmp __rt_fwrite_mode_ok_x86");                         // skip the read-only gate entirely
    emitter.label("__rt_fwrite_php_output_sink_x86");
    emitter.instruction(&format!(
        "mov QWORD PTR [rbp - 64], {PHP_WRITE_OUTPUT_BUFFER}"
    ));                                                                         // the write travels the output-buffer stack
    emitter.instruction("jmp __rt_fwrite_mode_ok_x86");                         // php://output is always writable
    emitter.label("__rt_fwrite_php_classified_x86");
}

/// The x86_64 counterpart of [`emit_append_skip_update_aarch64`].
fn emit_append_skip_update_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // -1 unless this is an append stream
    emitter.instruction("cmp r10, -1");
    emitter.instruction("je __rt_fwrite_return_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // hold the byte count across the calls below
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the descriptor
    emitter.instruction("xor esi, esi");
    emitter.instruction("mov edx, 1");                                          // SEEK_CUR
    emitter.instruction("call lseek");                                          // rax = where the descriptor ended up
    emitter.instruction("sub rax, QWORD PTR [rbp - 56]");                       // where the file ended before the write
    emitter.instruction("sub rax, QWORD PTR [rbp - 48]");                       // the bytes O_APPEND jumped over
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // hold the delta across the state lookup
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the handle the caller passed
    emitter.instruction("call __rt_stream_state");                              // rax = the owning state, zero for a raw descriptor
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fwrite_skip_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");
    emitter.instruction(&format!("add QWORD PTR [rax + {STREAM_APPEND_SKIP_OFFSET}], r10")); // accumulate across writes
    emitter.label("__rt_fwrite_skip_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // the byte count the caller is owed
}
