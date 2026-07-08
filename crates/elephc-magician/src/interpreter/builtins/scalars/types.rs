//! Purpose:
//! Shared scalar type helpers plus class, object, and resource introspection builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::scalars` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and shared PHP coercions flow through
//!   `RuntimeValueOps`.

use super::super::super::*;

/// Evaluates PHP's `get_called_class()` against the current eval method scope.
pub(in crate::interpreter) fn eval_builtin_get_called_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_get_called_class_result(context, values)
}

/// Returns the current late-static-bound class name or throws PHP's class-scope error.
pub(in crate::interpreter) fn eval_get_called_class_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = context
        .current_called_class_scope()
        .or_else(|| context.current_class_scope())
    else {
        return eval_throw_error(
            "get_called_class() must be called from within a class",
            context,
            values,
        );
    };
    values.string(class_name.trim_start_matches('\\'))
}

/// Evaluates PHP's `get_class(...)` over one eval object expression.
pub(in crate::interpreter) fn eval_builtin_get_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_get_class_no_arg_result(context, values),
        [object] => {
            let object = eval_expr(object, context, scope, values)?;
            eval_get_class_result(object, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves PHP's deprecated no-arg `get_class()` form from the current class scope.
pub(in crate::interpreter) fn eval_get_class_no_arg_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = context.current_class_scope() else {
        return eval_throw_error(
            "get_class() without arguments must be called from within a class",
            context,
            values,
        );
    };
    values.string(class_name.trim_start_matches('\\'))
}

/// Resolves the PHP-visible class name for one already materialized object cell.
pub(in crate::interpreter) fn eval_get_class_result(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class_name) = context.dynamic_object_class_name(identity) {
            return values.string(&class_name);
        }
    }
    values.object_class_name(object)
}

/// Evaluates PHP's SPL object identity builtins over one eval object expression.
pub(in crate::interpreter) fn eval_builtin_spl_object_identity(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_spl_object_identity_result(name, object, values)
}

/// Returns the unboxed object-payload identity in the native SPL builtin spelling.
pub(in crate::interpreter) fn eval_spl_object_identity_result(
    name: &str,
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)? as i64;
    match name {
        "spl_object_id" => values.int(identity),
        "spl_object_hash" => values.string(&identity.to_string()),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `get_parent_class(...)` over one eval object or class-name expression.
pub(in crate::interpreter) fn eval_builtin_get_parent_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_get_parent_class_no_arg_result(context, values),
        [object_or_class] => {
            let object_or_class = eval_expr(object_or_class, context, scope, values)?;
            eval_get_parent_class_result(object_or_class, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves PHP's deprecated no-arg `get_parent_class()` form from the current class scope.
pub(in crate::interpreter) fn eval_get_parent_class_no_arg_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = context.current_class_scope() else {
        return values.string("");
    };
    let class_name = values.string(class_name.trim_start_matches('\\'))?;
    eval_get_parent_class_result(class_name, context, values)
}

/// Resolves the PHP-visible parent class name for one object or class-name cell.
pub(in crate::interpreter) fn eval_get_parent_class_result(
    object_or_class: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object_or_class) {
        if let Some(class_name) = context.dynamic_object_class_name(identity) {
            if let Some(parent) = context.class_parent_names(&class_name).into_iter().next() {
                return values.string(&parent);
            }
            return values.string("");
        }
    }
    if values.type_tag(object_or_class)? == EVAL_TAG_STRING {
        let name = values.string_bytes(object_or_class)?;
        let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
        if context.class(&name).is_some() {
            if let Some(parent) = context.class_parent_names(&name).into_iter().next() {
                return values.string(&parent);
            }
            return values.string("");
        }
    }
    values.parent_class_name(object_or_class)
}

/// Evaluates `get_resource_type(...)` and `get_resource_id(...)` over one eval value.
pub(in crate::interpreter) fn eval_builtin_resource_introspection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [resource] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let resource = eval_expr(resource, context, scope, values)?;
    eval_resource_introspection_result(name, resource, values)
}

/// Evaluates a materialized resource introspection builtin argument.
pub(in crate::interpreter) fn eval_resource_introspection_result(
    name: &str,
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    match name {
        "get_resource_type" => values.string("stream"),
        "get_resource_id" => values.cast_int(resource),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Returns the PHP-visible type name for a concrete eval runtime tag.
pub(in crate::interpreter) fn eval_gettype_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "integer",
        EVAL_TAG_FLOAT => "double",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "boolean",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "NULL",
        _ => "NULL",
    }
}

/// Matches the static backend's legacy ASCII numeric-string scan.
pub(in crate::interpreter) fn eval_is_numeric_string(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    let mut consumed_digits = 0;
    if bytes[index] == b'-' {
        index += 1;
        if index >= bytes.len() {
            return false;
        }
    }

    while index < bytes.len() {
        if bytes[index] == b'.' {
            index += 1;
            break;
        }
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    consumed_digits > 0
}
