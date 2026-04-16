use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sprintf()");

    // Strategy: evaluate all args, push them on stack with type tags,
    // then call __rt_sprintf which processes the format string.
    //
    // Stack layout per argument (16 bytes each):
    //   [sp + 0] = value (x0 for int/bool, d0 bits for float, x1 for str ptr)
    //   [sp + 8] = type_tag | (for str: length in upper bits)
    // Type tags: 0=int, 1=str (with length in bits 8+), 2=float, 3=bool

    let arg_count = args.len() - 1; // exclude format string

    // -- evaluate and push arguments in reverse order --
    for i in (1..args.len()).rev() {
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
                    emitter.instruction("str x0, [sp, #-16]!");                 // push bool value (0 or 1)
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
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the integer sprintf operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the integer operand payload in the low half of the tagged argument record
                    emitter.instruction("mov QWORD PTR [rsp + 8], 0");          // tag the record as an integer operand for the x86_64 sprintf runtime
                }
                PhpType::Float => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the floating sprintf operand
                    emitter.instruction("movsd QWORD PTR [rsp], xmm0");         // store the floating operand bits in the low half of the tagged argument record
                    emitter.instruction("mov QWORD PTR [rsp + 8], 2");          // tag the record as a floating operand for the x86_64 sprintf runtime
                }
                PhpType::Bool => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the boolean sprintf operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the boolean payload in the low half of the tagged argument record
                    emitter.instruction("mov QWORD PTR [rsp + 8], 3");          // tag the record as a boolean operand for the x86_64 sprintf runtime
                }
                PhpType::Str => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the string sprintf operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the string pointer in the low half of the tagged argument record
                    emitter.instruction("mov rcx, rdx");                        // copy the string length before packing it into the tagged metadata word
                    emitter.instruction("shl rcx, 8");                          // shift the string length into the upper metadata bits of the tagged argument record
                    emitter.instruction("or rcx, 1");                           // tag the record as a string operand while preserving the packed string length
                    emitter.instruction("mov QWORD PTR [rsp + 8], rcx");        // store the packed string metadata word in the high half of the tagged argument record
                }
                _ => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for unsupported sprintf operands
                    emitter.instruction("mov QWORD PTR [rsp], 0");              // store a zero payload for unsupported sprintf operands
                    emitter.instruction("mov QWORD PTR [rsp + 8], 0");          // tag unsupported sprintf operands as integer zero fallbacks
                }
            },
        }
    }

    // -- evaluate format string --
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", arg_count));            // number of format arguments
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rdi", arg_count as i64);     // pass the number of packed variadic records in the first SysV integer argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_sprintf");                              // format the string through the target-aware sprintf runtime helper
    // runtime returns ptr+len and cleans up the caller's packed variadic records

    Some(PhpType::Str)
}
