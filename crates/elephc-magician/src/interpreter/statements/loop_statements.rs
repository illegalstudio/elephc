//! Purpose:
//! Executes static-local declarations, switch, and loop statement families.
//!
//! Called from:
//! - `crate::interpreter::statements::execute_stmt()`.
//!
//! Key details:
//! - Break/continue control, foreach array/object/iterator traversal, and key materialization are preserved.
//! - By-value foreach owns its evaluated receiver through every body exit and retires it afterward.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Executes a PHP `static $name = expr;` declaration in the current eval scope.
pub(in crate::interpreter) fn execute_static_var_stmt(
    name: &str,
    init: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(function_name) = context.current_function().map(str::to_string) else {
        let value = eval_expr(init, context, scope, values)?;
        let replaced = scope
            .set(name.to_string(), value, ScopeCellOwnership::Owned)
            .into_iter();
        crate::interpreter::scope_cells::retire_replaced_scope_cells(
            replaced, context, values,
        )?;
        return Ok(());
    };
    if scope.contains_visible(name) {
        return Ok(());
    }
    let value = if let Some(value) = context.static_local(&function_name, name) {
        value
    } else {
        let value = eval_expr(init, context, scope, values)?;
        let _ = context.set_static_local(function_name.clone(), name.to_string(), value);
        value
    };
    let replaced = scope
        .set(name.to_string(), value, ScopeCellOwnership::Borrowed)
        .into_iter();
    crate::interpreter::scope_cells::retire_replaced_scope_cells(
        replaced, context, values,
    )?;
    Ok(())
}

/// Executes a PHP switch with loose case matching, default fallback, and fallthrough.
pub(in crate::interpreter) fn execute_switch_stmt(
    expr: &EvalExpr,
    cases: &[EvalSwitchCase],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

    let subject = RuntimeOwnerSlot::new(eval_expr(expr, context, scope, values)?);
    with_runtime_owners(&subject, || {
        let executed = (|| {
            let mut default_index = None;
            let mut matched_index = None;
            for (index, case) in cases.iter().enumerate() {
                let Some(condition) = &case.condition else {
                    if default_index.is_none() {
                        default_index = Some(index);
                    }
                    continue;
                };
                let condition = RuntimeOwnerSlot::new(eval_expr(condition, context, scope, values)?);
                let compared = with_runtime_owners(&condition, || {
                    values.compare(
                        EvalBinOp::LooseEq,
                        subject.borrowed(),
                        condition.borrowed(),
                    )
                });
                let comparison = compared.map(RuntimeOwnerSlot::new);
                let matches = with_runtime_owners(&comparison.as_ref().ok(), || {
                    comparison
                        .as_ref()
                        .map_err(|status| *status)
                        .and_then(|comparison| values.truthy(comparison.borrowed()))
                });
                let comparison_cleanup = comparison
                    .as_ref()
                    .ok()
                    .map_or(Ok(()), |comparison| {
                        release_expr_result(comparison.take(), context, values)
                    });
                let condition_cleanup =
                    release_expr_result(condition.take(), context, values);
                let matches = matches
                    .and_then(|matches| comparison_cleanup.map(|()| matches))
                    .and_then(|matches| condition_cleanup.map(|()| matches))?;
                if matches {
                    matched_index = Some(index);
                    break;
                }
            }
            let Some(start_index) = matched_index.or(default_index) else {
                return Ok(EvalControl::None);
            };
            for case in &cases[start_index..] {
                match execute_statements(&case.body, context, scope, values)? {
                    EvalControl::None => {}
                    EvalControl::Break | EvalControl::Continue => break,
                    EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
                    EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                    EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
                }
            }
            Ok(EvalControl::None)
        })();
        let cleanup = with_runtime_owners(&executed.as_ref().ok(), || {
            release_expr_result(subject.take(), context, values)
        });
        match (executed, cleanup) {
            (Err(status), _) | (_, Err(status)) => Err(status),
            (Ok(control), Ok(())) => Ok(control),
        }
    })
}

/// Executes a PHP `do/while` loop, evaluating the condition after every body run.
pub(in crate::interpreter) fn execute_do_while_stmt(
    body: &[EvalStmt],
    condition: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    loop {
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
            EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
        }
        if !eval_condition(condition, context, scope, values)? {
            break;
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `for` loop while preserving update-on-continue semantics.
pub(in crate::interpreter) fn execute_for_stmt(
    init: &[EvalStmt],
    condition: Option<&EvalExpr>,
    update: &[EvalStmt],
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match execute_statements(init, context, scope, values)? {
        EvalControl::None | EvalControl::Continue => {}
        EvalControl::Break => return Ok(EvalControl::None),
        EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
        EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
        EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
    }
    loop {
        if let Some(condition) = condition {
            if !eval_condition(condition, context, scope, values)? {
                break;
            }
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
            EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
            EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `foreach` loop over eval array and Traversable object values.
pub(in crate::interpreter) fn execute_foreach_stmt(
    array: &EvalExpr,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let array = RuntimeOwnerSlot::new(eval_owned_expr(array, context, scope, values)?);
    let result = with_runtime_owners(&array, || match values.type_tag(array.borrowed())? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            execute_foreach_array_stmt(
                array.borrowed(),
                key_name,
                value_name,
                body,
                context,
                scope,
                values,
            )
        }
        EVAL_TAG_OBJECT => execute_foreach_object_stmt(
            array.borrowed(),
            key_name,
            value_name,
            body,
            context,
            scope,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    });
    let released = with_runtime_owners(&result.as_ref().ok(), || {
        eval_release_value(context, values, array.take())
    });
    match (result, released) {
        (Ok(control), Ok(())) => Ok(control),
        (Err(status), _) => Err(status),
        (Ok(control), Err(status)) => {
            if let EvalControl::Return(value) | EvalControl::Throw(value) = control {
                let _ = eval_release_value(context, values, value);
            }
            Err(status)
        }
    }
}

/// Executes `foreach` over a PHP array value using insertion-order runtime hooks.
pub(super) fn execute_foreach_array_stmt(
    array: RuntimeCellHandle,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let len = values.array_len(array)?;
    for index in 0..len {
        use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
        use elephc_memoir::runtime_owners::release_staged_cells;

        let key = RuntimeOwnerSlot::new(values.array_iter_key(array, index)?);
        let value = with_runtime_owners(&key, || values.array_iter_value(array, index))?;
        let value = RuntimeOwnerSlot::new(value);
        let control = with_runtime_owners(&(&key, &value), || {
            let operation = (|| {
                if let Some(key_name) = key_name {
                    let replaced = set_scope_cell(
                        context,
                        scope,
                        key_name.to_string(),
                        key.take(),
                        ScopeCellOwnership::Owned,
                        values,
                    )?;
                    crate::interpreter::scope_cells::retire_replaced_scope_cells(
                        replaced, context, values,
                    )?;
                } else {
                    values.release(key.take())?;
                }
                let replaced = set_scope_cell(
                    context,
                    scope,
                    value_name.to_string(),
                    value.take(),
                    ScopeCellOwnership::Owned,
                    values,
                )?;
                crate::interpreter::scope_cells::retire_replaced_scope_cells(
                    replaced, context, values,
                )?;
                execute_statements(body, context, scope, values)
            })();
            let cleanup = release_staged_cells(Ok(()), [key.take(), value.take()], |cell| {
                eval_release_value(context, values, cell)
            });
            operation.and_then(|control| cleanup.map(|()| control))
        })?;
        match control {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
            EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes `foreach` over an Iterator or IteratorAggregate object.
pub(super) fn execute_foreach_object_stmt(
    object: RuntimeCellHandle,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    if eval_foreach_object_is_a(object, "Iterator", context, values)? {
        return execute_foreach_iterator_stmt(
            object, key_name, value_name, body, context, scope, values,
        );
    }
    if eval_foreach_object_is_a(object, "IteratorAggregate", context, values)? {
        let iterator = RuntimeOwnerSlot::new(eval_method_call_result(
            object,
            "getIterator",
            Vec::new(),
            context,
            values,
        )?);
        let result = with_runtime_owners(&iterator, || match values.type_tag(iterator.borrowed())? {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => execute_foreach_array_stmt(
                iterator.borrowed(),
                key_name,
                value_name,
                body,
                context,
                scope,
                values,
            ),
            EVAL_TAG_OBJECT
                if eval_foreach_object_is_a(
                    iterator.borrowed(),
                    "Iterator",
                    context,
                    values,
                )? =>
            {
                execute_foreach_iterator_stmt(
                    iterator.borrowed(),
                    key_name,
                    value_name,
                    body,
                    context,
                    scope,
                    values,
                )
            }
            _ => Err(EvalStatus::RuntimeFatal),
        });
        let released = with_runtime_owners(&result.as_ref().ok(), || {
            eval_release_value(context, values, iterator.take())
        });
        return match (result, released) {
            (Ok(control), Ok(())) => Ok(control),
            (Err(status), _) => Err(status),
            (Ok(control), Err(status)) => {
                if let EvalControl::Return(value) | EvalControl::Throw(value) = control {
                    let _ = eval_release_value(context, values, value);
                }
                Err(status)
            }
        };
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Drives one Iterator object through PHP's `foreach` method-call sequence.
pub(super) fn execute_foreach_iterator_stmt(
    iterator: RuntimeCellHandle,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let result = eval_method_call_result(iterator, "rewind", Vec::new(), context, values)?;
    values.release(result)?;
    loop {
        let valid = eval_method_call_result(iterator, "valid", Vec::new(), context, values)?;
        let is_valid = values.truthy(valid)?;
        values.release(valid)?;
        if !is_valid {
            return Ok(EvalControl::None);
        }

        let value = eval_method_call_result(iterator, "current", Vec::new(), context, values)?;
        let key = if key_name.is_some() {
            Some(eval_method_call_result(
                iterator,
                "key",
                Vec::new(),
                context,
                values,
            )?)
        } else {
            None
        };
        if let Some((key_name, key)) = key_name.zip(key) {
            let replaced = set_scope_cell(
                context,
                scope,
                key_name.to_string(),
                key,
                ScopeCellOwnership::Owned,
                values,
            )?;
            crate::interpreter::scope_cells::retire_replaced_scope_cells(
                replaced, context, values,
            )?;
        }
        let replaced = set_scope_cell(
            context,
            scope,
            value_name.to_string(),
            value,
            ScopeCellOwnership::Owned,
            values,
        )?;
        crate::interpreter::scope_cells::retire_replaced_scope_cells(
            replaced, context, values,
        )?;

        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {
                let result =
                    eval_method_call_result(iterator, "next", Vec::new(), context, values)?;
                values.release(result)?;
            }
            EvalControl::Break => return Ok(EvalControl::None),
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
            EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
        }
    }
}

/// Shares iterator-interface detection between foreach and iterator builtins.
pub(in crate::interpreter) fn eval_foreach_object_is_a(
    object: RuntimeCellHandle,
    target: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(object, target, false, context, values)?
        .map_or_else(|| values.object_is_a(object, target, false), Ok)
}
