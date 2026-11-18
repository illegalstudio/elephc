//! Purpose:
//! Proves builtin key inputs before source folding can erase original kinds.
//!
//! Called from:
//! - Shared constant and typed builtin source evidence.
//!
//! Key details:
//! - Builtin home metadata selects the parameter; shared planning owns names and spreads.

use super::*;
use crate::builtins::array_key_inputs::ArrayKeyInput;

/// Rejects a fully proven invalid key or key-array element using existing source evidence.
pub(super) fn validate(expr: &Expr, name: &str, args: &[Expr], input: ArrayKeyInput,
    checked: bool, class: Option<&str>, definitions: &Definitions, evidence: &mut Evidence,
) -> Result<(), CompileError> {
    let Some(args) = planned_builtin_arguments(name, args, expr.span, evidence.profile) else { return Ok(()); };
    let Some(arg) = args.get(input.argument()) else { return Ok(()); };
    let mut call_evidence = Evidence { profile: evidence.profile, active: evidence.active.clone(),
        strict_types: evidence.strict_types, ..Default::default() };
    let values = match input {
        ArrayKeyInput::Value { .. } => evaluate(arg, checked, class, definitions, &mut call_evidence)?.map(|value| vec![value]),
        ArrayKeyInput::Values { .. } => array_numbers(arg, checked, class, definitions, &mut call_evidence, None)?,
    };
    for value in values.into_iter().flatten() {
        use elephc_semantics::conversions::ValueKind;
        let ty = match value.value_kind() {
            ValueKind::Integer => crate::types::PhpType::Int, ValueKind::String => crate::types::PhpType::Str,
            ValueKind::Float => crate::types::PhpType::Float, ValueKind::Boolean => crate::types::PhpType::Bool,
            ValueKind::Null => crate::types::PhpType::Void,
            _ => crate::types::PhpType::Array(Box::new(crate::types::PhpType::Mixed)),
        };
        let ty = if matches!(input, ArrayKeyInput::Values { .. }) { crate::types::PhpType::Array(Box::new(ty)) } else { ty };
        input.validate_type(evidence.profile, &ty, arg.span)?;
    }
    Ok(())
}
