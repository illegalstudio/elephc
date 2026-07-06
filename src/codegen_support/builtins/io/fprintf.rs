//! Purpose:
//! Emits PHP `fprintf` calls: formats like `sprintf` and writes the result to a
//! stream descriptor, returning the number of bytes written.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - `fprintf($stream, $format, ...$values)` = `sprintf($format, ...$values)` +
//!   `fwrite($stream, $result)`. The values are pushed as 16-byte tagged records
//!   (identical to `sprintf`/`printf`) and `__rt_sprintf` pops them on return.
//! - The descriptor is stashed on the stack BELOW the variadic records so it
//!   survives `__rt_sprintf`'s record cleanup, then the formatted bytes are sent
//!   through `__rt_fwrite` (which applies write filters and dispatches user
//!   wrappers, exactly like `fwrite`).

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `fprintf()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fprintf()");
    // args[0] = stream, args[1] = format, args[2..] = values.
    let arg_count = args.len() - 2;

    // -- evaluate the stream descriptor and stash it below the variadic records --
    emit_stream_fd_arg("fprintf", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("str x0, [sp, #-16]!"),            // push the descriptor (survives __rt_sprintf cleanup)
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve a 16-byte slot for the descriptor
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // stash the descriptor below the variadic records
        }
    }

    // -- push the format values in reverse as 16-byte tagged records --
    for i in (2..args.len()).rev() {
        let ty = emit_expr(&args[i], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => match ty {
                PhpType::Int => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push int value
                    emitter.instruction("str xzr, [sp, #8]");                   // type tag 0 = int
                }
                PhpType::Float => {
                    emitter.instruction("fmov x0, d0");                         // move float bits to int register
                    emitter.instruction("str x0, [sp, #-16]!");                 // push float bits
                    emitter.instruction("mov x0, #2");                          // type tag 2 = float
                    emitter.instruction("str x0, [sp, #8]");                    // store type tag
                }
                PhpType::Bool => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push bool value
                    emitter.instruction("mov x0, #3");                          // type tag 3 = bool
                    emitter.instruction("str x0, [sp, #8]");                    // store type tag
                }
                PhpType::Str => {
                    emitter.instruction("str x1, [sp, #-16]!");                 // push string pointer
                    emitter.instruction("lsl x0, x2, #8");                      // shift length left by 8
                    emitter.instruction("orr x0, x0, #1");                      // set type tag bit 0 = str
                    emitter.instruction("str x0, [sp, #8]");                    // store tag|length
                }
                _ => {
                    emitter.instruction("str xzr, [sp, #-16]!");                // push zero
                    emitter.instruction("str xzr, [sp, #8]");                   // type tag 0
                }
            },
            Arch::X86_64 => match ty {
                PhpType::Int => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged record for the integer operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the integer payload in the low half
                    emitter.instruction("mov QWORD PTR [rsp + 8], 0");          // tag the record as an integer operand
                }
                PhpType::Float => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged record for the floating operand
                    emitter.instruction("movsd QWORD PTR [rsp], xmm0");         // store the floating bits in the low half
                    emitter.instruction("mov QWORD PTR [rsp + 8], 2");          // tag the record as a floating operand
                }
                PhpType::Bool => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged record for the boolean operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the boolean payload in the low half
                    emitter.instruction("mov QWORD PTR [rsp + 8], 3");          // tag the record as a boolean operand
                }
                PhpType::Str => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged record for the string operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the string pointer in the low half
                    emitter.instruction("mov rcx, rdx");                        // copy the string length before packing it
                    emitter.instruction("shl rcx, 8");                          // shift the length into the upper metadata bits
                    emitter.instruction("or rcx, 1");                           // tag the record as a string operand
                    emitter.instruction("mov QWORD PTR [rsp + 8], rcx");        // store the packed string metadata word
                }
                _ => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged record for unsupported operands
                    emitter.instruction("mov QWORD PTR [rsp], 0");              // store a zero payload
                    emitter.instruction("mov QWORD PTR [rsp + 8], 0");          // tag the record as an integer zero fallback
                }
            },
        }
    }

    // -- evaluate the format string and format through the sprintf runtime --
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("mov x0, #{}", arg_count)), // number of packed variadic records
        Arch::X86_64 => abi::emit_load_int_immediate(emitter, "rdi", arg_count as i64), // number of packed variadic records
    }
    abi::emit_call_label(emitter, "__rt_sprintf");                              // format → ptr+len; pops the caller's packed records

    // -- write the formatted bytes to the stashed descriptor via __rt_fwrite --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp], #16");                           // pop the stashed descriptor into the fwrite fd argument (x1=ptr, x2=len)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the payload, applying any attached write filter
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // pop the stashed descriptor into the fwrite fd argument
            emitter.instruction("add rsp, 16");                                 // release the descriptor slot
            emitter.instruction("mov rsi, rax");                                // formatted string pointer → second fwrite argument (rdx=len already in place)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the payload, applying any attached write filter
        }
    }
    Some(PhpType::Int)
}
