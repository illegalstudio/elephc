//! Purpose:
//! Lowers variable and callable-indirect invocation paths.
//! Resolves the callable shape, prepares arguments, and leaves the call result for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - Callable metadata and argument signatures must stay synchronized with type checking and runtime dispatch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

/// Emits a callable-indirect call where the callee expression has already been evaluated
/// and placed in the result register. Handles `__invoke` on objects, closure captures as
/// hidden arguments, and ABI-compliant argument materialization on x86_64.
///
/// - On x86_64: saves the result register to the stack before args, then restores it after
///   to work around the limited argument-passing registers.
/// - If the callee is a known class with `__invoke`, delegates to method call codegen.
/// - Otherwise: resolves the signature from `ctx.closure_sigs` or infers from closure AST,
///   pushes arguments, and emits the call via `nested_call_reg`.
///
/// Returns the PHP return type of the callee (inferred from signature or closure return annotation).
pub(super) fn emit_loaded_expr_call(
    callee: &Expr,
    args_exprs: &[Expr],
    loaded_callee_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call loaded expression result");
    if let Some(class_name) =
        crate::codegen::functions::singular_object_class(loaded_callee_ty).map(str::to_string)
    {
        if ctx
            .classes
            .get(&class_name)
            .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
        {
            if matches!(loaded_callee_ty.codegen_repr(), PhpType::Mixed) {
                crate::codegen::expr::objects::emit_unbox_mixed_object_or_fatal(
                    b"Fatal error: Value of type null is not callable\n",
                    emitter,
                    ctx,
                    data,
                );
            }
            crate::codegen::abi::emit_push_reg(
                emitter,
                crate::codegen::abi::int_result_reg(emitter),
            ); // save the loaded invokable object below later method arguments
            let sig = ctx
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.methods.get("__invoke"))
                .cloned();
            let emitted_args = crate::codegen::expr::objects::emit_pushed_method_args(
                args_exprs,
                sig.as_ref(),
                emitter,
                ctx,
                data,
            );
            return crate::codegen::expr::objects::emit_method_call_with_saved_receiver_below_args(
                &class_name,
                "__invoke",
                &emitted_args.arg_types,
                emitted_args.source_temp_bytes,
                emitter,
                ctx,
            );
        }
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let callee_sig = callee_sig_for_expr(callee, ctx);
    let captures = crate::codegen::callables::callable_captures(callee, ctx);
    crate::codegen::abi::emit_push_reg(emitter, crate::codegen::abi::int_result_reg(emitter)); // save the already-evaluated callable below later arguments

    let emitted_args = args::emit_pushed_call_args(
        args_exprs,
        callee_sig.as_ref(),
        args::regular_param_count(callee_sig.as_ref(), args_exprs.len()),
        "indirect ref arg",
        true,
        false,
        emitter,
        ctx,
        data,
    );
    let mut arg_types = emitted_args.arg_types;

    let call_reg = crate::codegen::abi::nested_call_reg(emitter);
    let arg_temp_bytes = args::pushed_temp_bytes(&arg_types) + emitted_args.source_temp_bytes;
    crate::codegen::abi::emit_load_temporary_stack_slot(emitter, call_reg, arg_temp_bytes);
    crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
        emitter,
        call_reg,
        call_reg,
    );
    push_captures_as_hidden_args(&captures, emitter, ctx, &mut arg_types);

    let assignments =
        crate::codegen::abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = callee_sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or_else(|| match &callee.kind {
            ExprKind::Closure {
                return_type: Some(type_ann),
                ..
            } => crate::codegen::functions::codegen_static_type(type_ann, ctx),
            ExprKind::Closure { body, .. } => {
                crate::types::checker::infer_return_type_syntactic(body)
            }
            _ => PhpType::Int,
        });

    if save_concat_before_args {
        crate::codegen::abi::emit_call_reg(emitter, call_reg);
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, 16);
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
        crate::codegen::abi::emit_call_reg(emitter, call_reg);
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, 16);
    }

    ret_ty
}

/// Resolves the function signature for a callee expression in an indirect call context.
///
/// Looks up the signature in `ctx.closure_sigs` for `Variable` and `ArrayAccess` nodes
/// (where the array is a variable, e.g., `$arr()`). For `FirstClassCallable`, delegates to
/// `first_class_callable_sig`. Returns `None` for unsupported expression kinds, in which
/// case the caller defaults to `PhpType::Int`.
fn callee_sig_for_expr(
    callee: &Expr,
    ctx: &Context,
) -> Option<crate::types::FunctionSig> {
    match &callee.kind {
        ExprKind::Variable(var_name) => ctx.closure_sigs.get(var_name).cloned(),
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(arr_name) = &array.kind {
                ctx.closure_sigs.get(arr_name).cloned()
            } else {
                None
            }
        }
        ExprKind::FirstClassCallable(target) => super::first_class_callable_sig(target, ctx),
        ExprKind::FunctionCall { name, .. } => ctx
            .callable_return_sigs
            .get(name.as_str())
            .cloned(),
        _ => None,
    }
}

/// Pushes closure capture variables as hidden additional arguments before the regular call
/// arguments. For by-reference captures, emits the variable address; for by-value captures,
/// loads the value from the stack slot. Missing captures are warned but skipped to avoid
/// blocking compilation of partially captured closures.
fn push_captures_as_hidden_args(
    captures: &[(String, PhpType, bool)],
    emitter: &mut Emitter,
    ctx: &Context,
    arg_types: &mut Vec<PhpType>,
) {
    for (capture_name, capture_ty, by_ref) in captures {
        emitter.comment(&format!("push callable capture ${}", capture_name));
        if *by_ref {
            if !args::emit_ref_arg_variable_address(
                capture_name,
                "callable capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callable variable ${} not found",
                    capture_name
                ));
                continue;
            }
            args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callable variable ${} not found",
                    capture_name
                ));
                continue;
            };
            crate::codegen::abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            args::push_arg_value(emitter, capture_ty);
            arg_types.push(capture_ty.clone());
        }
    }
}
