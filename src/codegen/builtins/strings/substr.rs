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
    emitter.comment("substr()");
    emit_expr(&args[0], emitter, ctx, data);
    let neg_done = ctx.next_label("substr_neg_done");
    let len_done = ctx.next_label("substr_len_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- save string and evaluate offset --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push string ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // push offset value onto stack
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                              // move length argument to x3
            } else {
                emitter.instruction("mov x3, #-1");                             // set sentinel -1: use all remaining characters
            }
            // -- restore offset and string from stack --
            emitter.instruction("ldr x0, [sp], #16");                           // pop offset into x0
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop string ptr into x1, length into x2
            // -- handle negative offset --
            emitter.instruction("cmp x0, #0");                                  // check if offset is negative
            emitter.instruction(&format!("b.ge {}", neg_done));                 // skip adjustment if offset >= 0
            emitter.instruction("add x0, x2, x0");                              // convert negative offset: offset = length + offset
            emitter.instruction("cmp x0, #0");                                  // check if adjusted offset is still negative
            emitter.instruction("csel x0, xzr, x0, lt");                        // clamp to 0 if offset went below zero
            emitter.label(&neg_done);
            // -- clamp offset to string length --
            emitter.instruction("cmp x0, x2");                                  // compare offset to string length
            emitter.instruction("csel x0, x2, x0, gt");                         // clamp offset to length if it exceeds it
            // -- adjust pointer and compute result length --
            emitter.instruction("add x1, x1, x0");                              // advance string pointer by offset bytes
            emitter.instruction("sub x2, x2, x0");                              // remaining = length - offset
            // -- apply optional length argument --
            emitter.instruction("cmn x3, #1");                                  // test if x3 == -1 (no length arg given)
            emitter.instruction(&format!("b.eq {}", len_done));                 // skip length clamping if no length arg
            emitter.instruction("cmp x3, #0");                                  // check if length arg is negative
            emitter.instruction("csel x3, xzr, x3, lt");                        // clamp negative length to 0
            emitter.instruction("cmp x3, x2");                                  // compare length arg to remaining chars
            emitter.instruction("csel x2, x3, x2, lt");                         // result length = min(length arg, remaining)
            emitter.label(&len_done);
        }
        Arch::X86_64 => {
            // -- save string and evaluate offset --
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push string ptr and length onto the temporary stack
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");                                 // push offset value onto the temporary stack
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov rcx, rax");                            // move the optional length argument into the x86_64 scratch register
            } else {
                abi::emit_load_int_immediate(emitter, "rcx", -1);               // set sentinel -1 so the helper keeps the full tail when the length is omitted
            }
            // -- restore offset and string from stack --
            abi::emit_pop_reg(emitter, "rax");                                  // pop the substring offset into the primary integer result register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // pop the source string pointer and length into x86_64 scratch registers
            // -- handle negative offset --
            emitter.instruction("cmp rax, 0");                                  // check whether the requested offset is negative
            emitter.instruction(&format!("jge {}", neg_done));                  // skip the negative-offset fixup when the offset is already non-negative
            emitter.instruction("add rax, rsi");                                // convert the negative offset into a tail-relative byte index
            emitter.instruction("cmp rax, 0");                                  // check whether the adjusted tail-relative offset still underflowed past the start
            emitter.instruction("mov r8, 0");                                   // materialize zero for the negative-offset clamp without depending on extra runtime data
            emitter.instruction("cmovl rax, r8");                               // clamp the adjusted offset back to zero when it still points before the string start
            emitter.label(&neg_done);
            // -- clamp offset to string length --
            emitter.instruction("cmp rax, rsi");                                // compare the requested offset against the full source-string length
            emitter.instruction("cmovg rax, rsi");                              // clamp the offset to the full string length when it points past the end
            // -- adjust pointer and compute result length --
            emitter.instruction("add rdi, rax");                                // advance the source-string pointer by the final byte offset
            emitter.instruction("sub rsi, rax");                                // compute the remaining substring length after the final byte offset
            // -- apply optional length argument --
            emitter.instruction("cmp rcx, -1");                                 // check whether the caller omitted the optional length argument
            emitter.instruction(&format!("je {}", len_done));                   // keep the full remaining tail when the optional length argument was omitted
            emitter.instruction("cmp rcx, 0");                                  // check whether the requested substring length is negative
            emitter.instruction("mov r8, 0");                                   // materialize zero for the negative-length clamp without depending on extra runtime data
            emitter.instruction("cmovl rcx, r8");                               // clamp the requested substring length back to zero when it is negative
            emitter.instruction("cmp rcx, rsi");                                // compare the requested substring length against the remaining tail length
            emitter.instruction("cmovl rsi, rcx");                              // shrink the substring length when the explicit requested length is shorter than the tail
            emitter.label(&len_done);
            emitter.instruction("mov rax, rdi");                                // return the borrowed substring pointer in the primary x86_64 string result register
            emitter.instruction("mov rdx, rsi");                                // return the borrowed substring length in the secondary x86_64 string result register
        }
    }

    Some(PhpType::Str)
}
