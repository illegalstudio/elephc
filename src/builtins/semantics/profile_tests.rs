//! Purpose:
//! Verifies semantic profile propagation through builtin checking, signatures and callable bodies.
//!
//! Called from:
//! - Focused compiler unit tests.
//!
//! Key details:
//! - An inline test-only builtin exposes the policy through its result kind and constant value.
//! - Generated public builtin callables must retain user contracts even inside Internal source.

use super::*;
use crate::builtins::spec::DefaultSpec;
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

builtin! {
    name: "__semantic_profile_probe",
    area: Types,
    params: [value: Mixed = DefaultSpec::Null],
    returns: Mixed,
    semantics: probe_semantics(),
    summary: "test-only semantic profile propagation probe",
    internal: false,
}

builtin! {
    name: "__prepared_semantic_profile_probe",
    area: Types,
    params: [],
    returns: Mixed,
    semantics: prepared_probe_semantics(),
    summary: "test-only prepared invocation contract probe",
    internal: false,
}

/// Exercises nested generated Return statements through the full lowering context.
const fn prepared_probe_semantics() -> BuiltinSemantics {
    let mut semantics = probe_semantics();
    semantics.callable = BuiltinCallablePolicy::Prepared(callable_accepts_any_source);
    semantics
}

/// Builds a test-only descriptor whose result exposes the selected source policy.
const fn probe_semantics() -> BuiltinSemantics {
    let mut semantics = test_probe_semantics();
    semantics.validation = BuiltinValidation::Shared(validate_probe);
    semantics.result_type = BuiltinResultType::Shared(probe_result_type);
    semantics.result_ownership = BuiltinResultOwnership::NonHeap;
    semantics.callable = BuiltinCallablePolicy::Dynamic(callable_accepts_any_source);
    semantics.lowering = BuiltinLowering::Eir(lower_probe);
    semantics
}

/// Accepts the probe's optional argument through the normal shared validation path.
fn validate_probe(_input: &BuiltinSemanticInput<'_>) -> Result<(), CompileError> { Ok(()) }

/// Distinguishes a selected user guarantee from the trusted implementation source policy.
fn user_policy(profile: SemanticProfile, mode: SourceMode) -> bool {
    profile.contains(StrictFeature::Bools) && mode != SourceMode::Internal
}

/// Selects a different ABI-visible kind so dropping profile metadata causes a real mismatch.
fn probe_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    if user_policy(input.semantic_profile, input.source_mode) { PhpType::Int } else { PhpType::Float }
}

/// Emits the actual module profile as a value using the independently supplied lowering context.
fn lower_probe(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let profile = ctx.semantic_profile();
    assert_eq!(profile, call.semantic_profile);
    let (op, immediate, ty) = if user_policy(profile, call.source_mode) {
        (Op::ConstI64, Immediate::I64(profile.bits() as i64), PhpType::Int)
    } else { (Op::ConstF64, Immediate::F64(profile.bits() as f64), PhpType::Float) };
    assert_eq!(&ty, call.result_type);
    Ok(ctx.emit_value(op, Vec::new(), Some(immediate), ty, op.default_effects(), Some(call.span)))
}

/// Public callable metadata uses user policy while direct implementation calls keep their own mode.
#[test]
fn strict_profile_builtin_signatures_preserve_source_policy_and_legacy_defaults() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
    let direct = crate::builtins::registry::function_sig_with_semantics("__semantic_profile_probe", profile, SourceMode::Internal).unwrap();
    let public = crate::builtins::registry::first_class_callable_sig_with_semantics("__semantic_profile_probe", profile).unwrap();
    let legacy = crate::builtins::registry::first_class_callable_sig("__semantic_profile_probe").unwrap();
    assert_eq!(direct.return_type, PhpType::Float);
    assert_eq!(public.return_type, PhpType::Int);
    assert_eq!(legacy.return_type, PhpType::Float);
}
