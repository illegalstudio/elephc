//! Purpose:
//! Symbol, constant, class, and language-construct builtin probes.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

pub(in crate::interpreter) fn eval_builtin_function_probe(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\').to_ascii_lowercase();
    values.bool_value(eval_function_probe_exists(context, &name))
}

/// Evaluates `define(name, value)` for eval dynamic constant-name registration.
pub(in crate::interpreter) fn eval_builtin_define(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let defined = eval_define_name(name, value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(name)` against eval dynamic constant names.
pub(in crate::interpreter) fn eval_builtin_defined(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let exists = eval_defined_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `define(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_define_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let defined = eval_define_name(*name, *value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_defined_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let exists = eval_defined_name(*name, context, values)?;
    values.bool_value(exists)
}

/// Normalizes and registers one eval dynamic constant name.
pub(in crate::interpreter) fn eval_define_name(
    name: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if name.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_predefined_constant_value(&name).is_some() || context.has_constant(&name) {
        values.warning(DEFINE_ALREADY_DEFINED_WARNING)?;
        return Ok(false);
    }
    let value = values.retain(value)?;
    if context.define_constant(&name, value) {
        Ok(true)
    } else {
        values.release(value)?;
        Ok(false)
    }
}

/// Normalizes and probes one eval dynamic constant name.
pub(in crate::interpreter) fn eval_defined_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    Ok(eval_predefined_constant_value(&name).is_some() || context.has_constant(&name))
}

/// Reads a PHP constant name from a runtime cell without changing case.
pub(in crate::interpreter) fn eval_constant_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates `class_exists(...)` against dynamic and generated class-name tables.
pub(in crate::interpreter) fn eval_builtin_class_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `class_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_class_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_class_exists_name(*name, context, values)?,
        [name, _autoload] => eval_class_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-name cell and probes dynamic names before generated classes.
pub(in crate::interpreter) fn eval_class_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if context.has_class(name) {
        return Ok(true);
    }
    values.class_exists(name)
}

/// Evaluates `interface_exists(...)` against generated interface-name metadata.
pub(in crate::interpreter) fn eval_builtin_interface_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_interface_exists_name(name, values)?;
    values.bool_value(exists)
}

/// Evaluates `interface_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_interface_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_interface_exists_name(*name, values)?,
        [name, _autoload] => eval_interface_exists_name(*name, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP interface-name cell and probes generated interface metadata.
pub(in crate::interpreter) fn eval_interface_exists_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.interface_exists(name.trim_start_matches('\\'))
}

/// Evaluates `trait_exists(...)` and `enum_exists(...)` against generated metadata.
pub(in crate::interpreter) fn eval_builtin_class_like_exists(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let symbol = match args {
        [symbol] => eval_expr(symbol, context, scope, values)?,
        [symbol, autoload] => {
            let symbol = eval_expr(symbol, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            symbol
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_like_exists_name(name, symbol, values)?;
    values.bool_value(exists)
}

/// Evaluates materialized `trait_exists(...)` or `enum_exists(...)` arguments.
pub(in crate::interpreter) fn eval_class_like_exists_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [symbol] => eval_class_like_exists_name(name, *symbol, values)?,
        [symbol, _autoload] => eval_class_like_exists_name(name, *symbol, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-like name cell and probes generated trait or enum metadata.
pub(in crate::interpreter) fn eval_class_like_exists_name(
    name: &str,
    symbol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let symbol = values.string_bytes(symbol)?;
    let symbol = String::from_utf8(symbol).map_err(|_| EvalStatus::RuntimeFatal)?;
    let symbol = symbol.trim_start_matches('\\');
    match name {
        "trait_exists" => values.trait_exists(symbol),
        "enum_exists" => values.enum_exists(symbol),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates `is_a(...)` and `is_subclass_of(...)` over eval boxed object cells.
pub(in crate::interpreter) fn eval_builtin_is_a_relation(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_is_a_relation_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized `is_a(...)` or `is_subclass_of(...)` builtin arguments.
pub(in crate::interpreter) fn eval_is_a_relation_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (object_or_class, target_class, allow_string) = match evaluated_args {
        [object_or_class, target_class] => {
            (*object_or_class, *target_class, name == "is_subclass_of")
        }
        [object_or_class, target_class, allow_string] => (
            *object_or_class,
            *target_class,
            values.truthy(*allow_string)?,
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let target_class = values.string_bytes(target_class)?;
    let target_class = String::from_utf8(target_class).map_err(|_| EvalStatus::RuntimeFatal)?;
    let target_class = target_class.trim_start_matches('\\');
    let is_object = values.type_tag(object_or_class)? == 6;
    let result =
        if is_object && dynamic_object_is_a(object_or_class, target_class, context, values)? {
            !matches!(name, "is_subclass_of")
        } else if is_object || allow_string {
            values.object_is_a(object_or_class, target_class, name == "is_subclass_of")?
        } else {
            false
        };
    values.bool_value(result)
}

/// Returns whether an eval-created object matches a dynamic class name exactly.
pub(in crate::interpreter) fn dynamic_object_is_a(
    object: RuntimeCellHandle,
    target_class: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let identity = values.object_identity(object)?;
    Ok(context
        .dynamic_object_class(identity)
        .is_some_and(|class| class.name().eq_ignore_ascii_case(target_class)))
}

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_builtin_isset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return values.bool_value(false);
    }
    for arg in args {
        if !eval_isset_arg(arg, context, scope, values)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_builtin_empty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = eval_empty_arg(arg, context, scope, values)?;
    values.bool_value(empty)
}

/// Evaluates one `empty` operand without warning or failing on missing variables.
pub(in crate::interpreter) fn eval_empty_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(true);
        };
        return Ok(!values.truthy(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates one `isset` operand without allocating a null cell for missing variables.
pub(in crate::interpreter) fn eval_isset_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(false);
        };
        return Ok(!values.is_null(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.is_null(value)?)
}

/// Returns true when a PHP function name is visible to eval builtin probes.
pub(in crate::interpreter) fn eval_function_probe_exists(
    context: &ElephcEvalContext,
    name: &str,
) -> bool {
    !name.contains("::") && (context.has_function(name) || eval_php_visible_builtin_exists(name))
}
