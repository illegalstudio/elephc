//! Purpose:
//! Eval registry entry and implementation for `class_alias`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime aliases are stored in the eval context for eval declarations and
//!   generated/AOT class-like metadata.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "class_alias",
    area: Symbols,
    params: [r#class, alias, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `class_alias(class, alias, autoload?)` calls.
pub(in crate::interpreter) fn eval_class_alias_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_class_alias(args, context, scope, values)
}

/// Evaluates materialized `class_alias(...)` arguments.
pub(in crate::interpreter) fn eval_class_alias_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_alias_result(evaluated_args, context, values)
}

/// Evaluates `class_alias(class, alias, autoload?)` against eval and generated class tables.
pub(in crate::interpreter) fn eval_builtin_class_alias(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (class, alias) = match args {
        [class, alias] => (
            eval_expr(class, context, scope, values)?,
            eval_expr(alias, context, scope, values)?,
        ),
        [class, alias, autoload] => {
            let class = eval_expr(class, context, scope, values)?;
            let alias = eval_expr(alias, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            (class, alias)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_class_alias_result(&[class, alias], context, values)
}

/// Evaluates `class_alias(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_class_alias_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (class, alias) = match evaluated_args {
        [class, alias] => (*class, *alias),
        [class, alias, _autoload] => (*class, *alias),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let class = eval_class_alias_name(class, values)?;
    let alias = eval_class_alias_name(alias, values)?;
    if alias.is_empty()
        || context.resolve_class_like_name(&alias).is_some()
        || values.class_exists(&alias)?
        || eval_runtime_interface_exists(&alias, values)?
        || values.trait_exists(&alias)?
        || values.enum_exists(&alias)?
    {
        return values.bool_value(false);
    }
    let aliased = if context.resolve_class_like_name(&class).is_some() {
        context.define_class_alias(&class, &alias)
    } else if values.enum_exists(&class)? {
        context.define_external_enum_alias(&class, &alias)
    } else if values.class_exists(&class)? {
        context.define_external_class_alias(&class, &alias)
    } else if eval_runtime_interface_exists(&class, values)? {
        context.define_external_interface_alias(&class, &alias)
    } else if values.trait_exists(&class)? {
        context.define_external_trait_alias(&class, &alias)
    } else {
        false
    };
    values.bool_value(aliased)
}

/// Reads and normalizes one `class_alias()` class-name argument.
fn eval_class_alias_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(name.trim_start_matches('\\').to_string())
}
