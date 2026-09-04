//! Purpose:
//! Literal eval barrier analysis and post-eval name probes.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns true when a literal `eval` call may still need runtime scope/interpreter state.
pub(super) fn eval_literal_needs_barrier(ctx: &LoweringContext<'_, '_>, fragment: &str) -> bool {
    let static_call_supported = |name: &str, args: &[Expr]| {
        eval_literal_static_function_supported_by_lowering(ctx, name, args)
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        ctx.source_path(),
        crate::strict_php::is_enabled(),
        static_call_supported,
        |receiver, method, args| {
            eval_literal_static_method_supported_by_lowering(ctx, receiver, method, args)
        },
    );
    if plan.is_fully_static_no_bridge() {
        return false;
    }
    if plan.uses_scope_read_params()
        && eval_literal_scope_read_params_supported_by_lowering(
            ctx,
            plan.reads(),
            plan.quiet_reads(),
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        )
    {
        return false;
    }
    if plan.requires_runtime_eval_scope()
        && eval_literal_scope_constraints_supported_by_lowering(
            ctx,
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        )
    {
        return false;
    }
    true
}

/// Returns the caller locals written by an EIR literal `eval` that only needs scope state.
pub(super) fn eval_literal_scope_barrier_writes(
    ctx: &LoweringContext<'_, '_>,
    fragment: &str,
) -> Option<std::collections::BTreeSet<String>> {
    let static_call_supported = |name: &str, args: &[Expr]| {
        eval_literal_static_function_supported_by_lowering(ctx, name, args)
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        ctx.source_path(),
        crate::strict_php::is_enabled(),
        static_call_supported,
        |receiver, method, args| {
            eval_literal_static_method_supported_by_lowering(ctx, receiver, method, args)
        },
    );
    (plan.requires_runtime_eval_scope()
        && eval_literal_scope_constraints_supported_by_lowering(
            ctx,
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        ))
    .then(|| plan.writes().clone())
}

/// Returns true when all scope-read variables can be passed as direct Mixed params.
pub(super) fn eval_literal_scope_read_params_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    read_names: &std::collections::BTreeSet<String>,
    quiet_read_names: &std::collections::BTreeSet<String>,
    array_read_constraints: &std::collections::BTreeSet<String>,
    assoc_array_read_constraints: &std::collections::BTreeSet<String>,
    float_predicate_read_constraints: &std::collections::BTreeSet<String>,
) -> bool {
    read_names
        .iter()
        .all(|name| {
            eval_literal_scope_read_param_supported_by_lowering(
                ctx,
                name,
                quiet_read_names.contains(name),
            )
        })
        && array_read_constraints
            .iter()
            .all(|name| eval_literal_scope_read_array_param_supported_by_lowering(ctx, name))
        && assoc_array_read_constraints
            .iter()
            .all(|name| eval_literal_scope_read_assoc_array_param_supported_by_lowering(ctx, name))
        && float_predicate_read_constraints.iter().all(|name| {
            eval_literal_scope_read_float_predicate_param_supported_by_lowering(ctx, name)
        })
}

/// Returns true when all constrained scope reads fit caller local types.
pub(super) fn eval_literal_scope_constraints_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    array_read_constraints: &std::collections::BTreeSet<String>,
    assoc_array_read_constraints: &std::collections::BTreeSet<String>,
    float_predicate_read_constraints: &std::collections::BTreeSet<String>,
) -> bool {
    array_read_constraints
        .iter()
        .all(|name| eval_literal_scope_read_array_param_supported_by_lowering(ctx, name))
        && assoc_array_read_constraints
            .iter()
            .all(|name| eval_literal_scope_read_assoc_array_param_supported_by_lowering(ctx, name))
        && float_predicate_read_constraints.iter().all(|name| {
            eval_literal_scope_read_float_predicate_param_supported_by_lowering(ctx, name)
        })
}

/// Returns true when one read variable is initialized and needs no eval runtime state.
pub(super) fn eval_literal_scope_read_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    quiet: bool,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return quiet;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when one direct read-param is statically known to be array-like.
pub(super) fn eval_literal_scope_read_array_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_array_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when one direct read-param is statically known to be associative-array-like.
pub(super) fn eval_literal_scope_read_assoc_array_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_assoc_array_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when one direct read-param can feed float predicate builtins safely.
pub(super) fn eval_literal_scope_read_float_predicate_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_float_predicate_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when a local type can be boxed to the param-mode Mixed ABI.
pub(super) fn eval_literal_scope_read_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a local type satisfies array-only direct read-param semantics.
pub(super) fn eval_literal_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a local type satisfies associative-array direct read-param semantics.
pub(super) fn eval_literal_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a local type can reach IEEE float predicates without TypeError.
pub(super) fn eval_literal_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Returns the literal eval fragment when the call is a simple `eval('...')`.
pub(super) fn eval_literal_fragment<'a>(name: &str, args: &'a [Expr]) -> Option<&'a str> {
    if php_symbol_key(name.trim_start_matches('\\')) != "eval"
        || args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    match &args[0].kind {
        ExprKind::StringLiteral(fragment) => Some(fragment.as_str()),
        _ => None,
    }
}

/// Returns true when a literal-eval static function call can avoid the eval barrier.
pub(super) fn eval_literal_static_function_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(signature) = ctx
        .functions
        .iter()
        .find(|(function_name, _)| php_symbol_key(function_name.trim_start_matches('\\')) == key)
        .map(|(_, signature)| signature)
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a literal-eval static method call can avoid the eval barrier.
pub(super) fn eval_literal_static_method_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 || !matches!(receiver, StaticReceiver::Named(_)) {
        return false;
    }
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return false;
    };
    let method_key = php_symbol_key(method);
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return false;
    };
    if class_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let Some(signature) = static_method_implementation_signature(ctx, receiver, method) else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a dynamic eval fallback can preserve simple positional call semantics.
pub(super) fn plain_positional_call_args(args: &[Expr]) -> bool {
    !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
}

/// Lowers post-eval function-name probes through the eval context's dynamic table.
pub(super) fn lower_eval_function_probe(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let probe_name = php_symbol_key(name.trim_start_matches('\\'));
    if probe_name != "function_exists" && probe_name != "is_callable" {
        return None;
    }
    if !ctx.has_eval_barrier()
        || args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::StringLiteral(function_name) = &args[0].kind else {
        return None;
    };
    if function_name.contains("::")
        || resolve_static_string_callable(ctx, function_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(function_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    Some(ctx.emit_value(
        Op::EvalFunctionExists,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::EvalFunctionExists.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers post-eval class-name probes through the eval context's dynamic class table.
pub(super) fn lower_eval_class_probe(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let probe_name = php_symbol_key(name.trim_start_matches('\\'));
    if probe_name != "class_exists" {
        return None;
    }
    if !ctx.has_eval_barrier()
        || args.is_empty()
        || args.len() > 2
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::StringLiteral(class_name) = &args[0].kind else {
        return None;
    };
    if aot_class_exists_for_eval_probe(ctx, class_name) {
        return None;
    }
    if let Some(autoload) = args.get(1) {
        lower_expr(ctx, autoload);
    }
    let data = ctx.intern_class_name(class_name);
    Some(ctx.emit_value(
        Op::EvalClassExists,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::EvalClassExists.default_effects(),
        Some(expr.span),
    ))
}

/// Returns true when an AOT class already satisfies a native class_exists probe.
pub(super) fn aot_class_exists_for_eval_probe(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    let key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
}
