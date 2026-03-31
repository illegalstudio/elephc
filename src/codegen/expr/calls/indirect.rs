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

    let callee_sig = match &callee.kind {
        ExprKind::Variable(var_name) => ctx.closure_sigs.get(var_name).cloned(),
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(arr_name) = &array.kind {
                ctx.closure_sigs.get(arr_name).cloned()
            } else {
                None
            }
        }
        ExprKind::FirstClassCallable(target) => super::first_class_callable_sig(target, ctx),
        _ => None,
    };

    let mut arg_types = Vec::new();
    for (i, arg) in args_exprs.iter().enumerate() {
        let is_ref = callee_sig
            .as_ref()
            .and_then(|sig| sig.ref_params.get(i))
            .copied()
            .unwrap_or(false);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if ctx.global_vars.contains(var_name) {
                    let label = format!("_gvar_{}", var_name);
                    emitter.comment(&format!("indirect ref arg: address of global ${}", var_name));
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));   // load page of global var
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); // resolve global var address
                } else if ctx.ref_params.contains(var_name) {
                    let Some(var) = ctx.variables.get(var_name) else {
                        emitter.comment(&format!("WARNING: undefined ref variable ${}", var_name));
                        continue;
                    };
                    emitter.comment(&format!("indirect ref arg: forward underlying reference for ${}", var_name));
                    crate::codegen::abi::load_at_offset(emitter, "x0", var.stack_offset); // load existing reference pointer
                } else {
                    let Some(var) = ctx.variables.get(var_name) else {
                        emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                        continue;
                    };
                    emitter.comment(&format!("indirect ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", var.stack_offset)); // compute address of local variable
                }
            } else {
                let ty = super::super::emit_expr(arg, emitter, ctx, data);
                super::super::retain_borrowed_heap_arg(emitter, arg, &ty);
            }
            emitter.instruction("str x0, [sp, #-16]!");                             // push address for by-ref argument
            arg_types.push(PhpType::Int);
        } else {
            let ty = super::super::emit_expr(arg, emitter, ctx, data);
            super::super::retain_borrowed_heap_arg(emitter, arg, &ty);
            args::push_arg_value(emitter, &ty);
            arg_types.push(ty);
        }
    }

    let _callee_ty = super::super::emit_expr(callee, emitter, ctx, data);
    emitter.instruction("mov x9, x0");                                          // save closure address to x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    let assignments = args::build_arg_assignments(&arg_types, 0);

    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9
    args::load_arg_assignments(emitter, &assignments, args_exprs.len());

    let ret_ty = callee_sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or_else(|| match &callee.kind {
            ExprKind::Closure { body, .. } => crate::types::checker::infer_return_type_syntactic(body),
            _ => PhpType::Int,
        });

    emitter.instruction("mov x19, x9");                                         // preserve closure address across concat-offset save
    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // branch to closure via function pointer in x19
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}
