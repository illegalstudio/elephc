//! Purpose:
//! Preserves persistent reference identities in owned builtin argument evaluation.
//!
//! Called from:
//! - The common source-order call argument evaluator for shared runtime builtins.
//!
//! Key details:
//! - The neutral parameter contract decides passing mode for positional and named inputs.
//! - Reference owners pin the wrapper without retaining its previous PHP value separately.
//! - Unsupported lvalue representations fail before reaching a native reference callback.

use super::*;
use elephc_builtin_contract::BuiltinContract;

/// Resolves a source argument's passing mode without changing parameter binding or diagnostics.
pub(in crate::interpreter) fn by_reference(contract: Option<&BuiltinContract>, name: Option<&str>, position: usize) -> bool {
    let Some(contract) = contract else { return false; };
    let parameter = match name {
        Some(name) => contract.params.iter().find(|parameter| parameter.name == name),
        None => contract.params.get(position),
    };
    parameter.is_some_and(|parameter| parameter.by_ref)
}

/// Promotes a live variable or alias and returns an independent reference-wrapper owner.
pub(super) fn reference(
    expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let EvalExpr::LoadVar(name) = expr else { return Err(EvalStatus::UnsupportedConstruct); };
    let reference = eval_persistent_variable_reference(name, context, scope, values)?
        .ok_or(EvalStatus::UnsupportedConstruct)?;
    values.retain(reference)
}

/// Shares unpacking order while keeping explicit reference elements and detaching ordinary inputs.
pub(super) fn append_spread(
    contract: &BuiltinContract, spread: RuntimeCellHandle, evaluated: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    owners: &mut Vec<RuntimeCellHandle>,
) -> Result<(), EvalStatus> {
    let mut unpacked = Vec::new();
    append_unpacked_call_arg_values_with_owners(spread, &mut unpacked, saw_named, context, values, Some(owners))?;
    for argument in unpacked {
        if by_reference(Some(contract), argument.name.as_deref(), evaluated.len()) {
            if !values.is_reference(argument.value)? { return Err(EvalStatus::UnsupportedConstruct); }
            evaluated.push(argument);
        } else {
            let value = values.copy_value(argument.value)?;
            context.copy_array_element_aliases(argument.value, value);
            owners.push(value);
            evaluated.push(EvaluatedCallArg { value, ref_target: None, ..argument });
        }
    }
    Ok(())
}
