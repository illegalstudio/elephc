//! Purpose:
//! Evaluates array-element arguments and snapshots their writable PHP keys.
//!
//! Called from:
//! - `super::eval_call_arg_value()` for root and nested array lvalues.
//!
//! Key details:
//! - Persistent aliases keep Rust-owned integer/string keys, not native key leases.
//! - Offset and intermediate receiver owners retire on normal and exceptional exits.

use super::*;

/// Reads one element while preserving its storage target independently of key temporaries.
pub(super) fn eval_array_element_call_arg_value(
    array_expr: &EvalExpr,
    index_expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let (array, array_target, owns_array) = if let EvalExpr::LoadVar(name) = array_expr {
        let (array, owned) = match visible_scope_cell(context, scope, name) {
            Some(array) => (array, false),
            None => (values.null()?, true),
        };
        (array, Some(EvalReferenceTarget::Variable { scope, name: name.clone() }), owned)
    } else {
        let (array, target) = eval_call_arg_value(array_expr, context, scope, values)?;
        (array, target, true)
    };
    let mut index_owner = None;
    let result = (|| {
        let index = eval_owned_expr(index_expr, context, scope, values)?;
        index_owner = Some(index);
        let value = eval_array_get_result(array, index, context, values)?;
        let value = if value.is_borrowed() { values.retain(value)? } else { value };
        let target = (|| {
            if values.type_tag(array)? == EVAL_TAG_OBJECT { return Ok(None); }
            let Some(array_target) = array_target else { return Ok(None); };
            let key = eval_array_reference_key(index, values)?.ok_or(EvalStatus::RuntimeFatal)?;
            Ok(Some(EvalReferenceTarget::PersistentArrayElement {
                array_target: Box::new(array_target), key,
            }))
        })();
        match target {
            Ok(target) => Ok((value, target)),
            Err(status) => { let _ = release_expr_result(value, context, values); Err(status) },
        }
    })();
    let mut cleanup = Ok(());
    if let Some(index) = index_owner { cleanup = eval_release_value(context, values, index); }
    if owns_array {
        let released = release_expr_result(array, context, values);
        if cleanup.is_ok() { cleanup = released; }
    }
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok((value, _)), Err(status)) => {
            let _ = release_expr_result(value, context, values);
            Err(status)
        },
    }
}

/// Materializes one short-lived key for an operation on persistent reference metadata.
pub(super) fn materialize_array_reference_key(
    key: &EvalArrayReferenceKey,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match key {
        EvalArrayReferenceKey::Int(value) => values.int(*value),
        EvalArrayReferenceKey::String(bytes) => values.string_bytes_value(bytes),
    }
}
