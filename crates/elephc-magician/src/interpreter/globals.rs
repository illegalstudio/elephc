//! Purpose: resolve GLOBALS element accesses against the actual global scope.
//! Called from: expression and writable-location evaluation.
//! Key details: globals never become local aliases or a private array snapshot.

use super::*;

/// Identifies the superglobal spelling without treating an ordinary local as special.
pub(super) fn is_globals_array(expr: &EvalExpr) -> bool {
    matches!(expr, EvalExpr::LoadVar(name) if name == "GLOBALS")
}

/// Evaluates a global-variable name once, using PHP's string conversion boundary.
pub(super) fn eval_global_name(
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let value = eval_expr(index, context, scope, values)?;
    let converted = eval_string_context_value(value, context, values);
    let result = match converted {
        Ok(string) => {
            let bytes = values.string_bytes(string);
            let released = if string != value {
                eval_release_value(context, values, string)
            } else {
                Ok(())
            };
            bytes.and_then(|bytes| {
                released?;
                String::from_utf8(bytes).map_err(|_| EvalStatus::UnsupportedConstruct)
            })
        }
        Err(status) => Err(status),
    };
    let released = if expressions::eval_expr_result_aliases_storage(index) {
        Ok(())
    } else {
        eval_release_value(context, values, value)
    };
    result.and_then(|name| { released?; Ok(name) })
}

/// Reads an independent value; the global slot keeps its own reference.
pub(super) fn read_global_value(
    name: &str,
    quiet: bool,
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let global = context.global_scope_ptr().ok_or(EvalStatus::UnsupportedConstruct)?;
    let current = scope as *const ElephcEvalScope as *mut ElephcEvalScope;
    let cell = if global == current {
        scope.visible_cell(name)
    } else {
        // The context's global-scope lifetime is established by its embedding frame.
        unsafe { global.as_ref() }.and_then(|global| global.visible_cell(name))
    };
    if let Some(cell) = cell {
        return values.copy_value(cell);
    }
    if !quiet && !context.errors_suppressed() {
        values.warning(&format!("Undefined global variable ${name}"))?;
    }
    values.null()
}

/// Transfers an owned cell to global storage, preserving existing reference aliases.
pub(super) fn store_global_value(
    name: String,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let global = context.global_scope_ptr().ok_or(EvalStatus::UnsupportedConstruct)?;
    let current = scope as *mut ElephcEvalScope;
    let replaced = if global == current {
        scope.set_respecting_references(name, value, ScopeCellOwnership::Owned)
    } else {
        unsafe { global.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?
            .set_respecting_references(name, value, ScopeCellOwnership::Owned)
    };
    for replaced in replaced {
        eval_release_value(context, values, replaced)?;
    }
    Ok(())
}

/// Removes the global symbol itself, rather than unbinding a same-named local.
pub(super) fn unset_global_value(
    name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let global = context.global_scope_ptr().ok_or(EvalStatus::UnsupportedConstruct)?;
    let current = scope as *mut ElephcEvalScope;
    let removed = if global == current {
        scope.unset_respecting_references(name)
    } else {
        unsafe { global.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?
            .unset_respecting_references(name)
    };
    if let Some(removed) = removed {
        eval_release_value(context, values, removed)?;
    }
    Ok(())
}
