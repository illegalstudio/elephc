//! Purpose:
//! Retains explicit numeric policies for shared sum and product builtin lowering.
//!
//! Called from:
//! - The array_sum and array_product builtin homes.
//!
//! Key details:
//! - Direct and generated callable paths consume the same typed runtime inventory.
//! - Internal implementation calls retain PHP-promoting targets.

use crate::builtins::semantics::{BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, LoweredBuiltinValue, NormalizedBuiltinCall};
use crate::ir::{RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Declares an aggregate runtime identity once while attaching policy-aware EIR lowering.
pub(super) const fn semantics(target: RuntimeFnId) -> BuiltinSemantics {
    let mut semantics = crate::builtins::semantics::runtime_fn_semantics(target);
    semantics.lowering = BuiltinLowering::Eir(lower);
    semantics.target_strategy = BuiltinTargetStrategy::EirGraph;
    semantics.result_type = crate::builtins::semantics::BuiltinResultType::Shared(shared_result_type);
    semantics
}

/// Keeps numeric result alternatives independent from the aggregate's boxed storage representation.
pub(super) fn result_type(profile: elephc_semantics::SemanticProfile, source: crate::source::SourceMode) -> PhpType {
    crate::semantics::mixed::numeric_result(profile, source)
}

/// Shares direct, first-class and descriptor result metadata without specializing on current values.
fn shared_result_type(input: &crate::builtins::semantics::BuiltinSemanticInput<'_>) -> PhpType {
    result_type(input.semantic_profile, input.source_mode)
}

/// Selects a reviewed numeric aggregate target after ordinary shared argument planning.
fn lower(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>)
    -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let inventory = crate::builtins::registry::lookup(call.name)
        .map(|definition| definition.spec.semantics.runtime_functions);
    let Some(BuiltinRuntimeFunctions::One(target @ (RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct))) = inventory
        else { return Err(BuiltinLoweringError::new("numeric aggregate has no reviewed runtime target")); };
    let policy = crate::semantics::numeric::operation_policy(call.semantic_profile, call.source_mode);
    let runtime = if !policy.is_permissive() { RuntimeCallTarget::NumericFunction { target, policy } }
        else { RuntimeCallTarget::Function(target) };
    Ok(ctx.emit_runtime_call(runtime, call.operands.to_vec(), call.result_type.clone(), target.effects(), Some(call.span)))
}


/// Checks concrete element contracts while boxed alternatives retain the shared runtime guard.
pub(super) fn validate_elements(cx: &crate::builtins::spec::BuiltinCheckCtx<'_>, ty: &PhpType)
    -> Result<(), crate::errors::CompileError> {
    let Some(rule) = crate::semantics::numeric::operator_rule(cx.checker.semantic_profile, cx.source_mode,
        elephc_semantics::numeric_domains::NumericOperator::Arithmetic) else { return Ok(()); };
    let element = match ty {
        PhpType::Array(element) | PhpType::AssocArray { value: element, .. } => element.as_ref(),
        _ => return Ok(()),
    };
    if let PhpType::Object(name) = element {
        if name.is_empty() || name.eq_ignore_ascii_case("object") || cx.checker.interfaces.contains_key(name) { return Ok(()); }
        if crate::types::object_operators::cast_class_with_classes(element, &cx.checker.classes)
            .and_then(|class| crate::types::object_operators::cast_method(class, crate::types::object_operators::ObjectCast::Number)).is_some() {
            return Ok(());
        }
    }
    crate::semantics::numeric::validate_unary_type(rule, element, cx.span)
}
