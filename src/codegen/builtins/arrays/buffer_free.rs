use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("buffer_free()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_heap_free");                            // release the buffer header and contiguous payload through the target-aware heap helper

    // -- nullify the local stack slot so use-after-free hits a null check --
    // The type checker restricts buffer_free() to plain local variables only
    // (no ref params, globals, or statics), so writing xzr to the stack slot
    // is always the correct nullification path here.
    if let ExprKind::Variable(var_name) = &args[0].kind {
        if let Some(var) = ctx.variables.get(var_name) {
            if !ctx.ref_params.contains(var_name)
                && !ctx.global_vars.contains(var_name)
                && !ctx.static_vars.contains(var_name)
            {
                abi::emit_store_zero_to_local_slot(emitter, var.stack_offset);   // zero the local stack slot so subsequent buffer accesses trip the null-buffer fatal helper
            }
        }
    }

    Some(PhpType::Void)
}
