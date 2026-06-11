//! Purpose:
//! Emits the `zlib.inflate` read-direction stream filter attachment for
//! `stream_filter_append` / `stream_filter_prepend`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::stream_filter::emit_attach()` when the
//!   filter-name literal is `"zlib.inflate"`.
//!
//! Key details:
//! - Unlike the 1:1 `string.*` filters and the streaming `zlib.deflate` write
//!   filter, `zlib.inflate` is implemented by transforming the descriptor
//!   itself: at attach time the whole compressed stream is slurped, inflated
//!   once, written to an anonymous temp file, and `dup2`'d onto the original
//!   descriptor. Every later `fread`/`fseek`/`feof` then works unchanged — no
//!   per-fd filter state and no `__rt_fread` change are needed.
//! - The libz symbols (`inflateInit2_`, `inflate`, `inflateEnd`) are referenced
//!   only from this builtin's USER asm, so the shared runtime stays libz-free.
//! - v1 caps the compressed input at the 64 KiB `_stream_filter_buf` scratch
//!   and sizes the inflate output at 256x the input (min 64 KiB).

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Capacity of the shared `_stream_filter_buf` scratch, reused here as the
/// compressed-input slurp buffer.
const FILTER_BUF_SIZE: i64 = 65536;
/// x86_64 owned-heap kind word: the elephc heap marker in the high 32 bits.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `zlib.inflate` read-filter attachment. Returns the stream re-boxed
/// as a resource, matching `stream_filter_append`'s contract.
pub fn emit_zlib_inflate_attach(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_append(zlib.inflate)");
    emit_stream_fd_arg("stream_filter_append", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => emit_arm64(emitter, ctx),
        Arch::X86_64 => emit_x86_64(emitter, ctx),
    }
    Some(PhpType::Mixed)
}

/// ARM64: a 176-byte scratch frame holds the 112-byte `z_stream` at `[sp, #0]`
/// plus the descriptor, lengths, and buffers at `[sp, #112..168)`.
pub(super) fn emit_arm64(emitter: &mut Emitter, ctx: &mut Context) {
    let slurp = ctx.next_label("zlib_inflate_slurp");
    let slurp_done = ctx.next_label("zlib_inflate_slurped");
    let zero = ctx.next_label("zlib_inflate_zero");
    let zeroed = ctx.next_label("zlib_inflate_zeroed");
    let write = ctx.next_label("zlib_inflate_write");
    let write_done = ctx.next_label("zlib_inflate_written");

    emitter.instruction("sub sp, sp, #176");                                    // z_stream frame plus saved values
    emitter.instruction("str x0, [sp, #112]");                                  // save the source file descriptor

    // -- slurp every compressed byte from the descriptor into the scratch --
    emitter.instruction("str xzr, [sp, #120]");                                 // slurp offset = 0
    emitter.label(&slurp);
    emitter.instruction("ldr x0, [sp, #112]");                                  // fd to read compressed bytes from
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("ldr x9, [sp, #120]");                                  // current slurp offset
    emitter.instruction("add x1, x1, x9");                                      // write pointer = scratch base + offset
    emitter.instruction(&format!("mov x2, #{}", FILTER_BUF_SIZE));              // scratch capacity
    emitter.instruction("sub x2, x2, x9");                                      // remaining scratch capacity
    emitter.syscall(3);
    emitter.instruction("cmp x0, #0");                                          // did the read hit EOF or fail?
    emitter.instruction(&format!("b.le {}", slurp_done));                       // stop slurping at EOF or on error
    emitter.instruction("ldr x9, [sp, #120]");                                  // reload the slurp offset
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just read
    emitter.instruction("str x9, [sp, #120]");                                  // store the updated compressed length
    emitter.instruction(&format!("mov x10, #{}", FILTER_BUF_SIZE));             // scratch capacity
    emitter.instruction("cmp x9, x10");                                         // is the scratch buffer full?
    emitter.instruction(&format!("b.lt {}", slurp));                            // room remains: keep slurping
    emitter.label(&slurp_done);

    // -- size and allocate the inflate output buffer (256x input, min 64 KiB) --
    emitter.instruction("ldr x9, [sp, #120]");                                  // compressed length
    emitter.instruction("lsl x9, x9, #8");                                      // budget 256x the compressed size
    emitter.instruction(&format!("mov x10, #{}", FILTER_BUF_SIZE));             // minimum output buffer size
    emitter.instruction("cmp x9, x10");                                         // is the 256x budget larger?
    emitter.instruction("csel x9, x9, x10, gt");                                // pick the larger buffer size
    emitter.instruction("str x9, [sp, #152]");                                  // save the output buffer capacity
    emitter.instruction("mov x0, x9");                                          // buffer size into the allocator argument
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the decompressed-data buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("str x0, [sp, #128]");                                  // save the decompressed buffer pointer

    // -- zero the 112-byte z_stream so zalloc/zfree start NULL --
    emitter.instruction("mov x9, #0");                                          // z_stream byte clear index
    emitter.label(&zero);
    emitter.instruction("cmp x9, #112");                                        // cleared the whole z_stream struct?
    emitter.instruction(&format!("b.ge {}", zeroed));                           // the struct is fully zeroed
    emitter.instruction("strb wzr, [sp, x9]");                                  // zero one z_stream byte
    emitter.instruction("add x9, x9, #1");                                      // advance the clear index
    emitter.instruction(&format!("b {}", zero));                                // continue zeroing the struct
    emitter.label(&zeroed);

    // -- inflateInit2_(strm, -15, version, size): -15 selects raw inflate --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #-15");                                        // arg 1 = windowBits -15: raw inflate
    abi::emit_symbol_address(emitter, "x2", "_zlib_version");
    emitter.instruction("mov x3, #112");                                        // arg 3 = sizeof(z_stream) for the ABI check
    emitter.bl_c("inflateInit2_");                                              // initialize a raw-inflate zlib stream

    // -- point the stream at the slurped input and the output buffer --
    abi::emit_symbol_address(emitter, "x9", "_stream_filter_buf");
    emitter.instruction("str x9, [sp, #0]");                                    // z_stream.next_in = scratch base
    emitter.instruction("ldr x9, [sp, #120]");                                  // compressed length
    emitter.instruction("str w9, [sp, #8]");                                    // z_stream.avail_in = compressed length
    emitter.instruction("ldr x9, [sp, #128]");                                  // decompressed buffer pointer
    emitter.instruction("str x9, [sp, #24]");                                   // z_stream.next_out = decompressed buffer
    emitter.instruction("ldr x9, [sp, #152]");                                  // output buffer capacity
    emitter.instruction("str w9, [sp, #32]");                                   // z_stream.avail_out = output capacity

    // -- inflate the whole input in a single Z_FINISH pass --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #4");                                          // arg 1 = Z_FINISH
    emitter.bl_c("inflate");                                                    // decompress the entire input at once
    emitter.instruction("ldr x9, [sp, #40]");                                   // z_stream.total_out = decompressed length
    emitter.instruction("str x9, [sp, #136]");                                  // save the decompressed length
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.bl_c("inflateEnd");                                                 // release zlib's internal inflate state

    // -- back the descriptor with an anonymous temp file of the plain bytes --
    emitter.instruction("bl __rt_tmpfile");                                     // create an unlinked temp file, x0 = fd
    emitter.instruction("str x0, [sp, #144]");                                  // save the temp-file descriptor

    // -- write loop: copy every decompressed byte into the temp file --
    emitter.instruction("str xzr, [sp, #160]");                                 // write offset = 0
    emitter.label(&write);
    emitter.instruction("ldr x10, [sp, #136]");                                 // total decompressed length
    emitter.instruction("ldr x9, [sp, #160]");                                  // current write offset
    emitter.instruction("cmp x9, x10");                                         // copied every decompressed byte?
    emitter.instruction(&format!("b.ge {}", write_done));                       // the whole payload is written
    emitter.instruction("ldr x0, [sp, #144]");                                  // temp-file descriptor
    emitter.instruction("ldr x1, [sp, #128]");                                  // decompressed buffer pointer
    emitter.instruction("add x1, x1, x9");                                      // write pointer = buffer + offset
    emitter.instruction("sub x2, x10, x9");                                     // remaining bytes to write
    emitter.syscall(4);
    emitter.instruction("cmp x0, #0");                                          // did the write make progress?
    emitter.instruction(&format!("b.le {}", write_done));                       // stop on a write error
    emitter.instruction("ldr x9, [sp, #160]");                                  // reload the write offset
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just written
    emitter.instruction("str x9, [sp, #160]");                                  // store the updated write offset
    emitter.instruction(&format!("b {}", write));                               // continue writing the payload
    emitter.label(&write_done);

    // -- lseek(temp, 0, SEEK_SET): rewind so reads start at the plain bytes --
    emitter.instruction("ldr x0, [sp, #144]");                                  // temp-file descriptor
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET
    emitter.syscall(199);

    // -- dup2(temp, fd): the descriptor now serves the decompressed bytes --
    emitter.instruction("ldr x0, [sp, #144]");                                  // oldfd = temp file
    emitter.instruction("ldr x1, [sp, #112]");                                  // newfd = the stream descriptor
    emitter.bl_c("dup2");                                                       // redirect the descriptor onto the temp file

    // -- close the now-redundant temp-file descriptor --
    emitter.instruction("ldr x0, [sp, #144]");                                  // the temp-file descriptor
    emitter.syscall(6);

    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("ldr x0, [sp, #112]");                                  // reload the stream descriptor
    emitter.instruction("add sp, sp, #176");                                    // release the scratch frame
    emitter.instruction("mov x1, x0");                                          // resource payload = the descriptor
    emitter.instruction("mov x2, #0");                                          // resource mixed payloads have no high word
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}

/// x86_64: same 176-byte scratch layout; `inflateInit2_` takes four register
/// arguments, so no stack-argument shuffling is needed.
pub(super) fn emit_x86_64(emitter: &mut Emitter, ctx: &mut Context) {
    let slurp = ctx.next_label("zlib_inflate_slurp");
    let slurp_done = ctx.next_label("zlib_inflate_slurped");
    let sized = ctx.next_label("zlib_inflate_sized");
    let zero = ctx.next_label("zlib_inflate_zero");
    let zeroed = ctx.next_label("zlib_inflate_zeroed");
    let write = ctx.next_label("zlib_inflate_write");
    let write_done = ctx.next_label("zlib_inflate_written");

    emitter.instruction("sub rsp, 176");                                        // z_stream frame plus saved values
    emitter.instruction("mov QWORD PTR [rsp + 112], rax");                      // save the source file descriptor

    // -- slurp every compressed byte from the descriptor into the scratch --
    emitter.instruction("mov QWORD PTR [rsp + 120], 0");                        // slurp offset = 0
    emitter.label(&slurp);
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // fd to read compressed bytes from
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf");             // scratch base address
    emitter.instruction("add rsi, QWORD PTR [rsp + 120]");                      // write pointer = scratch base + offset
    emitter.instruction(&format!("mov rdx, {}", FILTER_BUF_SIZE));              // scratch capacity
    emitter.instruction("sub rdx, QWORD PTR [rsp + 120]");                      // remaining scratch capacity
    emitter.instruction("call read");                                           // read compressed bytes through libc read()
    emitter.instruction("cmp rax, 0");                                          // did the read hit EOF or fail?
    emitter.instruction(&format!("jle {}", slurp_done));                        // stop slurping at EOF or on error
    emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                       // reload the slurp offset
    emitter.instruction("add r9, rax");                                         // advance by the bytes just read
    emitter.instruction("mov QWORD PTR [rsp + 120], r9");                       // store the updated compressed length
    emitter.instruction(&format!("cmp r9, {}", FILTER_BUF_SIZE));               // is the scratch buffer full?
    emitter.instruction(&format!("jl {}", slurp));                              // room remains: keep slurping
    emitter.label(&slurp_done);

    // -- size and allocate the inflate output buffer (256x input, min 64 KiB) --
    emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                       // compressed length
    emitter.instruction("shl r9, 8");                                           // budget 256x the compressed size
    emitter.instruction(&format!("cmp r9, {}", FILTER_BUF_SIZE));               // is the 256x budget above the minimum?
    emitter.instruction(&format!("jge {}", sized));                             // keep the larger budget
    emitter.instruction(&format!("mov r9, {}", FILTER_BUF_SIZE));               // otherwise use the minimum buffer size
    emitter.label(&sized);
    emitter.instruction("mov QWORD PTR [rsp + 152], r9");                       // save the output buffer capacity
    emitter.instruction("mov rax, r9");                                         // buffer size into the allocator argument
    emitter.instruction("call __rt_heap_alloc");                                // allocate the decompressed-data buffer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("mov QWORD PTR [rsp + 128], rax");                      // save the decompressed buffer pointer

    // -- zero the 112-byte z_stream so zalloc/zfree start NULL --
    emitter.instruction("xor r9, r9");                                          // z_stream byte clear index
    emitter.label(&zero);
    emitter.instruction("cmp r9, 112");                                         // cleared the whole z_stream struct?
    emitter.instruction(&format!("jge {}", zeroed));                            // the struct is fully zeroed
    emitter.instruction("mov BYTE PTR [rsp + r9], 0");                          // zero one z_stream byte
    emitter.instruction("inc r9");                                              // advance the clear index
    emitter.instruction(&format!("jmp {}", zero));                              // continue zeroing the struct
    emitter.label(&zeroed);

    // -- inflateInit2_(strm, -15, version, size): -15 selects raw inflate --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, -15");                                        // arg 1 = windowBits -15: raw inflate
    abi::emit_symbol_address(emitter, "rdx", "_zlib_version");                  // arg 2 = the zlib version string
    emitter.instruction("mov ecx, 112");                                        // arg 3 = sizeof(z_stream) for the ABI check
    emitter.instruction("call inflateInit2_");                                  // initialize a raw-inflate zlib stream

    // -- point the stream at the slurped input and the output buffer --
    abi::emit_symbol_address(emitter, "r9", "_stream_filter_buf");              // scratch base address
    emitter.instruction("mov QWORD PTR [rsp + 0], r9");                         // z_stream.next_in = scratch base
    emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                       // compressed length
    emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                        // z_stream.avail_in = compressed length
    emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                       // decompressed buffer pointer
    emitter.instruction("mov QWORD PTR [rsp + 24], r9");                        // z_stream.next_out = decompressed buffer
    emitter.instruction("mov r9, QWORD PTR [rsp + 152]");                       // output buffer capacity
    emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                       // z_stream.avail_out = output capacity

    // -- inflate the whole input in a single Z_FINISH pass --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, 4");                                          // arg 1 = Z_FINISH
    emitter.instruction("call inflate");                                        // decompress the entire input at once
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // z_stream.total_out = decompressed length
    emitter.instruction("mov QWORD PTR [rsp + 136], rax");                      // save the decompressed length
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("call inflateEnd");                                     // release zlib's internal inflate state

    // -- back the descriptor with an anonymous temp file of the plain bytes --
    emitter.instruction("call __rt_tmpfile");                                   // create an unlinked temp file, rax = fd
    emitter.instruction("mov QWORD PTR [rsp + 144], rax");                      // save the temp-file descriptor

    // -- write loop: copy every decompressed byte into the temp file --
    emitter.instruction("mov QWORD PTR [rsp + 160], 0");                        // write offset = 0
    emitter.label(&write);
    emitter.instruction("mov r10, QWORD PTR [rsp + 136]");                      // total decompressed length
    emitter.instruction("mov r9, QWORD PTR [rsp + 160]");                       // current write offset
    emitter.instruction("cmp r9, r10");                                         // copied every decompressed byte?
    emitter.instruction(&format!("jge {}", write_done));                        // the whole payload is written
    emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                      // temp-file descriptor
    emitter.instruction("mov rsi, QWORD PTR [rsp + 128]");                      // decompressed buffer pointer
    emitter.instruction("add rsi, r9");                                         // write pointer = buffer + offset
    emitter.instruction("mov rdx, r10");                                        // total decompressed length
    emitter.instruction("sub rdx, r9");                                         // remaining bytes to write
    emitter.instruction("call write");                                          // write the plain bytes through libc write()
    emitter.instruction("cmp rax, 0");                                          // did the write make progress?
    emitter.instruction(&format!("jle {}", write_done));                        // stop on a write error
    emitter.instruction("mov r9, QWORD PTR [rsp + 160]");                       // reload the write offset
    emitter.instruction("add r9, rax");                                         // advance by the bytes just written
    emitter.instruction("mov QWORD PTR [rsp + 160], r9");                       // store the updated write offset
    emitter.instruction(&format!("jmp {}", write));                             // continue writing the payload
    emitter.label(&write_done);

    // -- lseek(temp, 0, SEEK_SET): rewind so reads start at the plain bytes --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                      // temp-file descriptor
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // rewind the temp file

    // -- dup2(temp, fd): the descriptor now serves the decompressed bytes --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                      // oldfd = temp file
    emitter.instruction("mov rsi, QWORD PTR [rsp + 112]");                      // newfd = the stream descriptor
    emitter.instruction("call dup2");                                           // redirect the descriptor onto the temp file

    // -- close the now-redundant temp-file descriptor --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                      // the temp-file descriptor
    emitter.instruction("call close");                                          // release the redundant descriptor

    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // resource payload = the descriptor
    emitter.instruction("add rsp, 176");                                        // release the scratch frame
    emitter.instruction("xor esi, esi");                                        // resource mixed payloads have no high word
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}
