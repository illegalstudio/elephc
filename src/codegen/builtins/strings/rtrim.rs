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
    emitter.comment("rtrim()");

    if args.len() == 1 {
        emit_expr(&args[0], emitter, ctx, data);
        // -- strip whitespace from the right --
        abi::emit_call_label(emitter, "__rt_rtrim");                            // call the target-aware runtime helper that trims ASCII whitespace from the end of the current string slice
    } else {
        // -- rtrim with character mask --
        emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("str x1, [sp, #-16]!");                     // preserve the source string pointer while the trim-mask expression is evaluated
                emitter.instruction("str x2, [sp, #-16]!");                     // preserve the source string length while the trim-mask expression is evaluated
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x1");                              // move the trim-mask pointer into the secondary AArch64 trim-mask argument register pair
                emitter.instruction("mov x4, x2");                              // move the trim-mask length into the secondary AArch64 trim-mask argument register pair
                emitter.instruction("ldr x2, [sp], #16");                       // restore the source string length after evaluating the trim-mask expression
                emitter.instruction("ldr x1, [sp], #16");                       // restore the source string pointer after evaluating the trim-mask expression
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the source string ptr/len while the trim-mask expression is evaluated on x86_64
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the trim-mask pointer into the secondary x86_64 trim-mask argument register
                emitter.instruction("mov rsi, rdx");                            // move the trim-mask length into the secondary x86_64 trim-mask argument register
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore the source string ptr/len after evaluating the trim-mask expression
            }
        }
        abi::emit_call_label(emitter, "__rt_rtrim_mask");                       // call the target-aware runtime helper that trims mask bytes from the end of the current string slice
    }

    Some(PhpType::Str)
}
