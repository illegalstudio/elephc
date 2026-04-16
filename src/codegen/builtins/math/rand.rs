use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}()", name));
    if args.len() == 2 {
        // -- rand(min, max): generate random int in [min, max] --
        emit_expr(&args[0], emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the inclusive minimum while evaluating the inclusive maximum expression
        emit_expr(&args[1], emitter, ctx, data);
        match emitter.target.arch {
            Arch::X86_64 => {
                abi::emit_pop_reg(emitter, "r9");                               // restore the inclusive minimum into a scratch register before forming the random range on SysV x86_64
                emitter.instruction("sub rax, r9");                             // compute the inclusive range width as max - min in the active integer result register
                emitter.instruction("add rax, 1");                              // widen the exclusive upper bound to max - min + 1 before sampling a uniform offset
                emitter.instruction("mov rdi, rax");                            // move the exclusive upper bound into the first SysV integer argument register for __rt_random_uniform
                abi::emit_call_label(emitter, "__rt_random_uniform");           // draw a uniform random offset in the half-open range [0, max - min + 1)
                emitter.instruction("add rax, r9");                             // shift the sampled offset back into the caller-visible inclusive [min, max] interval
            }
            _ => {
                abi::emit_pop_reg(emitter, "x9");                               // restore the inclusive minimum into a scratch register before forming the random range on AArch64
                emitter.instruction("sub x0, x0, x9");                          // compute the inclusive range width as max - min in the active integer result register
                emitter.instruction("add x0, x0, #1");                          // widen the exclusive upper bound to max - min + 1 before sampling a uniform offset
                abi::emit_push_reg(emitter, "x9");                              // preserve the inclusive minimum across the random helper call that reuses the primary integer result register
                abi::emit_call_label(emitter, "__rt_random_uniform");           // draw a uniform random offset in the half-open range [0, max - min + 1)
                abi::emit_pop_reg(emitter, "x9");                               // restore the saved inclusive minimum after the random helper returns the sampled offset
                emitter.instruction("add x0, x0, x9");                          // shift the sampled offset back into the caller-visible inclusive [min, max] interval
            }
        }
    } else {
        // -- rand() with no args: return non-negative random int --
        abi::emit_call_label(emitter, "__rt_random_u32");                       // generate a random uint32 through the target-aware runtime helper
    }
    Some(PhpType::Int)
}
