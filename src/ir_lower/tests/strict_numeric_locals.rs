//! Purpose:
//! Checks local value membership guards across the supported target matrix.
//!
//! Called from:
//! - Focused strict local EIR and code generation verification.
//!
//! Key details:
//! - A store contract retains Locals ownership without enabling arithmetic or overflow.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Every supported backend preserves value membership without enabling array element restrictions.
#[test]
fn strict_local_value_membership_emits_on_every_target() {
    let source = format!("{}\n{}", include_str!("../../../tests/fixtures/strict/scalar_binding_contracts.php"),
        include_str!("../../../tests/fixtures/strict/local_binding_members.php").strip_prefix("<?php").unwrap());
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        for expected in [crate::types::PhpType::Str, crate::types::PhpType::Bool,
            crate::types::PhpType::Void, crate::types::PhpType::Object("OriginalScalarBinding".into()),
            crate::types::PhpType::php_array()] {
            assert!(checked.local_store_contracts.values().any(|contract|
                contract.feature == StrictFeature::Locals && contract.ty == expected), "{name}: {expected}");
        }
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(!module.semantic_profile.contains(StrictFeature::ArrayTypes));
        assert!(!module.semantic_profile.contains(StrictFeature::Coercions));
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Numeric eval plans preserve existing binding origins without using future scalar frame slots.
#[test]
fn strict_numeric_eval_contracts_use_existing_binding_origins_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/native_eval_local_contract.php")
        .replace("echo gettype($owner)", "$future = 0; echo gettype($owner)");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert!(checked.local_store_contracts.iter().any(|((_, _, local), contract)|
            local == "owner" && contract.ty == crate::types::PhpType::Int
                && contract.feature == StrictFeature::Locals), "{name}: source-point integer origin");
        assert!(!checked.local_store_contracts.keys().any(|(_, _, local)| local == "future"),
            "{name}: a later zero-valued integer slot does not constrain earlier eval");
    }
}

/// All supported targets transport numeric contracts through the protected native reference ABI.
#[test]
fn strict_numeric_reference_callers_generate_exact_native_validators_on_every_target() {
    let source = format!("{}\n{}", include_str!("../../../tests/fixtures/strict/native_untyped_local_contract.php"),
        include_str!("../../../tests/fixtures/strict/numeric_reference_lifetimes.php").strip_prefix("<?php").unwrap());
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(module.required_runtime_features.native_reference_validation, "{name}");
        let callbacks = module.functions.iter().filter(|function|
            function.native_local_reference_contract.is_some()).collect::<Vec<_>>();
        assert_eq!(callbacks.len(), 4, "{name}: int, float, nullable int and explicit numeric union");
        for callback in callbacks {
            assert!(callback.signature.is_none());
            assert_eq!(callback.params.len(), 2);
            assert_eq!(callback.no_epilogue_cleanup_slots.len(), 2);
        }
        crate::ir::validate_module(&module).unwrap();
        let printed = crate::ir::print_module(&module);
        assert!(printed.contains("contract=int"), "{name}: explicit contract in textual EIR");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_reference_sources_bind_local"), "{name}");
        assert!(assembly.contains("__rt_reference_sources_unbind_local"), "{name}");
        let mut mismatched = module.clone();
        mismatched.semantic_profile = SemanticProfile::PERMISSIVE;
        assert!(matches!(crate::ir::validate_module(&mismatched),
            Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })), "{name}");
    }
}

/// Every target lowers catchable local guards after ordinary PHP numeric operations.
#[test]
fn strict_numeric_locals_guard_writes_and_updates_on_every_target() {
    let source = format!("{}\n{}", include_str!("../../../tests/fixtures/strict/numeric_local_contracts.php"),
        include_str!("../../../tests/fixtures/strict/numeric_local_rebinds.php").strip_prefix("<?php").unwrap());
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert!(checked.local_store_contracts.values().filter(|contract|
            contract.feature == StrictFeature::Locals).count() >= 4, "{name}");
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "numericBindings").unwrap();
        assert!(function.instructions.iter().filter(|instruction|
            instruction.op == crate::ir::Op::ThrowTypeError).count() >= 4, "{name}");
        assert!(!module.semantic_profile.contains(StrictFeature::Overflow));
        assert!(!module.semantic_profile.contains(StrictFeature::Coercions));
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}


/// Nullable candidate guards preserve raw reference storage and avoid narrowed-sink fatal updates.
#[test]
fn strict_nullable_numeric_locals_guard_before_reference_storage_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_numeric_local_contracts.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    for overflow in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
            .with(StrictFeature::Overflow, overflow);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            let function = module.functions.iter().find(|function| function.name == "updateNullableReference").unwrap();
            if !overflow {
                assert!(function.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::ThrowTypeError),
                    "{name}: nullable reference update lacks its local membership guard");
            }
            assert!(!function.instructions.iter().any(|instruction| matches!(instruction.op,
                crate::ir::Op::ICheckedAddToInt | crate::ir::Op::ICheckedSubToInt)), "{name}");
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Literal and runtime conversions preserve source contracts through every target's reference ABI.
#[test]
fn strict_settype_local_contracts_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/local_settype_contracts.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::object_cast_prelude::inject_if_used(program, &mut inventory).unwrap();
    let program = crate::name_resolver::resolve(program).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert!(checked.local_store_contracts.iter().filter(|((_, _, local), _)| local == "ordered").count() >= 3,
            "{name}: typed argument must preserve its caller contract: {:?}", checked.local_store_contracts);
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Literal eval publishes through scalar native sources while keeping its fully compiled scope runtime.
#[test]
fn strict_literal_eval_binding_contracts_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/literal_eval_binding_contracts.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(module.required_runtime_features.eval_scope, "{name}");
        assert!(!module.required_runtime_features.eval_bridge, "{name}: literal source remains AOT");
        let contracts = module.functions.iter().filter_map(|function| function.native_local_reference_contract.as_ref())
            .collect::<Vec<_>>();
        for contract in [crate::types::PhpType::Bool, crate::types::PhpType::Str,
            crate::types::PhpType::Float, crate::types::PhpType::Void] {
            assert!(contracts.contains(&&contract), "{name}: {contract}");
        }
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
