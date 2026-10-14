//! Purpose:
//! Executes local indexed and append writes against the live post-operand receiver.
//!
//! Called from:
//! - `statements::dispatch` for local array assignment statements.
//!
//! Key details:
//! - Index and RHS each run once before resolving the visible local and its storage kind.
//! - Array contracts, scalar errors, string offsets, and ArrayAccess share owner cleanup.

use super::*;

/// Appends to the receiver left in the local after evaluating the RHS.
pub(super) fn eval_array_append_var_stmt(
    name: &str, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_local_offset_write(name, None, value, context, scope, values)
}

/// Updates the receiver left in the local after evaluating the index and RHS.
pub(super) fn eval_array_set_var_stmt(
    name: &str, index: &EvalExpr, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_local_offset_write(name, Some(index), value, context, scope, values)
}

/// Dispatches a local write using live storage and releases every evaluated operand on all exits.
fn eval_local_offset_write(
    name: &str, index: Option<&EvalExpr>, value: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let mut result = (|| {
        let index = index.map(|index| eval_owned_expr(index, context, scope, values)).transpose()?;
        if let Some(index) = index { operands.push(index); }
        let value = eval_owned_expr(value, context, scope, values)?;
        operands.push(value);
        let current = visible_scope_cell(context, scope, name).map(|cell| values.retain(cell)).transpose()?;
        if let Some(current) = current { operands.push(current); }
        let tag = current.map(|cell| values.type_tag(cell)).transpose()?;
        if let Some(current) = current {
            if tag == Some(EVAL_TAG_OBJECT) {
                if !eval_array_access_object_matches(current, context, values)? {
                    let identity = values.object_identity(current)?;
                    let class_name = match context.dynamic_object_class_name(identity) {
                        Some(name) => name,
                        None => runtime_object_class_name(current, values)?,
                    };
                    return eval_throw_error(&format!("Cannot use object of type {class_name} as array"), context, values);
                }
                let index = match index {
                    Some(index) => index,
                    None => {
                        let index = values.null()?;
                        operands.push(index);
                        index
                    }
                };
                let returned = eval_method_call_result(current, "offsetSet", vec![index, value], context, values)?;
                operands.push(returned);
                return Ok(());
            }
            if let Some(index) = index {
                if tag == Some(EVAL_TAG_STRING) {
                    let replacement = eval_string_offset_write(current, index, value, context, values)?;
                    operands.push(replacement);
                    validate_generic_local_contracts(scope, name, replacement, context, values)?;
                    return write_back_owned_variable_ref_target(scope, name, replacement, context, values);
                }
            }
        }
        let array = match current {
            Some(cell) if matches!(tag, Some(EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)) => cell,
            _ => {
                let array = eval_array_autovivification(current, index.is_none(), context, values)?;
                operands.push(array);
                array
            }
        };
        let declared = generic_local_contracts(scope, name, context);
        let replacement = if let Some(index) = index {
            let index = match normalized_array_string_index(index, values)? {
                Some(normalized) => { operands.push(normalized); normalized }
                None => index,
            };
            for ty in &declared { validate_generic_local_element(Some(ty), array, index, value, context, values)?; }
            let replacement = eval_array_set_target_for_index(array, index, values)?;
            if replacement != array {
                operands.push(replacement);
                context.clone_array_element_aliases(array, replacement, None);
            }
            eval_array_element_reference_write(replacement, index, value, context, values)?;
            values.array_set(replacement, index, value)?;
            replacement
        } else {
            eval_array_append_owned_value_with_type(array, value, &declared, context, values)?
        };
        if visible_scope_cell(context, scope, name) == Some(replacement) { return Ok(()); }
        write_back_owned_variable_ref_target(scope, name, replacement, context, values)
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Allocates a normalized integer key only after the live receiver has selected array semantics.
fn normalized_array_string_index(
    index: RuntimeCellHandle, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.type_tag(index)? == EVAL_TAG_STRING {
        if let Some(key) = eval_numeric_string_array_key(&values.string_bytes(index)?) {
            return values.int(key).map(Some);
        }
    }
    Ok(None)
}
