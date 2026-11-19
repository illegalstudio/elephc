//! Purpose:
//! Verifies logical alternative proofs before dynamic arithmetic dispatch.
//!
//! Called from:
//! - Staged Mixed policy checker and EIR regression tests.
//!
//! Key details:
//! - Numeric unions remain closed while actual mixed values require existing flow narrowing.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Numeric use rejects unknown or partially unsupported alternatives before backend dispatch.
#[test]
fn strict_mixed_numeric_operands_require_all_alternatives() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for source in ["<?php function f(mixed $value):mixed{return $value+1;}",
        "<?php function f(mixed $value):mixed{return 1/$value;}",
        "<?php function f(mixed $value):mixed{return $value&1;}",
        "<?php function f(mixed $value):mixed{$value++;return $value;}",
        "<?php function f(int|array $value):mixed{return $value+1;}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-mixed"), "{source}: {error:?}");
    }
}

/// Successful guard facts and finite arithmetic joins lower on all five supported targets.
#[test]
fn strict_mixed_narrowed_numeric_chains_emit_on_every_target() {
    let source = include_str!("../../../examples/strict-mixed/main.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Numeric chains retain values and owners independently of arithmetic and overflow flags.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_mixed_narrowed_numeric_chains_preserve_values_and_owners() {
    let source = include_str!("../../../examples/strict-mixed/main.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "6:0:3.5", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }
}

/// Ordinary receivers require a complete proof while quiet probes and narrowed arrays stay usable.
#[test]
fn strict_mixed_receivers_require_all_alternatives() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for source in ["<?php function f(mixed $value):mixed{return $value[0];}",
        "<?php function f(array|false $value):mixed{return $value[0];}",
        "<?php function f(mixed $value):mixed{return $value->value;}",
        "<?php function f(mixed $value):mixed{return $value?->value;}",
        "<?php function f(array $value,mixed $key):mixed{return $value[$key];}",
        "<?php function f(array $value,mixed $key):bool{return isset($value[$key]);}",
        "<?php class C{public int $value=7;}function f(C $value,mixed $key):mixed{return $value->{$key};}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-mixed"), "{source}: {error:?}");
    }
    for source in ["<?php function f(mixed $value):mixed{return $value[0]??null;}",
        "<?php function f(mixed $value):bool{return isset($value->value);}",
        "<?php function f(mixed $value):mixed{if(is_array($value)){return $value[0];}return null;}",
        "<?php function f(array|string $value):mixed{return $value[0];}",
        "<?php function f(array $value,int|string $key):mixed{return $value[$key];}",
        "<?php function f(mixed $value):mixed{if(is_int($value)){$value++;return $value;}return null;}",
        "<?php class C{public int $value=7;}function f(?C $value):mixed{return $value?->value;}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Declared and builtin boundaries require narrowed alternatives without enabling exact conversions.
#[test]
fn strict_mixed_declared_boundaries_require_narrowing() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for source in ["<?php function target(string $value):void{}function f(mixed $value):void{target($value);}",
        "<?php function f(mixed $value):string{return $value;}",
        "<?php function f(mixed $value):int{return count($value);}",
        "<?php function f(mixed $value):mixed{return -$value;}",
        "<?php function f(mixed $value):string{return $value.'.';}",
        "<?php function f(mixed $value):mixed{return $value();}",
        "<?php function f(mixed $value):void{foreach($value as $item){echo gettype($item);}}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-mixed"), "{source}: {error:?}");
    }
    for source in ["<?php function f(mixed $value):mixed{return $value;}",
        "<?php function target(int|string $value):void{}function f(int|string $value):void{target($value);}",
        "<?php function f(mixed $value):string{if(is_string($value)){return $value;}return '';}",
        "<?php function f(mixed $value):int{return (int)$value;}",
        "<?php function f(mixed $value):string{return gettype($value);}",
        "<?php function f(callable $value):mixed{return $value();}",
        "<?php function f(mixed $value):mixed{if(is_callable($value)){return $value();}return null;}",
        "<?php function f(mixed $value):void{if(is_array($value)){foreach($value as $item){echo gettype($item);}}}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Unselected guarantees and trusted implementation code retain ordinary dynamic arithmetic.
#[test]
fn strict_mixed_checks_respect_independent_profiles_and_source_policy() {
    let source = "<?php function f(mixed $value):mixed{return $value+1;}";
    for (profile, mode) in [(SemanticProfile::PERMISSIVE, SourceMode::Php),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true), SourceMode::Php),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true), SourceMode::Internal)] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    }
}

/// Checked integer chains keep their proven result under independently combined numeric guarantees.
#[test]
fn strict_mixed_checked_integer_results_compose_on_every_target() {
    let source = "<?php function integerProof(int $value):int{return ($value+1)*2;}try{echo integerProof(3),'|';integerProof(PHP_INT_MAX);}catch(ArithmeticError $error){echo 'O';}";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true)
        .with(StrictFeature::Overflow, true).with(StrictFeature::Arithmetic, true)
        .with(StrictFeature::Coercions, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    #[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "8|O", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }
    let power_source = "<?php function powerProof(int $base,int $exponent):mixed{return $base**$exponent;}echo gettype(powerProof(2,-1));";
    let power_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true)
        .with(StrictFeature::Overflow, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(power_source, Target::parse(name).unwrap(), power_profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    #[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(power_source, Target::detect_host(), power_profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "double", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }

}

/// Nullsafe non-execution is separate from a real nullable value produced by a successful hop.
#[test]
fn strict_mixed_nullsafe_chains_preserve_successful_member_views() {
    let source = "<?php class MixedChainLeaf{public int $value=7;}class MixedChainRoot{public MixedChainLeaf $child;function __construct(){$this->child=new MixedChainLeaf();}}function selectMixedChain(?MixedChainRoot $value):mixed{return $value?->child->value;}echo gettype(selectMixedChain(null)),':',selectMixedChain(new MixedChainRoot());";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    #[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "NULL:7", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }
    let invalid = "<?php class L{public int $value=7;}class R{public ?L $child=null;}function f(?R $value):mixed{return $value?->child->value;}";
    let program = crate::parser::parse(&crate::lexer::tokenize(invalid).unwrap()).unwrap();
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("strict-mixed"), "{error:?}");
}

/// An array predicate proves the array family without assuming one packed or associative layout.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_mixed_array_guard_preserves_packed_and_hash_views() {
    let source = "<?php function mixedArrayGuard(mixed $value):mixed{if(is_array($value)){return $value[0];}return null;} \
        echo mixedArrayGuard([7]),':',mixedArrayGuard([0=>8,'other'=>9]),':',gettype(mixedArrayGuard(false));";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module); let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7:8:NULL", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }
}
