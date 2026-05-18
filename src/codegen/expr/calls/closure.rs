//! Purpose:
//! Lowers closure invocation expressions and captured environment handling.
//! Resolves the callable shape, prepares arguments, and leaves the call result for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - Callable metadata and argument signatures must stay synchronized with type checking and runtime dispatch.

use crate::codegen::abi;
use crate::codegen::context::{Context, DeferredClosure};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, Stmt, StmtKind, StaticReceiver, TypeExpr};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

use super::args;

fn infer_closure_return_type(
    body: &[Stmt],
    sig: &FunctionSig,
    capture_types: &[(String, PhpType, bool)],
) -> PhpType {
    if crate::types::checker::yield_validation::body_contains_yield(body) {
        return PhpType::Object("Generator".to_string());
    }

    fn collect_return_types(
        stmt: &Stmt,
        sig: &FunctionSig,
        capture_ctx: &Context,
        return_types: &mut Vec<PhpType>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                return_types.push(crate::codegen::functions::infer_local_type_with_ctx(
                    expr,
                    sig,
                    capture_ctx,
                ));
            }
            StmtKind::Return(None) => {
                return_types.push(PhpType::Void);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for stmt in then_body {
                    collect_return_types(stmt, sig, capture_ctx, return_types);
                }
                for (_, body) in elseif_clauses {
                    for stmt in body {
                        collect_return_types(stmt, sig, capture_ctx, return_types);
                    }
                }
                if let Some(body) = else_body {
                    for stmt in body {
                        collect_return_types(stmt, sig, capture_ctx, return_types);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for stmt in body {
                    collect_return_types(stmt, sig, capture_ctx, return_types);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for stmt in try_body {
                    collect_return_types(stmt, sig, capture_ctx, return_types);
                }
                for catch_clause in catches {
                    for stmt in &catch_clause.body {
                        collect_return_types(stmt, sig, capture_ctx, return_types);
                    }
                }
                if let Some(body) = finally_body {
                    for stmt in body {
                        collect_return_types(stmt, sig, capture_ctx, return_types);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for stmt in body {
                        collect_return_types(stmt, sig, capture_ctx, return_types);
                    }
                }
                if let Some(body) = default {
                    for stmt in body {
                        collect_return_types(stmt, sig, capture_ctx, return_types);
                    }
                }
            }
            _ => {}
        }
    }

    let mut capture_ctx = Context::new();
    for (name, ty, _) in capture_types {
        capture_ctx.alloc_var_with_static_type(name, ty.codegen_repr(), ty.clone());
    }

    let mut return_types = Vec::new();
    for stmt in body {
        collect_return_types(stmt, sig, &capture_ctx, &mut return_types);
    }
    if return_types.is_empty() {
        return PhpType::Int;
    }
    let mut result = return_types[0].clone();
    for ty in &return_types[1..] {
        result = super::super::widen_codegen_type(&result, ty);
    }
    result
}

pub(super) fn emit_closure(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    return_type: &Option<TypeExpr>,
    body: &[Stmt],
    captures: &[String],
    capture_refs: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> PhpType {
    let closure_label = ctx.next_label("closure");

    let mut capture_types: Vec<(String, PhpType, bool)> = Vec::new();
    for cap_name in captures {
        let ty = ctx
            .variables
            .get(cap_name)
            .map(|v| v.ty.clone())
            .unwrap_or(PhpType::Int);
        let by_ref = capture_refs.iter().any(|name| name == cap_name);
        capture_types.push((cap_name.clone(), ty, by_ref));
    }

    let mut param_types: Vec<(String, PhpType)> = params
        .iter()
        .map(|(p, type_ann, _, _)| {
            let ty = type_ann
                .as_ref()
                .map(|type_ann| functions::codegen_declared_type(type_ann, ctx))
                .unwrap_or(PhpType::Int);
            (p.clone(), ty)
        })
        .collect();
    if let Some(variadic_name) = variadic {
        param_types.push((
            variadic_name.clone(),
            PhpType::Array(Box::new(PhpType::Int)),
        ));
    }
    let mut defaults: Vec<Option<Expr>> = params
        .iter()
        .map(|(_, _, default, _)| default.clone())
        .collect();
    if variadic.is_some() {
        defaults.push(None);
    }
    let mut ref_params: Vec<bool> = params.iter().map(|(_, _, _, is_ref)| *is_ref).collect();
    let mut declared_params: Vec<bool> =
        params.iter().map(|(_, type_ann, _, _)| type_ann.is_some()).collect();
    if variadic.is_some() {
        ref_params.push(false);
        declared_params.push(false);
    }
    let preliminary_sig = FunctionSig {
        params: param_types.clone(),
        defaults: defaults.clone(),
        return_type: PhpType::Int,
        declared_return: false,
        ref_params: ref_params.clone(),
        declared_params: declared_params.clone(),
        variadic: variadic.clone(),
        deprecation: None,
    };
    let resolved_return_type = return_type
        .as_ref()
        .map(|type_ann| functions::codegen_static_type(type_ann, ctx))
        .unwrap_or_else(|| infer_closure_return_type(body, &preliminary_sig, &capture_types));
    let sig = FunctionSig {
        params: param_types,
        defaults,
        return_type: resolved_return_type,
        declared_return: return_type.is_some(),
        ref_params,
        declared_params,
        variadic: variadic.clone(),
        deprecation: None,
    };

    let param_names: Vec<String> = params.iter().map(|(n, _, _, _)| n.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: closure_label.clone(),
        params: param_names,
        body: body.to_vec(),
        sig,
        captures: capture_types.clone(),
        hidden_params: capture_types,
        current_class: ctx.current_class.clone(),
        // Real closure literals are only reachable through their wrapper, so the
        // dead-wrapper stub optimisation never applies here.
        needed: true,
    });

    emitter.comment("closure: load function address");
    abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &closure_label);
    PhpType::Callable
}

pub(super) fn emit_closure_call(
    var: &str,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if let Some(class_name) = ctx
        .variables
        .get(var)
        .and_then(|info| functions::singular_object_class(&info.static_ty))
        .map(str::to_string)
    {
        if ctx
            .classes
            .get(&class_name)
            .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
        {
            let object = Expr::new(ExprKind::Variable(var.to_string()), Span::dummy());
            return crate::codegen::expr::objects::emit_method_call(
                &object, "__invoke", args_exprs, emitter, ctx, data,
            );
        }
    }

    // First-class callable short-circuit: when the variable was last bound to a
    // first-class callable, calling it as `$cb(args)` reaches the target directly
    // instead of going through the closure wrapper.
    //
    // - `Function`: dispatch via extern → builtin → user-defined, mirroring
    //   `ExprKind::FunctionCall`.
    // - `Method` with a simple object (`Variable` or `This`): the captured
    //   receiver is re-loaded from the original variable slot just like the
    //   closure wrapper does today, so `emit_method_call` preserves semantics.
    // - `StaticMethod` with a `Named` receiver: late-static binding is already
    //   absent, so a direct static call is safe.
    // - `Method` with a complex object expression or `StaticMethod` with a
    //   `Self_`/`Parent`/`Static` receiver fall through to the closure wrapper
    //   path; reconstituting their captured runtime context is left for a
    //   future refinement.
    if let Some(target) = ctx.first_class_callable_targets.get(var).cloned() {
        match &target {
            CallableTarget::Function(name) => {
                let name_str = name.as_str();
                let span = args_exprs
                    .first()
                    .map(|e| e.span)
                    .unwrap_or_else(Span::dummy);
                if ctx.extern_functions.contains_key(name_str) {
                    return crate::codegen::ffi::emit_extern_call(
                        name_str, args_exprs, span, emitter, ctx, data,
                    );
                }
                if let Some(ty) = crate::codegen::builtins::emit_builtin_call(
                    name_str, args_exprs, span, emitter, ctx, data,
                ) {
                    return ty;
                }
                return super::function::emit_function_call(
                    name_str, args_exprs, emitter, ctx, data,
                );
            }
            CallableTarget::Method { object, method } => {
                if matches!(&object.kind, ExprKind::Variable(_) | ExprKind::This) {
                    return crate::codegen::expr::objects::emit_method_call(
                        object, method, args_exprs, emitter, ctx, data,
                    );
                }
            }
            CallableTarget::StaticMethod { receiver, method } => {
                // `Named`: direct compile-time class. `Static`: late-static binding
                // resolves via the caller scope's hidden `__elephc_called_class_id` /
                // `$this` slot, the same chain `emit_forwarded_called_class_id` uses
                // inside the closure wrapper — so calling here is equivalent without
                // the wrapper trampoline. `Self_` / `Parent` are pre-resolved to
                // `Named` at storage time and never reach this match arm.
                if matches!(receiver, StaticReceiver::Named(_) | StaticReceiver::Static) {
                    return crate::codegen::expr::objects::emit_static_method_call(
                        receiver, method, args_exprs, emitter, ctx, data,
                    );
                }
            }
        }
    }

    // We reach this point only when the short-circuit above did not fire. The
    // call is going to invoke the wrapper indirectly via `blr`, so the FCC
    // wrapper (if any) must keep its body. This is also the path real closures
    // take, where `mark_fcc_used` is a no-op.
    ctx.mark_fcc_used(var);

    emitter.comment(&format!("call ${}()", var));
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let sig = ctx.closure_sigs.get(var).cloned();
    let captures = ctx.closure_captures.get(var).cloned().unwrap_or_default();
    let visible_param_count = sig.as_ref().map(|s| s.params.len()).unwrap_or(args_exprs.len());
    let regular_param_count = sig
        .as_ref()
        .map(|s| if s.variadic.is_some() { visible_param_count.saturating_sub(1) } else { visible_param_count })
        .unwrap_or(args_exprs.len());
    let emitted_args = args::emit_pushed_call_args(
        args_exprs,
        sig.as_ref(),
        regular_param_count,
        "closure ref arg",
        true,
        true,
        emitter,
        ctx,
        data,
    );
    let mut arg_types = emitted_args.arg_types;

    if let Some(cached_sig) = ctx.closure_sigs.get(var).cloned() {
        for deferred in &mut ctx.deferred_closures {
            if deferred.sig.params == cached_sig.params && deferred.captures == captures {
                for (i, ty) in arg_types.iter().enumerate() {
                    if i < deferred.sig.params.len()
                        && !deferred
                            .sig
                            .declared_params
                            .get(i)
                            .copied()
                            .unwrap_or(false)
                        && !deferred.sig.ref_params.get(i).copied().unwrap_or(false)
                    {
                        deferred.sig.params[i].1 = ty.clone();
                    }
                }
                break;
            }
        }
        if let Some(cached) = ctx.closure_sigs.get_mut(var) {
            for (i, ty) in arg_types.iter().enumerate() {
                if i < cached.params.len()
                    && !cached.declared_params.get(i).copied().unwrap_or(false)
                    && !cached.ref_params.get(i).copied().unwrap_or(false)
                {
                    cached.params[i].1 = ty.clone();
                }
            }
        }
    }

    for (cap_name, cap_ty, by_ref) in &captures {
        emitter.comment(&format!("push captured ${}", cap_name));
        if *by_ref {
            if !args::emit_ref_arg_variable_address(cap_name, "closure capture ref", emitter, ctx)
            {
                emitter.comment(&format!(
                    "WARNING: captured variable ${} not found",
                    cap_name
                ));
                continue;
            }
            super::args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let cap_info = match ctx.variables.get(cap_name) {
                Some(v) => v,
                None => {
                    emitter.comment(&format!(
                        "WARNING: captured variable ${} not found",
                        cap_name
                    ));
                    continue;
                }
            };
            let cap_offset = cap_info.stack_offset;
            crate::codegen::abi::emit_load(emitter, cap_ty, cap_offset);
            super::args::push_arg_value(emitter, cap_ty);
            arg_types.push(cap_ty.clone());
        }
    }
    let var_info = match ctx.variables.get(var) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined closure variable ${}", var));
            if save_concat_before_args {
                super::super::restore_concat_offset_after_nested_call(emitter, ctx, &PhpType::Int);
            }
            crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
            return PhpType::Int;
        }
    };
    let var_offset = var_info.stack_offset;
    let call_reg = crate::codegen::abi::nested_call_reg(emitter);
    if ctx.ref_params.contains(var) {
        crate::codegen::abi::load_at_offset(emitter, call_reg, var_offset);     // load the by-reference callable slot address into the nested-call scratch register
        crate::codegen::abi::emit_load_from_address(emitter, call_reg, call_reg, 0);
    } else {
        crate::codegen::abi::load_at_offset(emitter, call_reg, var_offset);     // load the closure function address into the nested-call scratch register
    }
    crate::codegen::abi::emit_push_reg(emitter, call_reg);

    let assignments =
        crate::codegen::abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);

    crate::codegen::abi::emit_pop_reg(emitter, call_reg);
    let overflow_bytes = crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = ctx
        .closure_sigs
        .get(var)
        .map(|s| s.return_type.clone())
        .unwrap_or(PhpType::Int);

    if !save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }
    crate::codegen::abi::emit_call_reg(emitter, call_reg);
    if save_concat_before_args {
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
    }

    ret_ty
}
