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
        emitter.instruction("mov x19, x0");                                         // move synthesized callback address to x19
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
        emitter.instruction(&format!("adrp x19, {}@PAGE", label));                  // load page address of callback function
        emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));           // resolve full address of callback function
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
                if ctx.global_vars.contains(var_name) {
                    let label = format!("_gvar_{}", var_name);
                    emitter.comment(&format!("call_user_func ref arg: address of global ${}", var_name));
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));       // load page of global var
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); // resolve global var address
                } else if ctx.ref_params.contains(var_name) {
                    let var = ctx.variables.get(var_name).expect("undefined ref callback argument");
                    emitter.comment(&format!("call_user_func ref arg: forward underlying reference for ${}", var_name));
                    abi::load_at_offset(emitter, "x0", var.stack_offset);            // load existing reference pointer
                } else {
                    let var = ctx.variables.get(var_name).expect("undefined callback argument");
                    emitter.comment(&format!("call_user_func ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", var.stack_offset)); // compute address of local variable
                }
            } else {
                panic!("call_user_func() by-reference callback argument must be a variable");
            }
            emitter.instruction("str x0, [sp, #-16]!");                             // push argument address onto stack
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

    // -- pop arguments back into ABI registers in reverse --
    let mut int_reg = 0usize;
    let mut reg_assignments: Vec<(PhpType, usize)> = Vec::new();
    for ty in &arg_types {
        reg_assignments.push((ty.clone(), int_reg));
        int_reg += ty.register_count();
    }
    for i in (0..arg_types.len()).rev() {
        let (ty, start_reg) = &reg_assignments[i];
        match ty {
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string ptr+len into registers
                    "ldp x{}, x{}, [sp], #16",
                    start_reg, start_reg + 1
                ));
            }
            _ => {
                emitter.instruction(&format!(                                   // pop int/bool/array arg into register
                    "ldr x{}, [sp], #16",
                    start_reg
                ));
            }
        }
    }

    // -- load callback address and call via blr --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // call callback function via indirect branch
    crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    Some(ret_ty)
}
