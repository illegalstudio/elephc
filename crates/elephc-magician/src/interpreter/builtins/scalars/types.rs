//! Purpose:
//! Scalar casts, type names, object metadata, and type predicate builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::scalars` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and all PHP coercions flow through `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP scalar cast builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_cast(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_cast_result(name, value, context, values)
}

/// Dispatches an already evaluated value through the matching PHP cast hook.
pub(in crate::interpreter) fn eval_cast_result(
    name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "intval" => values.cast_int(value),
        "floatval" => values.cast_float(value),
        "strval" => {
            let value = eval_string_context_value(value, context, values)?;
            values.cast_string(value)
        }
        "boolval" => values.cast_bool(value),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `gettype(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
pub(in crate::interpreter) fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}

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
        if let Some(class) = context.dynamic_object_class(identity) {
            return values.string(class.name().trim_start_matches('\\'));
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
        if let Some(class) = context.dynamic_object_class(identity) {
            if let Some(parent) = context.class_parent_names(class.name()).into_iter().next() {
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

/// Evaluates PHP scalar/container type predicate builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_type_predicate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_type_predicate_result(name, value, context, values)
}

/// Converts a concrete runtime tag into a PHP `is_*` predicate result.
pub(in crate::interpreter) fn eval_type_predicate_result(
    name: &str,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = match name {
        "is_int" | "is_integer" | "is_long" => tag == EVAL_TAG_INT,
        "is_float" | "is_double" | "is_real" => tag == EVAL_TAG_FLOAT,
        "is_string" => tag == EVAL_TAG_STRING,
        "is_bool" => tag == EVAL_TAG_BOOL,
        "is_null" => tag == EVAL_TAG_NULL,
        "is_array" => matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC),
        "is_iterable" => eval_is_iterable_value(tag, value, context, values)?,
        "is_object" => tag == EVAL_TAG_OBJECT,
        "is_resource" => tag == EVAL_TAG_RESOURCE,
        "is_nan" => eval_float_value(value, values)?.is_nan(),
        "is_infinite" => eval_float_value(value, values)?.is_infinite(),
        "is_finite" => eval_float_value(value, values)?.is_finite(),
        "is_numeric" => {
            tag == EVAL_TAG_INT
                || tag == EVAL_TAG_FLOAT
                || (tag == EVAL_TAG_STRING && eval_is_numeric_string(&values.string_bytes(value)?))
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(result)
}

/// Returns PHP's `is_iterable()` result for arrays and Traversable-compatible objects.
fn eval_is_iterable_value(
    tag: u64,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(true);
    }
    if tag != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    for target in ["Traversable", "Iterator", "IteratorAggregate"] {
        if dynamic_object_is_a(value, target, false, context, values)?
            .map_or_else(|| values.object_is_a(value, target, false), Ok)?
        {
            return Ok(true);
        }
    }
    Ok(false)
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
