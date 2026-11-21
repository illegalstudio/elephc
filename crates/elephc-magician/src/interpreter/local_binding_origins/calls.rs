//! Purpose:
//! Selects immutable call source metadata using PHP namespace fallback priorities.
//!
//! Called from:
//! - Assignment producers and observed builtin predicate source views.
//!
//! Key details:
//! - Known untyped declarations open Locals bindings but remain unknown source evidence.
//! - A shadowing user function never inherits a global builtin predicate identity.

use super::*;
use std::borrow::Cow;

/// Chooses the namespace target or global fallback without evaluating PHP operands.
fn selected_name<'a>(name: &'a str, fallback: Option<&'a str>, context: &ElephcEvalContext) -> Cow<'a, str> {
    let name = call_lookup_name(name.trim_start_matches('\\'));
    let primary = context.function(&name).is_some() || context.native_function(&name).is_some()
        || split_generic_name(&name).ok().is_some_and(|(base, _)| context.generic_function_template(&base).is_some());
    if primary { return name; }
    fallback.map(|fallback| call_lookup_name(fallback.trim_start_matches('\\'))).unwrap_or(name)
}

/// Reads the selected return declaration while preserving unknown and explicit mixed roles.
pub(in crate::interpreter) fn producer(name: &str, fallback: Option<&str>,
    context: &ElephcEvalContext, role: ProducerContractRole,
) -> Option<EvalParameterType> {
    let name = selected_name(name, fallback, context);
    let source = if let Some(contract) = elephc_builtin_contract::lookup(&name) {
        Some(parameter_type(&builtin_producer_type(contract.returns)))
    } else if let Some(function) = context.function(&name) {
        function.return_type().cloned().or_else(|| role.opens_unknown(context)
            .then(|| parameter_type(&Type::atom("mixed"))))
    } else if let Some(function) = context.native_function(&name) {
        function.return_type().cloned().or_else(|| role.opens_unknown(context)
            .then(|| parameter_type(&Type::atom("mixed"))))
    } else { None };
    source.map(|ty| if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
        || context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { ty }
        else { parameter_type(&binding_kind_type(generic_type(&ty))) })
}

/// Identifies only the actual builtin selected after namespace shadowing and fallback.
pub(in crate::interpreter) fn builtin(name: &str, fallback: Option<&str>,
    context: &ElephcEvalContext,
) -> Option<&'static elephc_builtin_contract::BuiltinContract> {
    let name = selected_name(name, fallback, context);
    if context.function(&name).is_some() || context.native_function(&name).is_some() { return None; }
    elephc_builtin_contract::lookup(&name)
}
