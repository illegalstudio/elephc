//! Purpose:
//! Emits PHP `gzuncompress` calls.
//! Decompresses a zlib-compressed string with the system zlib (`uncompress`).
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - The zlib call is emitted inline at the call site so only programs that use
//!   `gzuncompress` carry a `libz` dependency; the checker adds `-lz` for them.
//! - A non-zero zlib status is boxed as PHP `false`; a success as a boxed
//!   string. The decompression buffer is sized at 256x the input (min 64 KiB).
//! - v1 ignores the optional `max_length` argument.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::args::emit_string_arg;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits codegen for PHP `gzuncompress()` string builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gzuncompress()");
    // The compressed argument may arrive as a boxed mixed value (e.g. the
    // string|false returned by file_get_contents), so coerce it to a plain
    // string before handing the pointer/length pair to zlib.
    emit_string_arg(&args[0], emitter, ctx, data);
    let ok = ctx.next_label("gzuncompress_ok");
    let after = ctx.next_label("gzuncompress_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- inline zlib uncompress: x1/x2 = compressed data --
            emitter.instruction("sub sp, sp, #48");                             // scratch frame for the decompression state
            emitter.instruction("str x1, [sp, #0]");                            // save the source pointer
            emitter.instruction("str x2, [sp, #8]");                            // save the source length
            emitter.instruction("lsl x9, x2, #8");                              // budget 256x the compressed size
            emitter.instruction("mov x10, #65536");                             // minimum decompression buffer size
            emitter.instruction("cmp x9, x10");                                 // is the 256x budget larger?
            emitter.instruction("csel x9, x9, x10, gt");                        // pick the larger buffer size
            emitter.instruction("str x9, [sp, #16]");                           // seed destLen with the buffer capacity
            emitter.instruction("mov x0, x9");                                  // buffer size into the allocator argument
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the decompressed-data buffer
            emitter.instruction("mov x9, #1");                                  // heap kind 1 = persisted elephc string
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the buffer as an owned string
            emitter.instruction("str x0, [sp, #24]");                           // save the destination buffer pointer
            emitter.instruction("add x1, sp, #16");                             // &destLen in/out parameter
            emitter.instruction("ldr x2, [sp, #0]");                            // source pointer
            emitter.instruction("ldr x3, [sp, #8]");                            // source length
            emitter.bl_c("uncompress");                                         // zlib-decompress the source
            emitter.instruction(&format!("cbz x0, {}", ok));                    // a zero zlib status means success
            emitter.instruction("mov x1, #0");                                  // a zlib error becomes a null result
            emitter.instruction("mov x2, #0");                                  // no length for the failure case
            emitter.instruction(&format!("b {}", after));                       // skip the success values
            emitter.label(&ok);
            emitter.instruction("ldr x1, [sp, #24]");                           // decompressed buffer becomes the result
            emitter.instruction("ldr x2, [sp, #16]");                           // uncompress wrote the decompressed length
            emitter.label(&after);
            emitter.instruction("add sp, sp, #48");                             // release the scratch frame
        }
        Arch::X86_64 => {
            let sized = ctx.next_label("gzuncompress_sized");
            // -- inline zlib uncompress: rax/rdx = compressed data --
            emitter.instruction("sub rsp, 48");                                 // scratch frame for the decompression state
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the source pointer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the source length
            emitter.instruction("mov r9, rdx");                                 // copy the compressed length
            emitter.instruction("shl r9, 8");                                   // budget 256x the compressed size
            emitter.instruction("cmp r9, 65536");                               // is the 256x budget above the minimum?
            emitter.instruction(&format!("jge {}", sized));                     // keep the larger budget
            emitter.instruction("mov r9, 65536");                               // otherwise use the minimum buffer size
            emitter.label(&sized);
            emitter.instruction("mov QWORD PTR [rsp + 16], r9");                // seed destLen with the buffer capacity
            emitter.instruction("mov rax, r9");                                 // buffer size into the allocator argument
            emitter.instruction("call __rt_heap_alloc");                        // allocate the decompressed-data buffer
            emitter.instruction(&format!(                                       // owned-string heap-kind word with the x86_64 heap marker
                "mov r10, 0x{:x}",
                (X86_64_HEAP_MAGIC_HI32 << 32) | 1
            ));
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the buffer as an owned string
            emitter.instruction("mov QWORD PTR [rsp + 24], rax");               // save the destination buffer pointer
            emitter.instruction("mov rdi, rax");                                // destination buffer pointer
            emitter.instruction("lea rsi, [rsp + 16]");                         // &destLen in/out parameter
            emitter.instruction("mov rdx, QWORD PTR [rsp + 0]");                // source pointer
            emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");                // source length
            emitter.instruction("call uncompress");                             // zlib-decompress the source
            emitter.instruction("test rax, rax");                               // a zero zlib status means success
            emitter.instruction(&format!("jz {}", ok));                         // take the success path
            emitter.instruction("xor eax, eax");                                // a zlib error becomes a null result
            emitter.instruction("xor edx, edx");                                // no length for the failure case
            emitter.instruction(&format!("jmp {}", after));                     // skip the success values
            emitter.label(&ok);
            emitter.instruction("mov rax, QWORD PTR [rsp + 24]");               // decompressed buffer becomes the result
            emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");               // uncompress wrote the decompressed length
            emitter.label(&after);
            emitter.instruction("add rsp, 48");                                 // release the scratch frame
        }
    }
    box_string_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a null pointer becomes PHP `false`, a non-null
/// pointer/length pair becomes a boxed string.
fn box_string_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("gzuncompress_false");
    let done_label = ctx.next_label("gzuncompress_done");

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
