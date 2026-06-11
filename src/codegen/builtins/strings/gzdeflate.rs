//! Purpose:
//! Emits PHP `gzdeflate` calls.
//! Compresses a string into raw DEFLATE data with the system zlib
//! (`deflateInit2_` / `deflate` / `deflateEnd`, windowBits -15).
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Raw DEFLATE (no zlib header/trailer) is what `gzinflate` and the
//!   `zlib.deflate` stream filter consume; `gzcompress` differs by emitting the
//!   zlib-wrapped format.
//! - The zlib calls are emitted inline at the call site (not as a shared
//!   runtime helper) so only programs that use `gzdeflate` carry a `libz`
//!   dependency; the checker adds `-lz` for them.
//! - The transient `z_stream` (112 bytes LP64) lives in a stack scratch frame;
//!   the result is an owned heap string (heap kind 1) sized by `compressBound`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::args::emit_string_arg;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits codegen for PHP `gzdeflate()` string builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gzdeflate()");
    // The data argument may arrive as a boxed mixed value, so coerce it to a
    // plain string before handing the pointer/length pair to zlib.
    emit_string_arg(&args[0], emitter, ctx, data);
    let zero = ctx.next_label("gzdeflate_zero");
    let zeroed = ctx.next_label("gzdeflate_zeroed");
    match emitter.target.arch {
        Arch::AArch64 => emit_arm64(args, emitter, ctx, data, &zero, &zeroed),
        Arch::X86_64 => emit_x86_64(args, emitter, ctx, data, &zero, &zeroed),
    }
    Some(PhpType::Str)
}

/// ARM64: `z_stream` scratch frame holds the 112-byte struct at `[sp, #0]`
/// plus saved values at `[sp, #112..160)`.
fn emit_arm64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    zero: &str,
    zeroed: &str,
) {
    abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the source string
    if args.len() >= 2 {
        emit_expr(&args[1], emitter, ctx, data);
    } else {
        emitter.instruction("mov x0, #-1");                                     // default zlib compression level
    }
    abi::emit_pop_reg_pair(emitter, "x1", "x2"); // restore the source pointer/length

    // -- reserve the z_stream (112 B) plus scratch slots --
    emitter.instruction("sub sp, sp, #160");                                    // z_stream frame plus saved values
    emitter.instruction("str x0, [sp, #136]");                                  // save the compression level
    emitter.instruction("str x1, [sp, #112]");                                  // save the source pointer
    emitter.instruction("str x2, [sp, #120]");                                  // save the source length

    // -- size and allocate the output buffer --
    emitter.instruction("mov x0, x2");                                          // source length into the compressBound argument
    emitter.bl_c("compressBound");                                              // x0 = worst-case compressed size
    emitter.instruction("str x0, [sp, #144]");                                  // save the output buffer capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the compressed-data buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("str x0, [sp, #128]");                                  // save the destination buffer pointer

    // -- zero the 112-byte z_stream so zalloc/zfree start NULL --
    emitter.instruction("mov x9, #0");                                          // z_stream byte clear index
    emitter.label(zero);
    emitter.instruction("cmp x9, #112");                                        // cleared the whole z_stream struct?
    emitter.instruction(&format!("b.ge {}", zeroed));                           // the struct is fully zeroed
    emitter.instruction("strb wzr, [sp, x9]");                                  // zero one z_stream byte
    emitter.instruction("add x9, x9, #1");                                      // advance the clear index
    emitter.instruction(&format!("b {}", zero));                                // continue zeroing the struct
    emitter.label(zeroed);

    // -- deflateInit2_(strm, level, Z_DEFLATED, -15, memLevel, strategy, ...) --
    // windowBits -15 selects raw deflate with no zlib header or trailer.
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("ldr x1, [sp, #136]");                                  // arg 1 = compression level
    emitter.instruction("mov x2, #8");                                          // arg 2 = Z_DEFLATED method
    emitter.instruction("mov x3, #-15");                                        // arg 3 = windowBits -15: raw deflate
    emitter.instruction("mov x4, #8");                                          // arg 4 = default memLevel
    emitter.instruction("mov x5, #0");                                          // arg 5 = Z_DEFAULT_STRATEGY
    abi::emit_symbol_address(emitter, "x6", "_zlib_version");
    emitter.instruction("mov x7, #112");                                        // arg 7 = sizeof(z_stream) for the ABI check
    emitter.bl_c("deflateInit2_");                                              // initialize a raw-deflate zlib stream

    // -- point the stream at the input and output buffers --
    emitter.instruction("ldr x9, [sp, #112]");                                  // reload the source pointer
    emitter.instruction("str x9, [sp, #0]");                                    // z_stream.next_in = source pointer
    emitter.instruction("ldr x9, [sp, #120]");                                  // reload the source length
    emitter.instruction("str w9, [sp, #8]");                                    // z_stream.avail_in = source length
    emitter.instruction("ldr x9, [sp, #128]");                                  // reload the destination buffer pointer
    emitter.instruction("str x9, [sp, #24]");                                   // z_stream.next_out = destination buffer
    emitter.instruction("ldr x9, [sp, #144]");                                  // reload the output buffer capacity
    emitter.instruction("str w9, [sp, #32]");                                   // z_stream.avail_out = output capacity

    // -- deflate the whole input in a single Z_FINISH pass --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #4");                                          // arg 1 = Z_FINISH
    emitter.bl_c("deflate");                                                    // compress the entire input at once

    // -- end the stream and return the compressed buffer --
    emitter.instruction("ldr x2, [sp, #40]");                                   // z_stream.total_out = compressed length
    emitter.instruction("str x2, [sp, #152]");                                  // save the compressed length across deflateEnd
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.bl_c("deflateEnd");                                                 // release zlib's internal deflate state
    emitter.instruction("ldr x1, [sp, #128]");                                  // compressed buffer becomes the result pointer
    emitter.instruction("ldr x2, [sp, #152]");                                  // restore the compressed length
    emitter.instruction("add sp, sp, #160");                                    // release the z_stream scratch frame
}

/// x86_64: same `z_stream` scratch layout; `deflateInit2_` takes its 7th and
/// 8th arguments on the stack.
fn emit_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    zero: &str,
    zeroed: &str,
) {
    abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the source string
    if args.len() >= 2 {
        emit_expr(&args[1], emitter, ctx, data);
    } else {
        emitter.instruction("mov eax, -1");                                     // default zlib compression level
    }
    emitter.instruction("mov rdi, rax");                                        // hold the compression level in a scratch register
    abi::emit_pop_reg_pair(emitter, "rsi", "rdx"); // restore the data pointer/length

    // -- reserve the z_stream (112 B) plus scratch slots --
    emitter.instruction("sub rsp, 160");                                        // z_stream frame plus saved values
    emitter.instruction("mov QWORD PTR [rsp + 136], rdi");                      // save the compression level
    emitter.instruction("mov QWORD PTR [rsp + 112], rsi");                      // save the source pointer
    emitter.instruction("mov QWORD PTR [rsp + 120], rdx");                      // save the source length

    // -- size and allocate the output buffer --
    emitter.instruction("mov rdi, rdx");                                        // source length into the compressBound argument
    emitter.instruction("call compressBound");                                  // rax = worst-case compressed size
    emitter.instruction("mov QWORD PTR [rsp + 144], rax");                      // save the output buffer capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate the compressed-data buffer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("mov QWORD PTR [rsp + 128], rax");                      // save the destination buffer pointer

    // -- zero the 112-byte z_stream so zalloc/zfree start NULL --
    emitter.instruction("xor r9, r9");                                          // z_stream byte clear index
    emitter.label(zero);
    emitter.instruction("cmp r9, 112");                                         // cleared the whole z_stream struct?
    emitter.instruction(&format!("jge {}", zeroed));                            // the struct is fully zeroed
    emitter.instruction("mov BYTE PTR [rsp + r9], 0");                          // zero one z_stream byte
    emitter.instruction("inc r9");                                              // advance the clear index
    emitter.instruction(&format!("jmp {}", zero));                              // continue zeroing the struct
    emitter.label(zeroed);

    // -- deflateInit2_(strm, level, Z_DEFLATED, -15, memLevel, strategy, ...) --
    // windowBits -15 selects raw deflate; args 7-8 are passed on the stack.
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 136]");                      // arg 1 = compression level
    emitter.instruction("mov edx, 8");                                          // arg 2 = Z_DEFLATED method
    emitter.instruction("mov ecx, -15");                                        // arg 3 = windowBits -15: raw deflate
    emitter.instruction("mov r8d, 8");                                          // arg 4 = default memLevel
    emitter.instruction("xor r9d, r9d");                                        // arg 5 = Z_DEFAULT_STRATEGY
    emitter.instruction("sub rsp, 16");                                         // reserve the two stack arguments
    abi::emit_symbol_address(emitter, "rax", "_zlib_version");                  // the zlib version string
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // stack arg 6 = version
    emitter.instruction("mov QWORD PTR [rsp + 8], 112");                        // stack arg 7 = sizeof(z_stream)
    emitter.instruction("call deflateInit2_");                                  // initialize a raw-deflate zlib stream
    emitter.instruction("add rsp, 16");                                         // release the stack-argument space

    // -- point the stream at the input and output buffers --
    emitter.instruction("mov r9, QWORD PTR [rsp + 112]");                       // reload the source pointer
    emitter.instruction("mov QWORD PTR [rsp + 0], r9");                         // z_stream.next_in = source pointer
    emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                       // reload the source length
    emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                        // z_stream.avail_in = source length
    emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                       // reload the destination buffer pointer
    emitter.instruction("mov QWORD PTR [rsp + 24], r9");                        // z_stream.next_out = destination buffer
    emitter.instruction("mov r9, QWORD PTR [rsp + 144]");                       // reload the output buffer capacity
    emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                       // z_stream.avail_out = output capacity

    // -- deflate the whole input in a single Z_FINISH pass --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, 4");                                          // arg 1 = Z_FINISH
    emitter.instruction("call deflate");                                        // compress the entire input at once

    // -- end the stream and return the compressed buffer --
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // z_stream.total_out = compressed length
    emitter.instruction("mov QWORD PTR [rsp + 152], rax");                      // save the compressed length across deflateEnd
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("call deflateEnd");                                     // release zlib's internal deflate state
    emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                      // compressed buffer becomes the result pointer
    emitter.instruction("mov rdx, QWORD PTR [rsp + 152]");                      // restore the compressed length
    emitter.instruction("add rsp, 160");                                        // release the z_stream scratch frame
}
