//! Purpose:
//! Builds lazy Reflection object-default metadata from written constructor arguments.
//!
//! Called from:
//! - `reflection_parameter_default_value()` during callable metadata assembly.
//!
//! Key details:
//! - The shared call planner owns named binding and omitted constructor slots.
//! - Written names and order are separate from normalized runtime arguments.
//! - Resolving metadata never executes a constructor and retains the eight-slot subset.

use super::*;

/// Converts a top-level object parameter default into Reflection metadata.
pub(super) fn reflection_object_parameter_default_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Result<Option<ReflectionParameterDefaultValue>> {
    let ExprKind::NewObject { class_name, args: written_exprs } = &default.kind else {
        return Ok(None);
    };
    let Some(args) = reflection_object_parameter_default_args(
        ctx,
        current_class,
        current_info,
        class_name.as_str(),
        written_exprs,
    )?
    else {
        return Ok(None);
    };
    let mut written_args = Vec::with_capacity(written_exprs.len());
    for arg in written_exprs {
        let (name, expr) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (Some(name.clone()), value.as_ref()),
            _ => (None, arg),
        };
        let Some(value) = reflection_parameter_default_non_object_value(
            ctx, current_class, current_info, expr,
        )? else { return Ok(None); };
        written_args.push(ReflectionDefaultObjectArg { name, value });
    }
    Ok(Some(ReflectionParameterDefaultValue::Object {
        class_name: class_name.as_str().to_string(),
        args,
        written_args,
    }))
}

/// Returns constructor slots from the shared planner, resolving omitted values in their class.
fn reflection_object_parameter_default_args(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    class_name: &str,
    args: &[Expr],
) -> Result<Option<Vec<ReflectionParameterDefaultValue>>> {
    if args.len() > 8 {
        return Ok(None);
    }
    let Some((_, class_info)) = resolve_reflection_class(ctx, class_name) else {
        return args.iter().map(|arg| reflection_parameter_default_non_object_value(
            ctx, current_class, current_info, arg,
        )).collect::<Result<Option<Vec<_>>>>();
    };
    let Some(constructor) = class_info.methods.get(&php_symbol_key("__construct")) else {
        return Ok(args.is_empty().then(Vec::new));
    };
    if constructor.variadic.is_some()
        || args.len() > constructor.params.len()
        || constructor.params.len() > 8
    {
        return Ok(None);
    }
    let Ok(plan) = crate::types::call_args::plan_call_args(
        constructor, args, crate::span::Span::synthetic(), false, false,
    ) else { return Ok(None); };
    if plan.has_spread_args() {
        return Ok(None);
    }
    let default_slots = plan.possible_default_argument_mask(constructor);
    let mut normalized = plan.normalized_args();
    for default in constructor.defaults.iter().skip(normalized.len()) {
        let Some(default) = default else { return Ok(None); };
        normalized.push(default.clone());
    }
    if normalized.len() != constructor.params.len() {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(normalized.len());
    for (index, arg) in normalized.iter().enumerate() {
        let (scope, info) = if default_slots.get(index).copied().unwrap_or(false) {
            (class_name, Some(class_info))
        } else { (current_class, current_info) };
        let Some(value) = reflection_parameter_default_non_object_value(ctx, scope, info, arg)?
        else { return Ok(None); };
        values.push(value);
    }
    Ok(Some(values))
}
