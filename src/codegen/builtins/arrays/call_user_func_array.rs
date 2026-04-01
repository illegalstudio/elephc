use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::expr::calls::args;
use crate::codegen::abi;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func_array()");

    // -- resolve callback function address and signature --
    let is_callable_expr = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let sig = if is_callable_expr {
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("mov x19, x0");                                         // move synthesized callback address to x19
        ctx.deferred_closures
            .last()
            .expect("call_user_func_array: missing synthesized callable signature")
            .sig
            .clone()
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                                // load callback address from callable variable
        ctx.closure_sigs
            .get(var_name)
            .expect("call_user_func_array: callable variable signature not found")
            .clone()
    } else {
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("call_user_func_array() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        emitter.instruction(&format!("adrp x19, {}@PAGE", label));                  // load page address of callback function
        emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));           // resolve full address of callback
        ctx.functions
            .get(&func_name)
            .expect("call_user_func_array: function not found")
            .clone()
    };

    // Evaluate the array argument (second arg)
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // Determine element type and size from the array type
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    };
    let elem_size = args::array_element_stride(&elem_ty);

    emitter.instruction("mov x20, x0");                                         // preserve the callback-argument array pointer across element boxing

    // -- extract elements from array and push them as regular call arguments --
    let mut arg_types = Vec::new();
    for (i, (_pname, _pty)) in sig.params.iter().enumerate() {
        emitter.instruction("add x9, x20, #24");                                // point x9 at the callback-argument array payload
        args::load_array_element_to_result(emitter, &elem_ty, "x9", i * elem_size);
        let target_ty = args::declared_target_ty(Some(&sig), i);
        let pushed_ty = args::push_loaded_array_element_arg(&elem_ty, target_ty, emitter, ctx, data);
        arg_types.push(pushed_ty);
    }

    let assignments = args::build_arg_assignments(&arg_types, 0);
    args::load_arg_assignments(emitter, &assignments, arg_types.len());

    let ret_ty = sig.return_type.clone();

    // -- call callback via the resolved address in x19 --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // call callback via indirect branch
    crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    Some(ret_ty)
}
