//! Purpose:
//! Verifies strict contracts preserve PHP values declared callable after main integration.
//!
//! Called from:
//! - Focused AST-to-EIR profile and native ownership regressions.
//!
//! Key details:
//! - A declared callable can remain a string, array or object before invocation.

use crate::{codegen_support::platform::Target, source::SourceMode, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function profiledCallableReturn(callable $value): callable { return $value; }
function profiledCallableName(callable $value): string {
    if (is_string($value)) { return $value; }
    return 'other';
}
$value = profiledCallableReturn('strlen');
echo gettype($value), '|', $value === 'strlen' ? 'same' : 'bad', '|';
echo profiledCallableName($value), '|', $value('abc');
class ProfiledCallableProvider {
    public static function length(string $text): int { return strlen($text); }
    public function __invoke(string $text): int { return strlen($text); }
}
$array = profiledCallableReturn(['ProfiledCallableProvider', 'length']);
echo '|', gettype($array), '|', $array('xy');
$object = profiledCallableReturn(new ProfiledCallableProvider());
echo '|', gettype($object), '|', $object('x');
"#;

/// Selects independently enabled guarantees as well as the public umbrella.
fn profiles() -> [SemanticProfile; 5] {
    [SemanticProfile::PERMISSIVE,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true),
        SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true),
        SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true),
        SemanticProfile::all()]
}

/// Logical callable contracts admit their PHP kinds without treating them as descriptors.
#[test]
fn declared_callable_profile_contracts_preserve_logical_kinds() {
    use crate::semantics::{comparisons, conversions, mixed, numeric};
    use elephc_semantics::conversions::ValueKind;
    let ty = PhpType::DeclaredCallable;
    assert_eq!(numeric::value_kind(&ty), ValueKind::Dynamic);
    for alternative in [PhpType::Str, PhpType::php_array(), PhpType::Object(String::new())] {
        assert!(comparisons::identity_kinds_overlap(&ty, &alternative));
    }
    for alternative in [PhpType::Int, PhpType::Float, PhpType::Void] {
        assert!(!comparisons::identity_kinds_overlap(&ty, &alternative));
    }
    assert!(conversions::implicit_contract(&ty).is_some());
    assert!(conversions::runtime_constraints(&ty) == vec![conversions::RuntimeConstraint::Callable]);
    assert_eq!(conversions::inferred_binding_contract(&ty), Some(ty.clone()));
    let hint = crate::parser::ast::TypeExpr::Named("callable".into());
    assert_eq!(conversions::context_free_hint(&hint), Some(ty.clone()));
    assert_eq!(crate::types::constant_binding::declared_type(&hint), ty);
    assert_eq!(crate::types::constant_binding::declared_type(
        &crate::parser::ast::TypeExpr::Array(Box::new(hint))),
        PhpType::Array(Box::new(PhpType::Callable)));
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    mixed::validate_use(profile, SourceMode::Php, &ty, crate::span::Span::dummy(),
        "invocation", PhpType::is_callable_like).unwrap();
    assert!(mixed::validate_use(profile, SourceMode::Php, &ty, crate::span::Span::dummy(),
        "concatenation", |member| *member == PhpType::Str).is_err());
}

/// Callable parameters, returns and invocation emit on all supported targets.
#[test]
fn declared_callable_profiles_emit_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for profile in profiles() {
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile,
                false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Callable values preserve string identity and clean ownership with either optimizer setting.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn declared_callable_profiles_native_preserve_values_and_owners() {
    for profile in profiles() {
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile,
                optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "profile={profile:?} optimize={optimize}: {stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), "string|same|strlen|3|array|2|object|1", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
