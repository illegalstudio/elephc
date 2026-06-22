//! Purpose:
//! Emits the `bzip2.compress` (write-direction) and `bzip2.decompress`
//! (read-direction) stream filter attachments for `stream_filter_append` /
//! `stream_filter_prepend`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::stream_filter::emit_attach()` when the
//!   filter-name literal is `"bzip2.compress"` or `"bzip2.decompress"`.
//!
//! Key details:
//! - `bzip2.compress` mirrors the `zlib.deflate` write filter
//!   (`stream_filter_zlib.rs`): a per-fd `bz_stream` is initialized with
//!   `BZ2_bzCompressInit`, each `fwrite` streams the payload through
//!   `BZ2_bzCompress(BZ_RUN)` into the shared scratch window, and `fclose`
//!   flushes the tail via `BZ2_bzCompress(BZ_FINISH)` until `BZ_STREAM_END`,
//!   then `BZ2_bzCompressEnd`. The libbz2 symbols are named ONLY from this
//!   per-program USER asm; the shared runtime reaches them indirectly through
//!   the `_bz2_fwrite_fn` / `_bz2_close_fn` slots, so non-bzip2 programs never
//!   link `-lbz2`. The write-filter table entry is id 10 and the per-fd handle
//!   lives in `_bzstream_handles`; the attach sets BOTH.
//! - `bzip2.decompress` reuses the already-shipped `compress.bzip2://` read core
//!   (`compress_bzip2_stream::emit_arm64`/`emit_x86_64`): slurp the whole
//!   compressed stream, one-shot `BZ2_bzBuffToBuffDecompress`, write to a temp
//!   file, `dup2` onto the descriptor — so later `fread`/`fseek`/`feof` work
//!   unchanged. Those helpers already re-box the descriptor as a resource.
//! - `bz_stream` is LP64-sized (80 bytes): next_in@0, avail_in@8(u32),
//!   next_out@24, avail_out@32(u32) — the same offsets as `z_stream`. Zeroing it
//!   leaves bzalloc/bzfree/opaque NULL so libbz2 uses its default allocator. The
//!   struct itself is intentionally not freed on close (small documented leak,
//!   matching the zlib filter).

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Size of the libbz2 `bz_stream` struct on LP64 targets, in bytes.
const BZ_STREAM_SIZE: i64 = 80;
/// Capacity of the shared `_stream_filter_buf` scratch used as the compress
/// output window.
const FILTER_BUF_SIZE: i64 = 65536;
/// x86_64 owned-heap kind word: the elephc heap marker in the high 32 bits.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `bzip2.compress` write-filter attachment. Returns the stream
/// re-boxed as a resource, matching `stream_filter_append`'s contract.
pub fn emit_bzip2_compress_attach(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_append(bzip2.compress)");
    emit_stream_fd_arg("stream_filter_append", &args[0], emitter, ctx, data);

    // PHP's 4th `$params` arg, as either a bare int (`$rw, 1`) setting the bzip2
    // blockSize100k (1..9), or the canonical array form (`['blocks' => 1,
    // 'work' => 30]`) from which `blocks` (blockSize100k) and `work` (workFactor,
    // 0..250) are read. Both literal forms are honored at compile time; anything
    // else keeps the defaults (blocks 9, work 0 = libbz2's default).
    let block_size = super::stream_filter::const_int_param(args, "blocks", true, 1, 9).unwrap_or(9);
    let work_factor =
        super::stream_filter::const_int_param(args, "work", false, 0, 250).unwrap_or(0);

    let fwrite_label = ctx.next_label("bz2_compress_fwrite");
    let close_label = ctx.next_label("bz2_compress_close");
    let skip_label = ctx.next_label("bz2_compress_skip_helpers");

    match emitter.target.arch {
        Arch::AArch64 => emit_compress_arm64(
            emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            block_size,
            work_factor,
        ),
        Arch::X86_64 => emit_compress_x86_64(
            emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            block_size,
            work_factor,
        ),
    }
    Some(PhpType::Mixed)
}

/// Emits the `bzip2.decompress` read-filter attachment. The descriptor is
/// already open (from `emit_stream_fd_arg`); the shipped `compress.bzip2://`
/// read core slurps, decompresses, and `dup2`s a temp file onto it, then
/// re-boxes the descriptor as a resource.
pub fn emit_bzip2_decompress_attach(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_append(bzip2.decompress)");
    emit_stream_fd_arg("stream_filter_append", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => super::compress_bzip2_stream::emit_arm64(emitter, |prefix| ctx.next_label(prefix)),
        Arch::X86_64 => super::compress_bzip2_stream::emit_x86_64(emitter, |prefix| ctx.next_label(prefix)),
    }
    Some(PhpType::Mixed)
}

/// Emits the reusable ARM64 `bzip2.decompress` read-filter body.
pub(crate) fn emit_decompress_arm64<F>(emitter: &mut Emitter, next_label: F)
where
    F: FnMut(&str) -> String,
{
    super::compress_bzip2_stream::emit_arm64(emitter, next_label);
}

/// Emits the reusable x86_64 `bzip2.decompress` read-filter body.
pub(crate) fn emit_decompress_x86_64<F>(emitter: &mut Emitter, next_label: F)
where
    F: FnMut(&str) -> String,
{
    super::compress_bzip2_stream::emit_x86_64(emitter, next_label);
}

/// Emits the ARM64 compress helpers, then the bz_stream initialization.
/// `work_factor` is the bzip2 workFactor (0..250, 0 = libbz2 default) from the
/// `['work' => N]` `$params` entry.
pub(crate) fn emit_compress_arm64(
    emitter: &mut Emitter,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
    block_size: i64,
    work_factor: i64,
) {
    // -- jump past the helper bodies so normal flow never falls into them --
    emitter.instruction(&format!("b {}", skip_label));                          // skip over the inline bzip2 helper routines

    // ================================================================
    // bzip2 compress fwrite helper.
    // Input:  x0 = fd, x1 = payload pointer, x2 = payload length.
    // Output: x0 = the input payload length (bytes "written").
    // ================================================================
    emitter.label(fwrite_label);
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]=fd [8]=length [16]=bz_stream ptr [32]=x29 [40]=x30
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor for the write loop
    emitter.instruction("str x2, [sp, #8]");                                    // save the payload length as the return value

    // -- load this descriptor's bz_stream handle and seed the input window --
    abi::emit_symbol_address(emitter, "x9", "_bzstream_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // x10 = bz_stream pointer for this descriptor
    emitter.instruction("str x10, [sp, #16]");                                  // save the bz_stream pointer across the calls
    emitter.instruction("str x1, [x10, #0]");                                   // bz_stream.next_in = payload pointer
    emitter.instruction("str w2, [x10, #8]");                                   // bz_stream.avail_in = payload length

    // -- compress loop: drain next_in into the scratch window and write it out --
    emitter.label(&format!("{}_loop", fwrite_label));
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the bz_stream pointer
    abi::emit_symbol_address(emitter, "x11", "_stream_filter_buf");
    emitter.instruction("str x11, [x10, #24]");                                 // bz_stream.next_out = scratch window base
    emitter.instruction(&format!("mov w12, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w12, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); //high half of the scratch window capacity
    emitter.instruction("str w12, [x10, #32]");                                 // bz_stream.avail_out = scratch window capacity
    emitter.instruction("mov x0, x10");                                         // arg 0 = bz_stream pointer
    emitter.instruction("mov w1, #0");                                          // arg 1 = BZ_RUN (0)
    emitter.bl_c("BZ2_bzCompress");                                             // run one compress step over the input window
    // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the bz_stream pointer after the compress call
    emitter.instruction("ldr w12, [x10, #32]");                                 // reload avail_out left after this compress step
    emitter.instruction(&format!("mov w13, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w13, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); //high half of the scratch window capacity
    emitter.instruction("sub w12, w13, w12");                                   // produced = capacity - avail_out
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("uxtw x2, w12");                                        // produced byte count as the write length
    emitter.syscall(4);
    // -- repeat while input remains OR the output window filled completely --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the bz_stream pointer after the write
    emitter.instruction("ldr w14, [x10, #8]");                                  // reload avail_in still pending
    emitter.instruction(&format!("cbnz w14, {}_loop", fwrite_label));           // more input bytes: keep compressing
    emitter.instruction("ldr w12, [x10, #32]");                                 // reload avail_out left after this compress step
    emitter.instruction(&format!("cbz w12, {}_loop", fwrite_label));            // window was filled: drain the remainder
    // -- done: return the original payload length --
    emitter.instruction("ldr x0, [sp, #8]");                                    // return value = the saved payload length
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bytes-consumed count

    // ================================================================
    // bzip2 compress close helper.
    // Input:  x0 = fd. Flushes the compress tail and ends the stream.
    // ================================================================
    emitter.label(close_label);
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]=fd [8]=bz_stream [16]=ret code [32]=x29 [40]=x30
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_bzstream_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // x10 = bz_stream pointer for this descriptor
    emitter.instruction(&format!("cbz x10, {}_done", close_label));             // nothing to flush when no filter is attached
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor across compress calls
    emitter.instruction("str x10, [sp, #8]");                                   // save the bz_stream pointer
    emitter.instruction("str xzr, [x10, #0]");                                  // bz_stream.next_in = NULL: no further input
    emitter.instruction("str wzr, [x10, #8]");                                  // bz_stream.avail_in = 0: input is exhausted

    // -- flush loop: compress with BZ_FINISH until BZ_STREAM_END --
    emitter.label(&format!("{}_loop", close_label));
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the bz_stream pointer
    abi::emit_symbol_address(emitter, "x11", "_stream_filter_buf");
    emitter.instruction("str x11, [x10, #24]");                                 // bz_stream.next_out = scratch window base
    emitter.instruction(&format!("mov w12, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w12, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); //high half of the scratch window capacity
    emitter.instruction("str w12, [x10, #32]");                                 // bz_stream.avail_out = scratch window capacity
    emitter.instruction("mov x0, x10");                                         // arg 0 = bz_stream pointer
    emitter.instruction("mov w1, #2");                                          // arg 1 = BZ_FINISH (2)
    emitter.bl_c("BZ2_bzCompress");                                             // flush a chunk of the compressed tail
    emitter.instruction("str x0, [sp, #16]");                                   // save the compress return code (4 = BZ_STREAM_END)
    // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the bz_stream pointer
    emitter.instruction("ldr w12, [x10, #32]");                                 // reload avail_out left after this flush step
    emitter.instruction(&format!("mov w13, #{}", FILTER_BUF_SIZE & 0xFFFF));    // low half of the scratch window capacity
    emitter.instruction(&format!("movk w13, #{}, lsl #16", FILTER_BUF_SIZE >> 16)); //high half of the scratch window capacity
    emitter.instruction("sub w12, w13, w12");                                   // produced = capacity - avail_out
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("uxtw x2, w12");                                        // produced byte count as the write length
    emitter.syscall(4);
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload the saved compress return code
    emitter.instruction("cmp x12, #4");                                         // did BZ2_bzCompress report BZ_STREAM_END?
    emitter.instruction(&format!("b.ne {}_loop", close_label));                 // not finished yet: flush another chunk

    // -- end the compress stream and drop the per-descriptor handle --
    emitter.instruction("ldr x0, [sp, #8]");                                    // arg 0 = bz_stream pointer
    emitter.bl_c("BZ2_bzCompressEnd");                                          // release libbz2's internal compress state
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    abi::emit_symbol_address(emitter, "x9", "_bzstream_handles");
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear this descriptor's bz_stream handle
    emitter.label(&format!("{}_done", close_label));
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the fclose path

    // ================================================================
    // Initialization: allocate and register a bz_stream for this fd.
    // ================================================================
    emitter.label(skip_label);
    emitter.instruction("sub sp, sp, #16");                                     // frame: [0]=fd [8]=bz_stream pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor across the calls
    emitter.instruction(&format!("mov x0, #{}", BZ_STREAM_SIZE));               // request a bz_stream-sized heap block
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the bz_stream struct, x0 = payload
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned allocation
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the bz_stream block as owned heap state
    emitter.instruction("str x0, [sp, #8]");                                    // save the bz_stream pointer

    // -- zero all 80 bytes so bzalloc/bzfree/opaque are NULL and counters start clean --
    emitter.instruction("mov x9, #0");                                          // byte clear index
    emitter.label(&format!("{}_zero", skip_label));
    emitter.instruction(&format!("cmp x9, #{}", BZ_STREAM_SIZE));               // cleared the whole bz_stream struct?
    emitter.instruction(&format!("b.ge {}_zeroed", skip_label));                // the struct is fully zeroed
    emitter.instruction("strb wzr, [x0, x9]");                                  // zero one bz_stream byte
    emitter.instruction("add x9, x9, #1");                                      // advance the clear index
    emitter.instruction(&format!("b {}_zero", skip_label));                     // continue zeroing the struct
    emitter.label(&format!("{}_zeroed", skip_label));

    // -- BZ2_bzCompressInit(strm, blockSize100k, verbosity=0, workFactor) --
    emitter.instruction("ldr x0, [sp, #8]");                                    // arg 0 = bz_stream pointer
    emitter.instruction(&format!("mov x1, #{}", block_size));                   // arg 1 = blockSize100k ($params 'blocks', default 9 = max)
    emitter.instruction("mov x2, #0");                                          // arg 2 = verbosity = 0
    emitter.instruction(&format!("mov x3, #{}", work_factor));                  // arg 3 = workFactor ($params 'work', default 0 = libbz2 default)
    emitter.bl_c("BZ2_bzCompressInit");                                         // initialize the bzip2 compress stream

    // -- register the handle and mark the descriptor's write filter as bzip2 --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the bz_stream pointer
    abi::emit_symbol_address(emitter, "x9", "_bzstream_handles");
    emitter.instruction("str x10, [x9, x0, lsl #3]");                           // store the bz_stream handle for this descriptor
    abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
    emitter.instruction("mov w11, #10");                                        // write-filter id 10 = bzip2.compress
    emitter.instruction("strb w11, [x9, x0]");                                  // record the bzip2 write filter for this descriptor

    // -- publish the helper addresses so __rt_fwrite / fclose can call them --
    abi::emit_symbol_address(emitter, "x11", fwrite_label);
    abi::emit_symbol_address(emitter, "x9", "_bz2_fwrite_fn");
    emitter.instruction("str x11, [x9]");                                       // _bz2_fwrite_fn = the compress fwrite helper
    abi::emit_symbol_address(emitter, "x11", close_label);
    abi::emit_symbol_address(emitter, "x9", "_bz2_close_fn");
    emitter.instruction("str x11, [x9]");                                       // _bz2_close_fn = the compress close helper

    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("add sp, sp, #16");                                     // release the initialization frame
    emitter.instruction("mov x1, x0");                                          // resource payload = the descriptor
    emitter.instruction("mov x2, #0");                                          // resource mixed payloads have no high word
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}

/// Emits the x86_64 compress helpers, then the bz_stream initialization.
/// `work_factor` is the bzip2 workFactor (0..250, 0 = libbz2 default) from the
/// `['work' => N]` `$params` entry.
pub(crate) fn emit_compress_x86_64(
    emitter: &mut Emitter,
    fwrite_label: &str,
    close_label: &str,
    skip_label: &str,
    block_size: i64,
    work_factor: i64,
) {
    // -- jump past the helper bodies so normal flow never falls into them --
    emitter.instruction(&format!("jmp {}", skip_label));                        // skip over the inline bzip2 helper routines

    // ================================================================
    // bzip2 compress fwrite helper.
    // Input:  rdi = fd, rsi = payload pointer, rdx = payload length.
    // Output: rax = the input payload length (bytes "written").
    // ================================================================
    emitter.label(fwrite_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame: [-8]=fd [-16]=length [-24]=bz_stream
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor for the write loop
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the payload length as the return value

    // -- load this descriptor's bz_stream handle and seed the input window --
    abi::emit_symbol_address(emitter, "r9", "_bzstream_handles");               // bz_stream handle table base
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi*8]");                     // r10 = bz_stream pointer for this descriptor
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the bz_stream pointer
    emitter.instruction("mov QWORD PTR [r10 + 0], rsi");                        // bz_stream.next_in = payload pointer
    emitter.instruction("mov DWORD PTR [r10 + 8], edx");                        // bz_stream.avail_in = payload length

    // -- compress loop: drain next_in into the scratch window and write it out --
    emitter.label(&format!("{}_loop", fwrite_label));
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the bz_stream pointer
    abi::emit_symbol_address(emitter, "r11", "_stream_filter_buf");             // scratch window base
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // bz_stream.next_out = scratch window base
    emitter.instruction(&format!("mov DWORD PTR [r10 + 32], {}", FILTER_BUF_SIZE)); //bz_stream.avail_out = scratch window capacity
    emitter.instruction("mov rdi, r10");                                        // arg 0 = bz_stream pointer
    emitter.instruction("xor esi, esi");                                        // arg 1 = BZ_RUN (0)
    emitter.instruction("call BZ2_bzCompress");                                 // run one compress step over the input window
    // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the bz_stream pointer
    emitter.instruction(&format!("mov eax, {}", FILTER_BUF_SIZE));              // scratch window capacity
    emitter.instruction("sub eax, DWORD PTR [r10 + 32]");                       // produced = capacity - avail_out
    emitter.instruction("mov edx, eax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the saved file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf");             // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the compressed chunk through libc write()
    // -- repeat while input remains OR the output window filled completely --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the bz_stream pointer
    emitter.instruction("cmp DWORD PTR [r10 + 8], 0");                          // any avail_in input bytes still pending?
    emitter.instruction(&format!("jne {}_loop", fwrite_label));                 // more input bytes: keep compressing
    emitter.instruction("cmp DWORD PTR [r10 + 32], 0");                         // did the output window fill completely?
    emitter.instruction(&format!("je {}_loop", fwrite_label));                  // window was filled: drain the remainder
    // -- done: return the original payload length --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return value = the saved payload length
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bytes-consumed count

    // ================================================================
    // bzip2 compress close helper.
    // Input:  rdi = fd. Flushes the compress tail and ends the stream.
    // ================================================================
    emitter.label(close_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame: [-8]=fd [-16]=bz_stream [-24]=ret code
    abi::emit_symbol_address(emitter, "r9", "_bzstream_handles");               // bz_stream handle table base
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi*8]");                     // r10 = bz_stream pointer for this descriptor
    emitter.instruction("test r10, r10");                                       // is a compress stream attached to this descriptor?
    emitter.instruction(&format!("jz {}_done", close_label));                   // nothing to flush when no filter is attached
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor across compress calls
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the bz_stream pointer
    emitter.instruction("mov QWORD PTR [r10 + 0], 0");                          // bz_stream.next_in = NULL: no further input
    emitter.instruction("mov DWORD PTR [r10 + 8], 0");                          // bz_stream.avail_in = 0: input is exhausted

    // -- flush loop: compress with BZ_FINISH until BZ_STREAM_END --
    emitter.label(&format!("{}_loop", close_label));
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the bz_stream pointer
    abi::emit_symbol_address(emitter, "r11", "_stream_filter_buf");             // scratch window base
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // bz_stream.next_out = scratch window base
    emitter.instruction(&format!("mov DWORD PTR [r10 + 32], {}", FILTER_BUF_SIZE)); //bz_stream.avail_out = scratch window capacity
    emitter.instruction("mov rdi, r10");                                        // arg 0 = bz_stream pointer
    emitter.instruction("mov esi, 2");                                          // arg 1 = BZ_FINISH (2)
    emitter.instruction("call BZ2_bzCompress");                                 // flush a chunk of the compressed tail
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the compress return code (4 = BZ_STREAM_END)
    // -- compute produced = capacity - avail_out and write it to the fd --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the bz_stream pointer
    emitter.instruction(&format!("mov eax, {}", FILTER_BUF_SIZE));              // scratch window capacity
    emitter.instruction("sub eax, DWORD PTR [r10 + 32]");                       // produced = capacity - avail_out
    emitter.instruction("mov edx, eax");                                        // produced byte count as the write length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd = the preserved file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf");             // write buffer = the scratch window base
    emitter.instruction("call write");                                          // write the compressed tail chunk through libc write()
    emitter.instruction("cmp QWORD PTR [rbp - 24], 4");                         // did BZ2_bzCompress report BZ_STREAM_END?
    emitter.instruction(&format!("jne {}_loop", close_label));                  // not finished yet: flush another chunk

    // -- end the compress stream and drop the per-descriptor handle --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = bz_stream pointer
    emitter.instruction("call BZ2_bzCompressEnd");                              // release libbz2's internal compress state
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    abi::emit_symbol_address(emitter, "r9", "_bzstream_handles");               // bz_stream handle table base
    emitter.instruction("mov QWORD PTR [r9 + rdi*8], 0");                       // clear this descriptor's bz_stream handle
    emitter.label(&format!("{}_done", close_label));
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the fclose path

    // ================================================================
    // Initialization: allocate and register a bz_stream for this fd.
    // ================================================================
    emitter.label(skip_label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the initialization frame pointer
    emitter.instruction("sub rsp, 24");                                         // frame: [-8]=fd [-16]=bz_stream ptr (24: this inline block enters rsp 16-aligned, push rbp made it 8, so +24≡8 mod 16 realigns to 0 at the libc calls)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the file descriptor across the calls
    emitter.instruction(&format!("mov rax, {}", BZ_STREAM_SIZE));               // request a bz_stream-sized heap block
    emitter.instruction("call __rt_heap_alloc");                                // allocate the bz_stream struct, rax = payload
    emitter.instruction(&format!(                                               // owned-heap kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the bz_stream block as owned heap state
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the bz_stream pointer

    // -- zero all 80 bytes so bzalloc/bzfree/opaque are NULL and counters start clean --
    emitter.instruction("xor r9, r9");                                          // byte clear index
    emitter.label(&format!("{}_zero", skip_label));
    emitter.instruction(&format!("cmp r9, {}", BZ_STREAM_SIZE));                // cleared the whole bz_stream struct?
    emitter.instruction(&format!("jge {}_zeroed", skip_label));                 // the struct is fully zeroed
    emitter.instruction("mov BYTE PTR [rax + r9], 0");                          // zero one bz_stream byte
    emitter.instruction("inc r9");                                              // advance the clear index
    emitter.instruction(&format!("jmp {}_zero", skip_label));                   // continue zeroing the struct
    emitter.label(&format!("{}_zeroed", skip_label));

    // -- BZ2_bzCompressInit(strm, blockSize100k, verbosity=0, workFactor) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = bz_stream pointer
    emitter.instruction(&format!("mov esi, {}", block_size));                   // arg 1 = blockSize100k ($params 'blocks', default 9 = max)
    emitter.instruction("xor edx, edx");                                        // arg 2 = verbosity = 0
    emitter.instruction(&format!("mov ecx, {}", work_factor));                  // arg 3 = workFactor ($params 'work', default 0 = libbz2 default)
    emitter.instruction("call BZ2_bzCompressInit");                             // initialize the bzip2 compress stream

    // -- register the handle and mark the descriptor's write filter as bzip2 --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the bz_stream pointer
    abi::emit_symbol_address(emitter, "r9", "_bzstream_handles");               // bz_stream handle table base
    emitter.instruction("mov QWORD PTR [r9 + rdi*8], r10");                     // store the bz_stream handle for this descriptor
    abi::emit_symbol_address(emitter, "r9", "_stream_write_filters");           // write-filter table base
    emitter.instruction("mov BYTE PTR [r9 + rdi], 10");                         // write-filter id 10 = bzip2.compress

    // -- publish the helper addresses so __rt_fwrite / fclose can call them --
    emitter.instruction(&format!("lea r10, [rip + {}]", fwrite_label));         // address of the compress fwrite helper
    abi::emit_symbol_address(emitter, "r9", "_bz2_fwrite_fn");                  // _bz2_fwrite_fn slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // _bz2_fwrite_fn = the compress fwrite helper
    emitter.instruction(&format!("lea r10, [rip + {}]", close_label));          // address of the compress close helper
    abi::emit_symbol_address(emitter, "r9", "_bz2_close_fn");                   // _bz2_close_fn slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // _bz2_close_fn = the compress close helper

    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // resource payload = the descriptor
    emitter.instruction("add rsp, 24");                                         // release the initialization frame (matches the aligned sub rsp, 24)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("xor esi, esi");                                        // resource mixed payloads have no high word
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}
