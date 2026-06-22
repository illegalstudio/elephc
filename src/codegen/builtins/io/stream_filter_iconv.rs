//! Purpose:
//! Emits the `convert.iconv.<from>/<to>` charset-conversion stream filter for
//! `stream_filter_append` / `stream_filter_prepend`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::stream_filter::emit_attach()` when the
//!   filter-name literal begins with `convert.iconv.`.
//!
//! Key details:
//! - Like the `zlib.inflate` read filter, the descriptor itself is transformed
//!   at attach time: the whole stream is slurped, transcoded once through libc
//!   `iconv`, written to an anonymous temp file, and `dup2`'d onto the original
//!   descriptor. Every later `fread`/`fseek`/`feof` then works unchanged — no
//!   per-fd filter state and no `__rt_fread` change are needed.
//! - `iconv_open`/`iconv`/`iconv_close` live in libc (glibc, macOS libSystem,
//!   musl), so no extra `-l` and no function-pointer indirection are needed.
//! - The `<from>` and `<to>` charset names are parsed from the filter name at
//!   compile time and emitted as null-terminated C strings.
//! - v1 limitations: the conversion direction is applied to the descriptor (so
//!   it behaves as a read transform); input is capped at the 64 KiB
//!   `_stream_filter_buf` scratch and output at 4x that (min 64 KiB). A bad
//!   charset pair (`iconv_open` fails) leaves the stream unconverted. musl's
//!   iconv supports a limited charset set (UTF-8/UTF-16/UTF-32 are fine).

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Capacity of the shared `_stream_filter_buf` scratch, reused as the input slurp buffer.
const FILTER_BUF_SIZE: i64 = 65536;
/// x86_64 owned-heap kind word: the elephc heap marker in the high 32 bits.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `convert.iconv.<from>/<to>` filter attachment. `spec` is the
/// portion after `convert.iconv.` — e.g. `UTF-8/UTF-16LE`. Returns the stream
/// re-boxed as a resource, matching `stream_filter_append`'s contract; a
/// malformed spec (no `/`) attaches no conversion.
pub fn emit(
    spec: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_append(convert.iconv.*)");
    emit_stream_fd_arg("stream_filter_append", &args[0], emitter, ctx, data);

    // Parse "<from>/<to>"; a missing slash means no usable charset pair.
    let charsets = spec.split_once('/');
    let (from, to) = match charsets {
        Some((f, t)) if !f.is_empty() && !t.is_empty() => (f, t),
        _ => {
            // No conversion: just re-box the descriptor as a resource.
            return Some(rebox_fd_as_resource(emitter));
        }
    };
    // Emit null-terminated C strings for iconv_open(tocode, fromcode).
    let (from_sym, _) = data.add_string(format!("{}\0", from).as_bytes());
    let (to_sym, _) = data.add_string(format!("{}\0", to).as_bytes());

    // The descriptor is in the int result register. Evaluate the read/write mode
    // (args[2], default STREAM_FILTER_ALL = 3) and dispatch at runtime: a
    // WRITE-only filter (mode == 2) installs a streaming per-fwrite transcoder;
    // READ and ALL (the common no-arg case) keep the attach-time read transform,
    // preserving all existing behavior.
    let write_label = ctx.next_label("iconv_mode_write");
    let after_label = ctx.next_label("iconv_mode_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the descriptor across the mode evaluation
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);                        // evaluate the read/write mode into x0
            } else {
                emitter.instruction("mov x0, #3");                              // default mode = STREAM_FILTER_ALL
            }
            emitter.instruction("mov x9, x0");                                  // hold the mode while the descriptor is restored
            emitter.instruction("ldr x0, [sp], #16");                           // restore the descriptor into the result register
            emitter.instruction("cmp x9, #2");                                  // is this a STREAM_FILTER_WRITE-only filter?
            emitter.instruction(&format!("b.eq {}", write_label));              // install the streaming write transcoder
            emit_read_arm64(emitter, &from_sym, &to_sym, |prefix| ctx.next_label(prefix)); // READ / ALL: attach-time read transform
            emitter.instruction(&format!("b {}", after_label));                 // skip the write-attach path
            emitter.label(&write_label);
            super::stream_filter_iconv_write::emit_iconv_write_attach(emitter, ctx, &from_sym, &to_sym);
            emitter.label(&after_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the descriptor across the mode evaluation
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);                        // evaluate the read/write mode into rax
            } else {
                emitter.instruction("mov eax, 3");                              // default mode = STREAM_FILTER_ALL
            }
            emitter.instruction("mov r9, rax");                                 // hold the mode while the descriptor is restored
            abi::emit_pop_reg(emitter, "rax");                                  // restore the descriptor into the result register
            emitter.instruction("cmp r9, 2");                                   // is this a STREAM_FILTER_WRITE-only filter?
            emitter.instruction(&format!("je {}", write_label));                // install the streaming write transcoder
            emit_read_x86_64(emitter, &from_sym, &to_sym, |prefix| ctx.next_label(prefix)); // READ / ALL: attach-time read transform
            emitter.instruction(&format!("jmp {}", after_label));               // skip the write-attach path
            emitter.label(&write_label);
            super::stream_filter_iconv_write::emit_iconv_write_attach(emitter, ctx, &from_sym, &to_sym);
            emitter.label(&after_label);
        }
    }
    Some(PhpType::Mixed)
}

/// Re-boxes the descriptor (currently in the int result register) as a resource
/// Mixed cell, the value `stream_filter_append` returns. Returns `PhpType::Mixed`.
fn rebox_fd_as_resource(emitter: &mut Emitter) -> PhpType {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // resource payload = the descriptor
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads have no high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // re-box the stream as the filter resource
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // resource payload = the descriptor
            emitter.instruction("xor esi, esi");                                // resource mixed payloads have no high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // re-box the stream as the filter resource
        }
    }
    PhpType::Mixed
}

/// ARM64: 160-byte scratch frame.
/// [0]=fd [8]=input len [16]=out buf [24]=out cap [32]=temp fd [40]=iconv cd
/// [48]=iconv inbuf [56]=iconv inbytesleft [64]=iconv outbuf [72]=iconv outbytesleft
/// [80]=converted len [88]=write offset; x29/x30 at [144].
pub(crate) fn emit_read_arm64<F>(
    emitter: &mut Emitter,
    from_sym: &str,
    to_sym: &str,
    mut next_label: F,
)
where
    F: FnMut(&str) -> String,
{
    let slurp = next_label("iconv_slurp");
    let slurp_done = next_label("iconv_slurped");
    let sized = next_label("iconv_sized");
    let skip = next_label("iconv_skip");
    let write = next_label("iconv_write");
    let write_done = next_label("iconv_written");

    emitter.instruction("sub sp, sp, #160");                                    // iconv scratch frame
    emitter.instruction("stp x29, x30, [sp, #144]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #144");                                   // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source file descriptor

    // -- slurp every byte from the descriptor into the scratch buffer --
    emitter.instruction("str xzr, [sp, #8]");                                   // input length = 0
    emitter.label(&slurp);
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd to read from
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("ldr x9, [sp, #8]");                                    // current input length
    emitter.instruction("add x1, x1, x9");                                      // write pointer = scratch base + length
    emitter.instruction(&format!("mov x2, #{}", FILTER_BUF_SIZE));              // scratch capacity
    emitter.instruction("sub x2, x2, x9");                                      // remaining scratch capacity
    emitter.syscall(3);
    emitter.instruction("cmp x0, #0");                                          // did the read hit EOF or fail?
    emitter.instruction(&format!("b.le {}", slurp_done));                       // stop slurping at EOF or on error
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the input length
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just read
    emitter.instruction("str x9, [sp, #8]");                                    // store the updated input length
    emitter.instruction(&format!("mov x10, #{}", FILTER_BUF_SIZE));             // scratch capacity
    emitter.instruction("cmp x9, x10");                                         // is the scratch buffer full?
    emitter.instruction(&format!("b.lt {}", slurp));                            // room remains: keep slurping
    emitter.label(&slurp_done);

    // -- size and allocate the output buffer (4x input, min 64 KiB) --
    emitter.instruction("ldr x9, [sp, #8]");                                    // input length
    emitter.instruction("lsl x9, x9, #2");                                      // budget 4x the input size
    emitter.instruction(&format!("mov x10, #{}", FILTER_BUF_SIZE));             // minimum output buffer size
    emitter.instruction("cmp x9, x10");                                         // is the 4x budget larger?
    emitter.instruction(&format!("b.gt {}", sized));                            // keep the larger budget
    emitter.instruction(&format!("mov x9, #{}", FILTER_BUF_SIZE));              // otherwise use the minimum size
    emitter.label(&sized);
    emitter.instruction("str x9, [sp, #24]");                                   // save the output buffer capacity
    emitter.instruction("mov x0, x9");                                          // buffer size into the allocator argument
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the converted-data buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer header
    emitter.instruction("str x0, [sp, #16]");                                   // save the output buffer pointer

    // -- iconv_open(tocode, fromcode): a -1 result leaves the stream unconverted --
    abi::emit_symbol_address(emitter, "x0", to_sym);
    abi::emit_symbol_address(emitter, "x1", from_sym);
    emitter.bl_c("iconv_open");                                                 // open the charset conversion descriptor
    emitter.instruction("cmn x0, #1");                                          // is the descriptor (iconv_t)-1?
    emitter.instruction(&format!("b.eq {}", skip));                             // iconv_open failed → skip the conversion
    emitter.instruction("str x0, [sp, #40]");                                   // save the iconv conversion descriptor

    // -- set up the iconv in/out cursors and remaining-byte counts --
    abi::emit_symbol_address(emitter, "x9", "_stream_filter_buf");
    emitter.instruction("str x9, [sp, #48]");                                   // iconv inbuf = scratch base
    emitter.instruction("ldr x9, [sp, #8]");                                    // input length
    emitter.instruction("str x9, [sp, #56]");                                   // iconv inbytesleft = input length
    emitter.instruction("ldr x9, [sp, #16]");                                   // output buffer pointer
    emitter.instruction("str x9, [sp, #64]");                                   // iconv outbuf = output buffer
    emitter.instruction("ldr x9, [sp, #24]");                                   // output buffer capacity
    emitter.instruction("str x9, [sp, #72]");                                   // iconv outbytesleft = output capacity

    // -- iconv(cd, &inbuf, &inbytesleft, &outbuf, &outbytesleft) --
    emitter.instruction("ldr x0, [sp, #40]");                                   // conversion descriptor
    emitter.instruction("add x1, sp, #48");                                     // &inbuf
    emitter.instruction("add x2, sp, #56");                                     // &inbytesleft
    emitter.instruction("add x3, sp, #64");                                     // &outbuf
    emitter.instruction("add x4, sp, #72");                                     // &outbytesleft
    emitter.bl_c("iconv");                                                      // transcode the whole input in one pass
    emitter.instruction("ldr x9, [sp, #24]");                                   // output capacity
    emitter.instruction("ldr x10, [sp, #72]");                                  // bytes still free in the output buffer
    emitter.instruction("sub x9, x9, x10");                                     // converted length = capacity - free
    emitter.instruction("str x9, [sp, #80]");                                   // save the converted length
    emitter.instruction("ldr x0, [sp, #40]");                                   // conversion descriptor
    emitter.bl_c("iconv_close");                                                // release the iconv descriptor

    // -- back the descriptor with an anonymous temp file of the converted bytes --
    emitter.instruction("bl __rt_tmpfile");                                     // create an unlinked temp file, x0 = fd
    emitter.instruction("str x0, [sp, #32]");                                   // save the temp-file descriptor

    // -- write loop: copy every converted byte into the temp file --
    emitter.instruction("str xzr, [sp, #88]");                                  // write offset = 0
    emitter.label(&write);
    emitter.instruction("ldr x10, [sp, #80]");                                  // total converted length
    emitter.instruction("ldr x9, [sp, #88]");                                   // current write offset
    emitter.instruction("cmp x9, x10");                                         // copied every converted byte?
    emitter.instruction(&format!("b.ge {}", write_done));                       // the whole payload is written
    emitter.instruction("ldr x0, [sp, #32]");                                   // temp-file descriptor
    emitter.instruction("ldr x1, [sp, #16]");                                   // output buffer pointer
    emitter.instruction("add x1, x1, x9");                                      // write pointer = buffer + offset
    emitter.instruction("sub x2, x10, x9");                                     // remaining bytes to write
    emitter.syscall(4);
    emitter.instruction("cmp x0, #0");                                          // did the write make progress?
    emitter.instruction(&format!("b.le {}", write_done));                       // stop on a write error
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload the write offset
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just written
    emitter.instruction("str x9, [sp, #88]");                                   // store the updated write offset
    emitter.instruction(&format!("b {}", write));                               // continue writing the payload
    emitter.label(&write_done);

    // -- lseek(temp, 0, SEEK_SET): rewind so reads start at the converted bytes --
    emitter.instruction("ldr x0, [sp, #32]");                                   // temp-file descriptor
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET
    emitter.syscall(199);

    // -- dup2(temp, fd): the descriptor now serves the converted bytes --
    emitter.instruction("ldr x0, [sp, #32]");                                   // oldfd = temp file
    emitter.instruction("ldr x1, [sp, #0]");                                    // newfd = the stream descriptor
    emitter.bl_c("dup2");                                                       // redirect the descriptor onto the temp file

    // -- close the now-redundant temp-file descriptor --
    emitter.instruction("ldr x0, [sp, #32]");                                   // the temp-file descriptor
    emitter.syscall(6);

    emitter.label(&skip);
    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the stream descriptor
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // release the scratch frame
    emitter.instruction("mov x1, x0");                                          // resource payload = the descriptor
    emitter.instruction("mov x2, #0");                                          // resource mixed payloads have no high word
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}

/// x86_64: same 160-byte sp-relative scratch layout as the AArch64 path.
pub(crate) fn emit_read_x86_64<F>(
    emitter: &mut Emitter,
    from_sym: &str,
    to_sym: &str,
    mut next_label: F,
)
where
    F: FnMut(&str) -> String,
{
    let slurp = next_label("iconv_slurp");
    let slurp_done = next_label("iconv_slurped");
    let sized = next_label("iconv_sized");
    let skip = next_label("iconv_skip");
    let write = next_label("iconv_write");
    let write_done = next_label("iconv_written");

    emitter.instruction("sub rsp, 160");                                        // iconv scratch frame
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // save the source file descriptor

    // -- slurp every byte from the descriptor into the scratch buffer --
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // input length = 0
    emitter.label(&slurp);
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // fd to read from
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf");             // scratch base address
    emitter.instruction("add rsi, QWORD PTR [rsp + 8]");                        // write pointer = scratch base + length
    emitter.instruction(&format!("mov rdx, {}", FILTER_BUF_SIZE));              // scratch capacity
    emitter.instruction("sub rdx, QWORD PTR [rsp + 8]");                        // remaining scratch capacity
    emitter.instruction("call read");                                           // read bytes through libc read()
    emitter.instruction("cmp rax, 0");                                          // did the read hit EOF or fail?
    emitter.instruction(&format!("jle {}", slurp_done));                        // stop slurping at EOF or on error
    emitter.instruction("mov r9, QWORD PTR [rsp + 8]");                         // reload the input length
    emitter.instruction("add r9, rax");                                         // advance by the bytes just read
    emitter.instruction("mov QWORD PTR [rsp + 8], r9");                         // store the updated input length
    emitter.instruction(&format!("cmp r9, {}", FILTER_BUF_SIZE));               // is the scratch buffer full?
    emitter.instruction(&format!("jl {}", slurp));                              // room remains: keep slurping
    emitter.label(&slurp_done);

    // -- size and allocate the output buffer (4x input, min 64 KiB) --
    emitter.instruction("mov r9, QWORD PTR [rsp + 8]");                         // input length
    emitter.instruction("shl r9, 2");                                           // budget 4x the input size
    emitter.instruction(&format!("cmp r9, {}", FILTER_BUF_SIZE));               // is the 4x budget above the minimum?
    emitter.instruction(&format!("jge {}", sized));                             // keep the larger budget
    emitter.instruction(&format!("mov r9, {}", FILTER_BUF_SIZE));               // otherwise use the minimum size
    emitter.label(&sized);
    emitter.instruction("mov QWORD PTR [rsp + 24], r9");                        // save the output buffer capacity
    emitter.instruction("mov rax, r9");                                         // buffer size into the allocator argument
    emitter.instruction("call __rt_heap_alloc");                                // allocate the converted-data buffer
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); //owned-string heap-kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer header
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // save the output buffer pointer

    // -- iconv_open(tocode, fromcode): a -1 result leaves the stream unconverted --
    abi::emit_symbol_address(emitter, "rdi", &to_sym);                          // arg 0 = tocode
    abi::emit_symbol_address(emitter, "rsi", &from_sym);                        // arg 1 = fromcode
    emitter.instruction("call iconv_open");                                     // open the charset conversion descriptor
    emitter.instruction("cmp rax, -1");                                         // is the descriptor (iconv_t)-1?
    emitter.instruction(&format!("je {}", skip));                               // iconv_open failed → skip the conversion
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // save the iconv conversion descriptor

    // -- set up the iconv in/out cursors and remaining-byte counts --
    abi::emit_symbol_address(emitter, "r9", "_stream_filter_buf");              // scratch base address
    emitter.instruction("mov QWORD PTR [rsp + 48], r9");                        // iconv inbuf = scratch base
    emitter.instruction("mov r9, QWORD PTR [rsp + 8]");                         // input length
    emitter.instruction("mov QWORD PTR [rsp + 56], r9");                        // iconv inbytesleft = input length
    emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                        // output buffer pointer
    emitter.instruction("mov QWORD PTR [rsp + 64], r9");                        // iconv outbuf = output buffer
    emitter.instruction("mov r9, QWORD PTR [rsp + 24]");                        // output buffer capacity
    emitter.instruction("mov QWORD PTR [rsp + 72], r9");                        // iconv outbytesleft = output capacity

    // -- iconv(cd, &inbuf, &inbytesleft, &outbuf, &outbytesleft) --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 40]");                       // conversion descriptor
    emitter.instruction("lea rsi, [rsp + 48]");                                 // &inbuf
    emitter.instruction("lea rdx, [rsp + 56]");                                 // &inbytesleft
    emitter.instruction("lea rcx, [rsp + 64]");                                 // &outbuf
    emitter.instruction("lea r8, [rsp + 72]");                                  // &outbytesleft
    emitter.instruction("call iconv");                                          // transcode the whole input in one pass
    emitter.instruction("mov r9, QWORD PTR [rsp + 24]");                        // output capacity
    emitter.instruction("sub r9, QWORD PTR [rsp + 72]");                        // converted length = capacity - free
    emitter.instruction("mov QWORD PTR [rsp + 80], r9");                        // save the converted length
    emitter.instruction("mov rdi, QWORD PTR [rsp + 40]");                       // conversion descriptor
    emitter.instruction("call iconv_close");                                    // release the iconv descriptor

    // -- back the descriptor with an anonymous temp file of the converted bytes --
    emitter.instruction("call __rt_tmpfile");                                   // create an unlinked temp file, rax = fd
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // save the temp-file descriptor

    // -- write loop: copy every converted byte into the temp file --
    emitter.instruction("mov QWORD PTR [rsp + 88], 0");                         // write offset = 0
    emitter.label(&write);
    emitter.instruction("mov r10, QWORD PTR [rsp + 80]");                       // total converted length
    emitter.instruction("mov r9, QWORD PTR [rsp + 88]");                        // current write offset
    emitter.instruction("cmp r9, r10");                                         // copied every converted byte?
    emitter.instruction(&format!("jge {}", write_done));                        // the whole payload is written
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // temp-file descriptor
    emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");                       // output buffer pointer
    emitter.instruction("add rsi, r9");                                         // write pointer = buffer + offset
    emitter.instruction("mov rdx, r10");                                        // total converted length
    emitter.instruction("sub rdx, r9");                                         // remaining bytes to write
    emitter.instruction("call write");                                          // write the converted bytes via libc write()
    emitter.instruction("cmp rax, 0");                                          // did the write make progress?
    emitter.instruction(&format!("jle {}", write_done));                        // stop on a write error
    emitter.instruction("mov r9, QWORD PTR [rsp + 88]");                        // reload the write offset
    emitter.instruction("add r9, rax");                                         // advance by the bytes just written
    emitter.instruction("mov QWORD PTR [rsp + 88], r9");                        // store the updated write offset
    emitter.instruction(&format!("jmp {}", write));                             // continue writing the payload
    emitter.label(&write_done);

    // -- lseek(temp, 0, SEEK_SET): rewind so reads start at the converted bytes --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // temp-file descriptor
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // rewind the temp file

    // -- dup2(temp, fd): the descriptor now serves the converted bytes --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // oldfd = temp file
    emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                        // newfd = the stream descriptor
    emitter.instruction("call dup2");                                           // redirect the descriptor onto the temp file

    // -- close the now-redundant temp-file descriptor --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // the temp-file descriptor
    emitter.instruction("call close");                                          // release the redundant descriptor

    emitter.label(&skip);
    // -- re-box the descriptor as a resource, matching stream_filter_append --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // reload the stream descriptor
    emitter.instruction("add rsp, 160");                                        // release the scratch frame
    emitter.instruction("xor esi, esi");                                        // resource mixed payloads have no high word
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // re-box the stream as the filter resource
}
