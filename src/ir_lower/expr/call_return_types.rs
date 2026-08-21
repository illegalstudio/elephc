//! Purpose:
//! Function-like call result-type normalization.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns the best available return type for a function-like call.
pub(in crate::ir_lower) fn call_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    _operands: &[crate::ir::ValueId],
) -> PhpType {
    let php_type = if let Some(sig) = ctx.functions.get(name) {
        eir_user_function_return_type(sig)
    } else if let Some(sig) = ctx.extern_functions.get(name) {
        sig.return_type.clone()
    } else if let Some(sig) = builtin_call_signature(name) {
        sig.return_type
    } else {
        PhpType::Mixed
    };
    normalize_value_php_type(php_type)
}

/// Returns the caller-visible EIR return type for a user function signature.
pub(super) fn eir_user_function_return_type(signature: &FunctionSig) -> PhpType {
    if signature.declared_return || !signature_has_dynamic_untyped_param(signature) {
        return signature.return_type.clone();
    }
    dynamic_param_container_return_type(&signature.return_type)
}

/// Returns true when a PHP signature has params that EIR must receive as Mixed.
pub(super) fn signature_has_dynamic_untyped_param(signature: &FunctionSig) -> bool {
    signature.params.iter().enumerate().any(|(index, (name, _))| {
        let declared = signature.declared_params.get(index).copied().unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        !declared && !by_ref && !variadic
    })
}

/// Widens inferred container return elements that may be built from dynamic params.
///
/// MUST STAY IN STEP with the callee-side copy in `crate::ir_lower::function`: this one
/// types the call site and that one types the body. When they disagree the caller reads
/// the callee's return in the wrong representation, with nothing to flag it.
pub(super) fn dynamic_param_container_return_type(return_type: &PhpType) -> PhpType {
    // A resource can only leave such a function BOXED — see the callee-side copy.
    if matches!(return_type, PhpType::Resource(_)) {
        return PhpType::Mixed;
    }
    match return_type.codegen_repr() {
        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Union(members) => PhpType::Union(
            members
                .iter()
                .map(dynamic_param_container_return_type)
                .collect(),
        ),
        other => other,
    }
}
