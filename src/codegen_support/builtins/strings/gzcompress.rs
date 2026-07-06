//! Purpose:
//! Emits PHP `gzcompress` calls.
//! Compresses a string with the system zlib (`compressBound` + `compress2`).
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - The zlib calls are emitted inline at the call site (not as a shared
//!   runtime helper) so that only programs that actually use `gzcompress`
//!   carry a dependency on `libz`. The required-library declaration in the
//!   checker adds `-lz` to the link for those programs.
//! - The result is an owned heap string (heap kind 1) sized by `compressBound`.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::args::emit_string_arg;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits codegen for PHP `gzcompress()` string builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gzcompress()");
    // The data argument may arrive as a boxed mixed value, so coerce it to a
    // plain string before handing the pointer/length pair to zlib.
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the data string
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
            } else {
                emitter.instruction("mov x0, #-1");                             // default zlib compression level
            }
            abi::emit_pop_reg_pair(emitter, "x1", "x2"); // restore the data pointer/length
            // -- inline zlib compress: x0 = level, x1/x2 = data --
            emitter.instruction("sub sp, sp, #64");                             // scratch frame for the compression state
            emitter.instruction("str x0, [sp, #0]");                            // save the compression level
            emitter.instruction("str x1, [sp, #8]");                            // save the source pointer
            emitter.instruction("str x2, [sp, #16]");                           // save the source length
            emitter.instruction("mov x0, x2");                                  // source length into the compressBound argument
            emitter.bl_c("compressBound");                                      // x0 = worst-case compressed size
            emitter.instruction("str x0, [sp, #24]");                           // seed destLen with the buffer capacity
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the compressed-data buffer
            emitter.instruction("mov x9, #1");                                  // heap kind 1 = persisted elephc string
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the buffer as an owned string
            emitter.instruction("str x0, [sp, #32]");                           // save the destination buffer pointer
            emitter.instruction("add x1, sp, #24");                             // &destLen in/out parameter
            emitter.instruction("ldr x2, [sp, #8]");                            // source pointer
            emitter.instruction("ldr x3, [sp, #16]");                           // source length
            emitter.instruction("ldr x4, [sp, #0]");                            // compression level
            emitter.bl_c("compress2");                                          // zlib-compress the source into the buffer
            emitter.instruction("ldr x1, [sp, #32]");                           // compressed buffer becomes the result pointer
            emitter.instruction("ldr x2, [sp, #24]");                           // compress2 wrote the compressed length here
            emitter.instruction("add sp, sp, #64");                             // release the scratch frame
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the data string
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
            } else {
                emitter.instruction("mov eax, -1");                             // default zlib compression level
            }
            emitter.instruction("mov rdi, rax");                                // compression level into a scratch register
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx"); // restore the data pointer/length
            // -- inline zlib compress: rdi = level, rsi/rdx = data --
            emitter.instruction("sub rsp, 64");                                 // scratch frame for the compression state
            emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                // save the compression level
            emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                // save the source pointer
            emitter.instruction("mov QWORD PTR [rsp + 16], rdx");               // save the source length
            emitter.instruction("mov rdi, rdx");                                // source length into the compressBound argument
            emitter.instruction("call compressBound");                          // rax = worst-case compressed size
            emitter.instruction("mov QWORD PTR [rsp + 24], rax");               // seed destLen with the buffer capacity
            emitter.instruction("call __rt_heap_alloc");                        // allocate the compressed-data buffer
            emitter.instruction(&format!(                                       // owned-string heap-kind word with the x86_64 heap marker
                "mov r10, 0x{:x}",
                (X86_64_HEAP_MAGIC_HI32 << 32) | 1
            ));
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the buffer as an owned string
            emitter.instruction("mov QWORD PTR [rsp + 32], rax");               // save the destination buffer pointer
            emitter.instruction("mov rdi, rax");                                // destination buffer pointer
            emitter.instruction("lea rsi, [rsp + 24]");                         // &destLen in/out parameter
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // source pointer
            emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");               // source length
            emitter.instruction("mov r8, QWORD PTR [rsp + 0]");                 // compression level
            emitter.instruction("call compress2");                              // zlib-compress the source into the buffer
            emitter.instruction("mov rax, QWORD PTR [rsp + 32]");               // compressed buffer becomes the result pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");               // compress2 wrote the compressed length here
            emitter.instruction("add rsp, 64");                                 // release the scratch frame
        }
    }
    Some(PhpType::Str)
}
