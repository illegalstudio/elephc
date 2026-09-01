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
    let object_ty = ctx.builder.value_php_type(object);
    let key = php_symbol_key(method);
    if let Some(class_name) = method_receiver_object_class(&object_ty) {
        let normalized = class_name.trim_start_matches('\\');
        return class_method_signature(ctx, normalized, &key).cloned();
    }
    if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
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
    let object_ty = ctx.builder.value_php_type(object);
    let nullable = singular_object_class(&object_ty)
        .map(|(_, nullable)| nullable)
        .unwrap_or(false);
    let Some(return_ty) = method_signature(ctx, object, method)
        .map(|signature| normalize_value_php_type(signature.return_type))
    else {
        if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
            return PhpType::Mixed;
        }
        return fallback_expr_type(expr);
    };
    let return_ty = builtin_datetime_magic_serialize_return_type(ctx, &object_ty, method)
        .unwrap_or(return_ty);
    let return_ty = if let Some(receiver_name) = method_receiver_object_class(&object_ty) {
        instance_method_late_static_return_for_ir(ctx, receiver_name, &php_symbol_key(method))
            .map(|return_type| late_static_return_type_for_ir(ctx, &return_type, receiver_name))
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

/// Preserves the known associative representation of inherited ext/date `__serialize()` hooks.
///
/// PHP declares these methods as returning generic `array`, but each internal implementation
/// always returns a string-keyed hash. EIR must keep that concrete storage shape so direct calls
/// do not reinterpret the hash header as a packed array. User overrides retain their declared
/// generic return because their runtime key shape is not known here.
fn builtin_datetime_magic_serialize_return_type(
    ctx: &LoweringContext<'_, '_>,
    receiver_type: &PhpType,
    method: &str,
) -> Option<PhpType> {
    let method_key = php_symbol_key(method);
    if method_key != "__serialize" {
        return None;
    }
    let receiver_name = method_receiver_object_class(receiver_type)?.trim_start_matches('\\');
    let class_info = ctx.classes.get(receiver_name)?;
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver_name)
        .trim_start_matches('\\');
    if !matches!(
        impl_class,
        "DateTime"
            | "DateTimeImmutable"
            | "DateTimeZone"
            | "DateInterval"
            | "DatePeriod"
    ) {
        return None;
    }
    Some(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
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
            Some(existing) if !method_argument_signatures_match(existing, &signature) => {
                return None;
            }
            Some(_) => {}
            None => common = Some(signature),
        }
    }
    common
}

/// Returns whether two dynamic method candidates accept the same PHP call arguments.
fn method_argument_signatures_match(left: &FunctionSig, right: &FunctionSig) -> bool {
    left.params == right.params
        && left.param_type_exprs == right.param_type_exprs
        && left.param_attributes == right.param_attributes
        && left.defaults == right.defaults
        && left.ref_params == right.ref_params
        && left.declared_params == right.declared_params
        && left.variadic == right.variadic
}

/// Returns true when an instance-method receiver has no single compile-time class.
pub(super) fn dynamic_method_receiver_needs_mixed_fallback(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Mixed | PhpType::Object(_))),
        _ => false,
    }
}
