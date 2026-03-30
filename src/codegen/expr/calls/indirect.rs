use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

pub(super) fn emit_expr_call(
    callee: &Expr,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call expression result");

    let mut arg_types = Vec::new();
    for arg in args_exprs {
        let ty = super::super::emit_expr(arg, emitter, ctx, data);
        super::super::retain_borrowed_heap_arg(emitter, arg, &ty);
        args::push_arg_value(emitter, &ty);
        arg_types.push(ty);
    }

    let _callee_ty = super::super::emit_expr(callee, emitter, ctx, data);
    emitter.instruction("mov x9, x0");                                          // save closure address to x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    let assignments = args::build_arg_assignments(&arg_types, 0);

    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9
    args::load_arg_assignments(emitter, &assignments, args_exprs.len());

    let ret_ty = match &callee.kind {
        ExprKind::Variable(var_name) => {
            if let Some(sig) = ctx.closure_sigs.get(var_name) {
                sig.return_type.clone()
            } else {
                PhpType::Int
            }
        }
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(arr_name) = &array.kind {
                if let Some(sig) = ctx.closure_sigs.get(arr_name) {
                    sig.return_type.clone()
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            }
        }
        ExprKind::Closure { body, .. } => crate::types::checker::infer_return_type_syntactic(body),
        _ => PhpType::Int,
    };

    emitter.instruction("mov x19, x9");                                         // preserve closure address across concat-offset save
    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // branch to closure via function pointer in x19
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}
