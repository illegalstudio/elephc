//! Purpose:
//! Emits PHP `gzinflate` calls.
//! Decompresses raw DEFLATE data with the system zlib (`inflateInit2_` /
//! `inflate` / `inflateEnd`, windowBits -15).
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Raw DEFLATE is what `gzdeflate` and the `zlib.deflate` stream filter
//!   produce; `gzuncompress` differs by expecting the zlib-wrapped format.
//! - The zlib calls are emitted inline at the call site so only programs that
//!   use `gzinflate` carry a `libz` dependency; the checker adds `-lz`.
//! - A non-`Z_STREAM_END` inflate status is boxed as PHP `false`, success as a
//!   boxed string. The output buffer is sized at 256x the input (min 64 KiB);
//!   the optional `max_length` argument is ignored in v1.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::args::emit_string_arg;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits codegen for PHP `gzinflate()` string builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gzinflate()");
    // The compressed argument may arrive as a boxed mixed value (e.g. the
    // string|false returned by file_get_contents), so coerce it to a plain
    // string before handing the pointer/length pair to zlib.
    emit_string_arg(&args[0], emitter, ctx, data);
    let zero = ctx.next_label("gzinflate_zero");
    let zeroed = ctx.next_label("gzinflate_zeroed");
    let fail = ctx.next_label("gzinflate_fail");
    let done = ctx.next_label("gzinflate_done");
    match emitter.target.arch {
        Arch::AArch64 => emit_arm64(emitter, &zero, &zeroed, &fail, &done),
        Arch::X86_64 => emit_x86_64(emitter, &zero, &zeroed, &fail, &done),
    }
    box_string_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// ARM64: `z_stream` scratch frame holds the 112-byte struct at `[sp, #0]`
/// plus saved values at `[sp, #112..160)`. Leaves a pointer/length result in
/// `x1`/`x2`, or `x1 = 0` on a zlib error.
fn emit_arm64(emitter: &mut Emitter, zero: &str, zeroed: &str, fail: &str, done: &str) {
    // -- reserve the z_stream (112 B) plus scratch slots --
    emitter.instruction("sub sp, sp, #160");                                    // z_stream frame plus saved values
    emitter.instruction("str x1, [sp, #112]");                                  // save the source pointer
    emitter.instruction("str x2, [sp, #120]");                                  // save the source length

    // -- size the output buffer at 256x the input (min 64 KiB) --
    emitter.instruction("lsl x9, x2, #8");                                      // budget 256x the compressed size
    emitter.instruction("mov x10, #65536");                                     // minimum decompression buffer size
    emitter.instruction("cmp x9, x10");                                         // is the 256x budget larger?
    emitter.instruction("csel x9, x9, x10, gt");                                // pick the larger buffer size
    emitter.instruction("str x9, [sp, #144]");                                  // save the output buffer capacity
    emitter.instruction("mov x0, x9");                                          // buffer size into the allocator argument
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the decompressed-data buffer
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

    // -- inflateInit2_(strm, -15, version, size): -15 selects raw inflate --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #-15");                                        // arg 1 = windowBits -15: raw inflate
    abi::emit_symbol_address(emitter, "x2", "_zlib_version");
    emitter.instruction("mov x3, #112");                                        // arg 3 = sizeof(z_stream) for the ABI check
    emitter.bl_c("inflateInit2_");                                              // initialize a raw-inflate zlib stream

    // -- point the stream at the input and output buffers --
    emitter.instruction("ldr x9, [sp, #112]");                                  // reload the source pointer
    emitter.instruction("str x9, [sp, #0]");                                    // z_stream.next_in = source pointer
    emitter.instruction("ldr x9, [sp, #120]");                                  // reload the source length
    emitter.instruction("str w9, [sp, #8]");                                    // z_stream.avail_in = source length
    emitter.instruction("ldr x9, [sp, #128]");                                  // reload the destination buffer pointer
    emitter.instruction("str x9, [sp, #24]");                                   // z_stream.next_out = destination buffer
    emitter.instruction("ldr x9, [sp, #144]");                                  // reload the output buffer capacity
    emitter.instruction("str w9, [sp, #32]");                                   // z_stream.avail_out = output capacity

    // -- inflate the whole input in a single Z_FINISH pass --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #4");                                          // arg 1 = Z_FINISH
    emitter.bl_c("inflate");                                                    // decompress the entire input at once
    emitter.instruction("str x0, [sp, #136]");                                  // save the inflate status code
    emitter.instruction("ldr x2, [sp, #40]");                                   // z_stream.total_out = inflated length
    emitter.instruction("str x2, [sp, #152]");                                  // save the inflated length across inflateEnd

    // -- end the stream --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.bl_c("inflateEnd");                                                 // release zlib's internal inflate state

    // -- success only when inflate reported Z_STREAM_END --
    emitter.instruction("ldr x9, [sp, #136]");                                  // reload the inflate status code
    emitter.instruction("cmp x9, #1");                                          // did inflate reach Z_STREAM_END?
    emitter.instruction(&format!("b.ne {}", fail));                             // a zlib error becomes a false result
    emitter.instruction("ldr x1, [sp, #128]");                                  // decompressed buffer becomes the result
    emitter.instruction("ldr x2, [sp, #152]");                                  // restore the inflated length
    emitter.instruction(&format!("b {}", done));                                // skip the failure values
    emitter.label(fail);
    emitter.instruction("mov x1, #0");                                          // a null pointer marks the zlib error
    emitter.instruction("mov x2, #0");                                          // no length for the failure case
    emitter.label(done);
    emitter.instruction("add sp, sp, #160");                                    // release the z_stream scratch frame
}

/// x86_64: same `z_stream` scratch layout. Leaves a pointer/length result in
/// `rax`/`rdx`, or `rax = 0` on a zlib error.
fn emit_x86_64(emitter: &mut Emitter, zero: &str, zeroed: &str, fail: &str, done: &str) {
    let sized = format!("{}_sized", zero);

    // -- reserve the z_stream (112 B) plus scratch slots --
    emitter.instruction("sub rsp, 160");                                        // z_stream frame plus saved values
    emitter.instruction("mov QWORD PTR [rsp + 112], rax");                      // save the source pointer
    emitter.instruction("mov QWORD PTR [rsp + 120], rdx");                      // save the source length

    // -- size the output buffer at 256x the input (min 64 KiB) --
    emitter.instruction("mov r9, rdx");                                         // copy the compressed length
    emitter.instruction("shl r9, 8");                                           // budget 256x the compressed size
    emitter.instruction("cmp r9, 65536");                                       // is the 256x budget above the minimum?
    emitter.instruction(&format!("jge {}", sized));                             // keep the larger budget
    emitter.instruction("mov r9, 65536");                                       // otherwise use the minimum buffer size
    emitter.label(&sized);
    emitter.instruction("mov QWORD PTR [rsp + 144], r9");                       // save the output buffer capacity
    emitter.instruction("mov rax, r9");                                         // buffer size into the allocator argument
    emitter.instruction("call __rt_heap_alloc");                                // allocate the decompressed-data buffer
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

    // -- inflateInit2_(strm, -15, version, size): -15 selects raw inflate --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, -15");                                        // arg 1 = windowBits -15: raw inflate
    abi::emit_symbol_address(emitter, "rdx", "_zlib_version");                  // arg 2 = the zlib version string
    emitter.instruction("mov ecx, 112");                                        // arg 3 = sizeof(z_stream) for the ABI check
    emitter.instruction("call inflateInit2_");                                  // initialize a raw-inflate zlib stream

    // -- point the stream at the input and output buffers --
    emitter.instruction("mov r9, QWORD PTR [rsp + 112]");                       // reload the source pointer
    emitter.instruction("mov QWORD PTR [rsp + 0], r9");                         // z_stream.next_in = source pointer
    emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                       // reload the source length
    emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                        // z_stream.avail_in = source length
    emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                       // reload the destination buffer pointer
    emitter.instruction("mov QWORD PTR [rsp + 24], r9");                        // z_stream.next_out = destination buffer
    emitter.instruction("mov r9, QWORD PTR [rsp + 144]");                       // reload the output buffer capacity
    emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                       // z_stream.avail_out = output capacity

    // -- inflate the whole input in a single Z_FINISH pass --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, 4");                                          // arg 1 = Z_FINISH
    emitter.instruction("call inflate");                                        // decompress the entire input at once
    emitter.instruction("mov QWORD PTR [rsp + 136], rax");                      // save the inflate status code
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // z_stream.total_out = inflated length
    emitter.instruction("mov QWORD PTR [rsp + 152], rax");                      // save the inflated length across inflateEnd

    // -- end the stream --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("call inflateEnd");                                     // release zlib's internal inflate state

    // -- success only when inflate reported Z_STREAM_END --
    emitter.instruction("cmp QWORD PTR [rsp + 136], 1");                        // did inflate reach Z_STREAM_END?
    emitter.instruction(&format!("jne {}", fail));                              // a zlib error becomes a false result
    emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                      // decompressed buffer becomes the result
    emitter.instruction("mov rdx, QWORD PTR [rsp + 152]");                      // restore the inflated length
    emitter.instruction(&format!("jmp {}", done));                              // skip the failure values
    emitter.label(fail);
    emitter.instruction("xor eax, eax");                                        // a null pointer marks the zlib error
    emitter.instruction("xor edx, edx");                                        // no length for the failure case
    emitter.label(done);
    emitter.instruction("add rsp, 160");                                        // release the z_stream scratch frame
}

/// Boxes the inflate result: a null pointer becomes PHP `false`, a non-null
/// pointer/length pair becomes a boxed string.
fn box_string_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("gzinflate_false");
    let done_label = ctx.next_label("gzinflate_boxed");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null pointer means a zlib error
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the string payload across the allocation
            emitter.instruction("mov x0, #24");                                 // mixed cells store a tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation as a mixed cell
            emitter.instruction("mov x9, #1");                                  // runtime tag 1 = string
            emitter.instruction("str x9, [x0]");                                // store the string tag
            abi::emit_pop_reg_pair(emitter, "x10", "x11"); // reload the string pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store the string payload words
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after a valid result
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // a null pointer means a zlib error
            emitter.instruction(&format!("jz {}", false_label));                // box false on a zlib error
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the string payload across the allocation
            emitter.instruction("mov rax, 24");                                 // mixed cells store a tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction(&format!(                                       // mixed-cell heap-kind word with the x86_64 heap marker
                "mov r10, 0x{:x}",
                (X86_64_HEAP_MAGIC_HI32 << 32) | 5
            ));
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as a mixed cell
            emitter.instruction("mov r10, 1");                                  // runtime tag 1 = string
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the string tag
            abi::emit_pop_reg_pair(emitter, "r10", "r11"); // reload the string pointer and length
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the string pointer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // store the string length
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after a valid result
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
