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
    emitter.comment("date()");

    match emitter.target.arch {
        Arch::AArch64 => {
            if args.len() == 2 {
                // -- evaluate timestamp argument first --
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");                     // push timestamp onto stack

                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                // x1=format ptr, x2=format len

                // -- pop timestamp into x0 --
                emitter.instruction("ldr x0, [sp], #16");                       // pop timestamp from stack
            } else {
                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                // x1=format ptr, x2=format len

                // -- use -1 to signal "use current time" --
                emitter.instruction("mov x0, #-1");                             // timestamp -1 = use current time
            }
        }
        Arch::X86_64 => {
            if args.len() == 2 {
                // -- evaluate timestamp argument first --
                emit_expr(&args[1], emitter, ctx, data);
                abi::emit_push_reg(emitter, "rax");                             // save the timestamp while the format-string expression is evaluated

                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the format-string pointer into the first x86_64 string-argument register
                emitter.instruction("mov rsi, rdx");                            // move the format-string length into the paired x86_64 string-argument register
                abi::emit_pop_reg(emitter, "rax");                              // restore the timestamp into the x86_64 integer result register
            } else {
                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the format-string pointer into the first x86_64 string-argument register
                emitter.instruction("mov rsi, rdx");                            // move the format-string length into the paired x86_64 string-argument register
                emitter.instruction("mov rax, -1");                             // timestamp -1 = use current time
            }
        }
    }

    // -- call runtime: aarch64 x0/x1/x2, x86_64 rax/rdi/rsi --
    abi::emit_call_label(emitter, "__rt_date");                                 // format the requested Unix timestamp through the target-aware date runtime helper

    Some(PhpType::Str)
}
