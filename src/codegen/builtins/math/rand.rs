use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
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
        emitter.instruction("str x0, [sp, #-16]!");                             // push min value onto stack
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("ldr x9, [sp], #16");                               // pop min value into x9
        emitter.instruction("sub x0, x0, x9");                                  // x0 = max - min
        emitter.instruction("add x0, x0, #1");                                  // x0 = range size (max - min + 1)
        emitter.instruction("str x9, [sp, #-16]!");                             // push min back for later use
        emitter.instruction("bl __rt_random_uniform");                          // generate a uniform random offset in [0, range)
        emitter.instruction("ldr x9, [sp], #16");                               // pop min value back into x9
        emitter.instruction("add x0, x0, x9");                                  // x0 = random + min (shift into range)
    } else {
        // -- rand() with no args: return non-negative random int --
        emitter.instruction("bl __rt_random_u32");                              // generate a random uint32 through the runtime helper
    }
    Some(PhpType::Int)
}
