//! Purpose:
//! Method signatures, aliases, and late-static result metadata.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns the checked signature for an instance method call when metadata is available.
pub(super) fn method_signature(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    method_signature_for_receiver_type(ctx, &ctx.builder.value_php_type(object), method)
}

/// Returns the checked signature for an instance method call on a receiver of `object_ty`.
///
/// The type-level core of `method_signature`, for callers that know the receiver's type but have
/// no lowered receiver value, such as a closure signature derived before its body is lowered.
pub(super) fn method_signature_for_receiver_type(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method: &str,
) -> Option<FunctionSig> {
    let key = php_symbol_key(method);
    if let Some((class_name, _)) = singular_object_class(object_ty) {
        let normalized = class_name.trim_start_matches('\\');
        if !normalized.is_empty() {
            return class_method_signature(ctx, normalized, &key).cloned();
        }
    }
    if dynamic_method_receiver_needs_mixed_fallback(object_ty) {
        if ctx.has_eval_barrier() {
            return None;
        }
        return common_dynamic_method_signature(ctx, &key);
    }
    None
}

/// Promotes the writable destination used by PDOStatement binding methods to a durable Mixed cell.
pub(super) fn promote_pdo_binding_ref_argument(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    args: &[Expr],
) {
    if !type_may_be_pdo_statement(ctx, &ctx.builder.value_php_type(object)) {
        return;
    }
    let parameter_name = match php_symbol_key(method).as_str() {
        "bindparam" => "variable",
        "bindcolumn" => "var",
        _ => return,
    };
    let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
    let argument = expanded_args
        .iter()
        .enumerate()
        .find_map(|(index, arg)| match &arg.kind {
            ExprKind::NamedArg { name, value } if name == parameter_name => Some(value.as_ref()),
            ExprKind::NamedArg { .. } => None,
            _ if index == 1 => Some(arg),
            _ => None,
        });
    let Some(Expr {
        kind: ExprKind::Variable(name),
        span,
    }) = argument
    else {
        return;
    };
    ctx.promote_local_mixed_ref_cell(name, Some(*span));
}

/// Returns whether a receiver type can dispatch to PDOStatement binding methods.
fn type_may_be_pdo_statement(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class) => class_extends_class(ctx, class, "PDOStatement"),
        PhpType::Union(members) => members
            .iter()
            .any(|member| type_may_be_pdo_statement(ctx, member)),
        _ => false,
    }
}

/// Returns the conservative return-to-argument alias summary for a method dispatch.
///
/// A non-final receiver type includes every closed-world descendant implementation,
/// because runtime dispatch can select an override. Missing or synthetic summaries
/// therefore fall back to `Unknown` rather than enabling unsafe cleanup.
pub(super) fn method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> ReturnArgAlias {
    let object_ty = ctx.builder.value_php_type(object);
    let method_key = php_symbol_key(method);
    let mut summary: Option<ReturnArgAlias> = None;
    if let Some((class_name, _)) = singular_object_class(&object_ty) {
        let base_class = class_name.trim_start_matches('\\');
        let Some(base_info) = ctx.classes.get(base_class) else {
            return ReturnArgAlias::Unknown;
        };
        if base_info.is_final || base_info.final_methods.contains(&method_key) {
            return class_method_return_arg_alias(ctx, base_class, &method_key)
                .unwrap_or(ReturnArgAlias::Unknown);
        }
        for candidate in ctx.classes.keys() {
            if !is_same_or_descendant_class(ctx, candidate, base_class) {
                continue;
            }
            let Some(alias) = class_method_return_arg_alias(ctx, candidate, &method_key) else {
                continue;
            };
            summary = Some(match summary {
                Some(current) => current.merge(&alias),
                None => alias,
            });
        }
        return summary.unwrap_or(ReturnArgAlias::Unknown);
    }
    if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
        if ctx.has_eval_barrier() {
            return ReturnArgAlias::Unknown;
        }
        for candidate in ctx.classes.keys() {
            let Some(alias) = class_method_return_arg_alias(ctx, candidate, &method_key) else {
                continue;
            };
            summary = Some(match summary {
                Some(current) => current.merge(&alias),
                None => alias,
            });
        }
    }
    summary.unwrap_or(ReturnArgAlias::Unknown)
}

/// Resolves one concrete class's dispatched implementation and its source summary.
pub(super) fn class_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<ReturnArgAlias> {
    class_method_signature(ctx, class_name, method_key)?;
    let class_info = ctx.classes.get(class_name)?;
    let impl_class = class_info
        .method_impl_classes
        .get(method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    Some(
        ctx.return_alias_summaries
            .method(impl_class, method_key)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown),
    )
}

/// Returns a class/interface method signature, preferring the implementing class metadata.
pub(super) fn class_method_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<&'a FunctionSig> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.classes.get(normalized) {
        let impl_class = class_info
            .method_impl_classes
            .get(method_key)
            .map(String::as_str)
            .unwrap_or(normalized);
        return ctx
            .classes
            .get(impl_class)
            .and_then(|impl_info| impl_info.methods.get(method_key))
            .or_else(|| class_info.methods.get(method_key));
    }
    ctx.interfaces
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(method_key))
}

/// Returns the checked return type for an instance method call when metadata is available.
pub(super) fn method_call_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    method_call_result_type_for_receiver_type(
        ctx,
        &ctx.builder.value_php_type(object),
        method,
        op,
        expr,
    )
}

/// Returns the checked return type for an instance method call on a receiver of `object_ty`.
///
/// The type-level core of `method_call_result_type`: late-static returns are bound to the
/// receiver class and a nullsafe call on a nullable receiver adds `null`. A closure whose body is
/// `return $o->m();` derives its signature through this before the body is lowered, so the two
/// agree on the result the call hands back (issue #1269).
pub(super) fn method_call_result_type_for_receiver_type(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    let nullable = singular_object_class(object_ty)
        .map(|(_, nullable)| nullable)
        .unwrap_or(false);
    let Some(return_ty) = method_signature_for_receiver_type(ctx, object_ty, method)
        .map(|signature| normalize_value_php_type(signature.return_type))
    else {
        if dynamic_method_receiver_needs_mixed_fallback(object_ty) {
            return PhpType::Mixed;
        }
        return fallback_expr_type(expr);
    };
    let return_ty = if let Some((receiver_name, _)) = singular_object_class(object_ty) {
        instance_method_late_static_return_for_ir(ctx, receiver_name, &php_symbol_key(method))
            .map(|return_type| late_static_return_type_for_ir(ctx, &return_type, receiver_name))
            .map(|bound| late_static_result_within_abi(bound, &return_ty))
            .unwrap_or(return_ty)
    } else {
        return_ty
    };
    if op == Op::NullsafeMethodCall && nullable {
        nullable_result_type(return_ty)
    } else {
        return_ty
    }
}

/// Returns the return type a closure signature records for a directly returned
/// `$receiver->method(...)` whose receiver has type `object_ty`.
///
/// Mirrors `method_call_result_type` wherever the metadata names the method, and answers boxed
/// `Mixed` wherever the call could dispatch somewhere this metadata cannot see: a receiver that
/// is not an object type, or a method its class does not declare (a `__call` dispatch, or a
/// closure later rebound to a class that does). Boxing is always a sound return contract. The
/// syntactic fallback this replaces answered `Int` for every method call, so a closure returning
/// `$o->name()` handed a string back through an int slot and printed 0 (issue #1269).
pub(in crate::ir_lower) fn closure_return_method_call_type(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method: &str,
    expr: &Expr,
) -> PhpType {
    if singular_object_class(object_ty).is_none()
        && !dynamic_method_receiver_needs_mixed_fallback(object_ty)
    {
        return PhpType::Mixed;
    }
    if method_signature_for_receiver_type(ctx, object_ty, method).is_none() {
        return PhpType::Mixed;
    }
    method_call_result_type_for_receiver_type(ctx, object_ty, method, Op::MethodCall, expr)
}

/// Returns preserved late-static return syntax for EIR instance dispatch.
pub(super) fn instance_method_late_static_return_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver_type: &str,
    method_key: &str,
) -> Option<TypeExpr> {
    let normalized = receiver_type.trim_start_matches('\\');
    if let Some(class_info) = ctx.classes.get(normalized) {
        if let Some(return_type) = class_info.late_static_method_returns.get(method_key) {
            return Some(return_type.clone());
        }
    }
    ctx.interfaces
        .get(normalized)
        .and_then(|interface_info| interface_info.late_static_method_returns.get(method_key))
        .cloned()
}

/// Keeps a late-static result type only while it travels in the signature's call ABI.
///
/// An override family can box a `static` return into the Mixed call ABI its siblings share
/// (`crate::ir_lower::program::virtual_abi`); the body then hands back a Mixed cell, and typing
/// the call as the bound object class would read that cell as an object pointer.
///
/// The boxed type still names the receiver's own class in place of the declaring one
/// (`static` under `static|false` called on a `D` is `D|false`), so properties only the
/// receiver declares stay readable.
pub(super) fn late_static_result_within_abi(bound: PhpType, nominal: &PhpType) -> PhpType {
    if nominal.codegen_repr() == PhpType::Mixed && bound.codegen_repr() != PhpType::Mixed {
        crate::types::override_families::boxed_member_return(nominal, &bound)
    } else {
        bound
    }
}

/// Binds preserved late-static return syntax to an EIR call-site receiver type.
pub(super) fn late_static_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    return_type: &TypeExpr,
    receiver_type: &str,
) -> PhpType {
    let bound = return_type.substitute_relative_class_types(receiver_type, None);
    normalize_value_php_type(ctx.type_expr_to_php_type_for_value(&bound))
}

/// Returns a common method signature for dynamic receivers when every candidate agrees.
pub(super) fn common_dynamic_method_signature(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
) -> Option<FunctionSig> {
    let mut common = None;
    for class_name in ctx.classes.keys() {
        let Some(signature) = class_method_signature(ctx, class_name, method_key).cloned() else {
            continue;
        };
        match common.as_ref() {
            Some(existing) if existing != &signature => return None,
            Some(_) => {}
            None => common = Some(signature),
        }
    }
    common
}

/// Returns true when an instance-method receiver has no single compile-time class.
pub(super) fn dynamic_method_receiver_needs_mixed_fallback(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Mixed => true,
        PhpType::Object(name) => name.is_empty(),
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Mixed | PhpType::Object(_))),
        _ => false,
    }
}
