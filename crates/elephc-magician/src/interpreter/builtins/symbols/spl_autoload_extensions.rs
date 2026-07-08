//! Purpose:
//! Eval registry entry and implementation for `spl_autoload_extensions`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - The extension list is eval-local mutable state on the context.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "spl_autoload_extensions",
    area: Symbols,
    params: [file_extensions = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_extensions(...)` calls.
pub(in crate::interpreter) fn eval_spl_autoload_extensions_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_autoload_extensions(args, context, scope, values)
}

/// Evaluates materialized `spl_autoload_extensions(...)` arguments.
pub(in crate::interpreter) fn eval_spl_autoload_extensions_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_extensions_result(evaluated_args, context, values)
}

/// Evaluates `spl_autoload_extensions()`.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_extensions(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = match args {
        [] => Vec::new(),
        [extensions] => vec![eval_expr(extensions, context, scope, values)?],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_spl_autoload_extensions_result(&evaluated_args, context, values)
}

/// Evaluates materialized `spl_autoload_extensions()` arguments.
pub(in crate::interpreter) fn eval_spl_autoload_extensions_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => {}
        [extensions] if values.type_tag(*extensions)? == EVAL_TAG_NULL => {}
        [extensions] => {
            let extensions = values.string_bytes(*extensions)?;
            let extensions = String::from_utf8(extensions).map_err(|_| EvalStatus::RuntimeFatal)?;
            context.set_spl_autoload_extensions(extensions);
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    values.string(context.spl_autoload_extensions())
}
