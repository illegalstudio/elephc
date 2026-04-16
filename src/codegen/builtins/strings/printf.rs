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
    emitter.comment("printf()");

    // printf = sprintf + echo
    let arg_count = args.len() - 1;

    // -- push args in reverse (same as sprintf) --
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
                    emitter.instruction("str x0, [sp, #-16]!");                 // push bool value
                    emitter.instruction("mov x0, #3");                          // type tag 3 = bool
                    emitter.instruction("str x0, [sp, #8]");                    // store type tag
                }
                PhpType::Str => {
                    emitter.instruction("str x1, [sp, #-16]!");                 // push string pointer
                    emitter.instruction("lsl x0, x2, #8");                      // shift length left by 8
                    emitter.instruction("orr x0, x0, #1");                      // set type tag = str
                    emitter.instruction("str x0, [sp, #8]");                    // store tag|length
                }
                _ => {
                    emitter.instruction("str xzr, [sp, #-16]!");                // push zero
                    emitter.instruction("str xzr, [sp, #8]");                   // type tag 0
                }
            },
            Arch::X86_64 => match ty {
                PhpType::Int => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the integer printf operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the integer operand payload in the low half of the tagged argument record
                    emitter.instruction("mov QWORD PTR [rsp + 8], 0");          // tag the record as an integer operand for the x86_64 sprintf runtime
                }
                PhpType::Float => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the floating printf operand
                    emitter.instruction("movsd QWORD PTR [rsp], xmm0");         // store the floating operand bits in the low half of the tagged argument record
                    emitter.instruction("mov QWORD PTR [rsp + 8], 2");          // tag the record as a floating operand for the x86_64 sprintf runtime
                }
                PhpType::Bool => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the boolean printf operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the boolean operand payload in the low half of the tagged argument record
                    emitter.instruction("mov QWORD PTR [rsp + 8], 3");          // tag the record as a boolean operand for the x86_64 sprintf runtime
                }
                PhpType::Str => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for the string printf operand
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // store the string pointer in the low half of the tagged argument record
                    emitter.instruction("mov rcx, rdx");                        // copy the string length before packing it into the tagged metadata word
                    emitter.instruction("shl rcx, 8");                          // shift the string length into the upper metadata bits of the tagged argument record
                    emitter.instruction("or rcx, 1");                           // tag the record as a string operand while preserving the packed string length
                    emitter.instruction("mov QWORD PTR [rsp + 8], rcx");        // store the packed string metadata word in the high half of the tagged argument record
                }
                _ => {
                    emitter.instruction("sub rsp, 16");                         // reserve one 16-byte tagged argument record for unsupported printf operands
                    emitter.instruction("mov QWORD PTR [rsp], 0");              // store a zero payload for unsupported printf operands
                    emitter.instruction("mov QWORD PTR [rsp + 8], 0");          // tag unsupported printf operands as integer zero fallbacks
                }
            },
        }
    }

    // -- evaluate format string --
    emit_expr(&args[0], emitter, ctx, data);

    // -- call sprintf runtime --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", arg_count));            // argument count
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rdi", arg_count as i64);     // pass the number of packed variadic records in the first SysV integer argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_sprintf");                              // format string → ptr+len through the target-aware runtime helper

    // -- write result to stdout --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
            emitter.instruction("mov x0, x2");                                  // return char count
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rdx");                                // preserve the formatted byte count across the x86_64 write syscall sequence
            emitter.instruction("mov rsi, rax");                                // move the formatted string pointer into the SysV write buffer register
            emitter.instruction("mov rdx, rcx");                                // move the formatted string length into the SysV write byte-count register
            emitter.instruction("mov edi, 1");                                  // fd = stdout for the Linux x86_64 write syscall
            emitter.instruction("mov eax, 1");                                  // syscall 1 = write on Linux x86_64
            emitter.instruction("syscall");                                     // write the formatted bytes to stdout through the Linux x86_64 syscall ABI
            emitter.instruction("mov rax, rcx");                                // return the number of bytes written in the primary x86_64 integer result register
        }
    }

    Some(PhpType::Int)
}
