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
    emitter.comment("basename()");
    emit_expr(&args[0], emitter, ctx, data);
    if args.len() >= 2 {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the path ptr/len while the suffix expression is evaluated
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x1");                              // move the suffix pointer into the secondary runtime string-argument pair
                emitter.instruction("mov x4, x2");                              // move the suffix length into the secondary runtime string-argument pair
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the path ptr/len after evaluating the suffix expression
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the path ptr/len while the suffix expression is evaluated
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the suffix pointer into the x86_64 secondary runtime string-argument slot
                emitter.instruction("mov rsi, rdx");                            // move the suffix length into the x86_64 secondary runtime string-argument slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore the path ptr/len after evaluating the suffix expression
            }
        }
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x3, #0");                              // no suffix supplied: pointer = 0
                emitter.instruction("mov x4, #0");                              // no suffix supplied: length = 0 (runtime branches on this)
            }
            Arch::X86_64 => {
                emitter.instruction("xor edi, edi");                            // no suffix supplied: pointer = 0
                emitter.instruction("xor esi, esi");                            // no suffix supplied: length = 0 (runtime branches on this)
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_basename");                             // call the target-aware runtime helper that returns the trailing name component
    Some(PhpType::Str)
}
