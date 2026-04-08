use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::expr::calls::args;
use crate::codegen::abi;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func()");

    // -- resolve callback function address --
    let is_callable_expr = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let mut sig: Option<FunctionSig> = None;
    if is_callable_expr {
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("mov x19, x0");                                     // move synthesized callback address to x19
        if let Some(deferred) = ctx.deferred_closures.last() {
            sig = Some(deferred.sig.clone());
        }
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                                // load callback address from callable variable
        if let Some(closure_sig) = ctx.closure_sigs.get(var_name) {
            sig = Some(closure_sig.clone());
        }
    } else {
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("call_user_func() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        sig = ctx.functions.get(&func_name).cloned();
        emitter.adrp("x19", &format!("{}", label));              // load page address of callback function
        emitter.add_lo12("x19", "x19", &format!("{}", label));       // resolve full address of callback function
    }
    let ret_ty = sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Int);

    // -- evaluate remaining arguments and push onto stack --
    let mut arg_types = Vec::new();
    for (i, arg) in args[1..].iter().enumerate() {
        let is_ref = sig
            .as_ref()
            .and_then(|sig| sig.ref_params.get(i))
            .copied()
            .unwrap_or(false);
        let target_ty = args::declared_target_ty(sig.as_ref(), i);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !args::emit_ref_arg_variable_address(var_name, "call_user_func ref arg", emitter, ctx) {
                    panic!("call_user_func() by-reference callback argument variable not found");
                }
            } else {
                panic!("call_user_func() by-reference callback argument must be a variable");
            }
            args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
            continue;
        }

        let pushed_ty = args::push_expr_arg(arg, target_ty, emitter, ctx, data);
        arg_types.push(pushed_ty);
    }

    if let Some(sig) = &sig {
        let regular_param_count = if sig.variadic.is_some() {
            sig.params.len().saturating_sub(1)
        } else {
            sig.params.len()
        };
        for i in arg_types.len()..regular_param_count {
            if let Some(Some(default_expr)) = sig.defaults.get(i) {
                let target_ty = sig.params.get(i).map(|(_, ty)| ty);
                let pushed_ty = args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
                arg_types.push(pushed_ty);
            }
        }
    }

    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);

    // -- load callback address and call via blr --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // call callback function via indirect branch
    crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, &ret_ty);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack callback arguments after the indirect call returns
    }

    Some(ret_ty)
}
