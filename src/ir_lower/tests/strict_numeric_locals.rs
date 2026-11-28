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
        let contracts = checked.local_store_contracts.values()
            .filter(|contract| contract.feature == StrictFeature::Locals)
            .map(|contract| contract.ty.clone())
            .chain(checked.local_store_contracts.origins.values().cloned())
            .collect::<Vec<_>>();
        for expected in [crate::types::PhpType::Str, crate::types::PhpType::Bool,
            crate::types::PhpType::Void, crate::types::PhpType::Object("OriginalScalarBinding".into()),
            crate::types::PhpType::php_array()] {
            assert!(contracts.contains(&expected), "{name}: {expected}; contracts={contracts:?}");
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
    let source = include_str!("../../../tests/fixtures/strict/native_eval_local_contract.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert!(checked.local_store_contracts.iter().any(|((_, _, local), contract)|
            local == "owner" && contract.ty == crate::types::PhpType::Int
                && contract.feature == StrictFeature::Locals), "{name}: source-point integer origin");
        let future = checked.local_store_contracts.iter().filter(|((_, _, local), _)|
            local == "future").collect::<Vec<_>>();
        assert!(matches!(future.as_slice(), [((_, span, _), contract)]
            if span.line == 27 && contract.ty == crate::types::PhpType::Int
                && contract.feature == StrictFeature::Locals),
            "{name}: only the post-eval native store keeps its predecessor contract: {future:?}");
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function|
            function.name == "futureNumericScope").unwrap();
        assert!(!function.instructions.iter().any(|inst| {
            inst.op == crate::ir::Op::EvalScopeBindLocalType
                && matches!(inst.immediate.as_ref(),
                    Some(crate::ir::Immediate::EvalScopeStore(metadata))
                    if module.data.global_names[metadata.name.as_raw() as usize] == "future")
        }), "{name}: the later native slot must not constrain the preceding eval");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
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
        for local in ["value", "compound", "prefix", "postfix"] {
            let guarded_store = checked.local_store_contracts.iter().any(|((_, _, candidate), contract)|
                candidate == local && contract.feature == StrictFeature::Locals);
            let guarded_origin = checked.local_store_contracts.origins.iter().any(|((_, _, candidate), contract)|
                candidate == local && contract == &crate::types::PhpType::Int);
            assert!(guarded_store || guarded_origin,
                "{name}: ${local} must retain its integer binding contract");
        }
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "numericBindings").unwrap();
        assert_eq!(function.instructions.iter().filter(|instruction|
            instruction.op == crate::ir::Op::ThrowTypeError).count(), 3,
            "{name}: compound, prefix, and postfix updates need direct guards: {}",
            crate::ir::print_function(function));
        assert!(module.required_runtime_features.native_reference_validation,
            "{name}: the ordinary assignment guard is carried by its checked reference store");
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
    let source = format!("{}\n{}", include_str!("../../../tests/fixtures/strict/literal_eval_binding_contracts.php"),
        include_str!("../../../tests/fixtures/strict/literal_eval_scope_string_owners.php").strip_prefix("<?php").unwrap());
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
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

/// All target backends preserve each source-order publication before the fragment's observable output.
#[test]
fn strict_literal_eval_eager_publication_emits_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/literal_eval_eager_publication.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(!module.required_runtime_features.eval_bridge, "{name}: source remains compiled");
        let fragments = module.functions.iter().filter(|function| function.instructions.iter()
            .any(|instruction| instruction.op == crate::ir::Op::EvalScopeSet)).collect::<Vec<_>>();
        assert_eq!(fragments.len(), 4, "{name}: identical fragments share their body across callers");
        for fragment in fragments {
            let writes = fragment.instructions.iter().enumerate().filter(|(_, instruction)|
                instruction.op == crate::ir::Op::EvalScopeSet).map(|(index, _)| index).collect::<Vec<_>>();
            assert_eq!(writes.len(), 2, "{name}: {}", fragment.name);
            let output = fragment.instructions.iter().position(|instruction| instruction.op == crate::ir::Op::EchoValue).unwrap();
            assert!(writes[0] < output, "{name}: the first assignment must precede following output");
        }
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// First-producer metadata is explicit, compact and shared independently of caller activation origins.
#[test]
fn strict_literal_eval_fresh_origins_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/eval_fresh_binding_origin.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(!module.required_runtime_features.eval_bridge, "{name}: pure compiled fragment");
        let contracts = module.functions.iter().flat_map(|function| &function.instructions)
            .filter_map(|inst| match &inst.immediate {
                Some(crate::ir::Immediate::EvalScopeStore(metadata)) => metadata.contract.as_ref(),
                _ => None,
            }).collect::<Vec<_>>();
        for ty in [crate::types::PhpType::Int, crate::types::PhpType::Str,
            crate::types::PhpType::Void, crate::types::PhpType::php_array()] {
            assert!(contracts.contains(&&ty), "{name}: first producer {ty}");
        }
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_eval_scope_set_with_contract"), "{name}");
        let mut invalid = module.clone();
        invalid.semantic_profile = profile.with(StrictFeature::Locals, false);
        assert!(matches!(crate::ir::validate_module(&invalid),
            Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })), "{name}");
    }
}

/// EIR binding state protects future and conditionally initialized native slots on every target.
#[test]
fn strict_eval_binding_presence_emits_on_every_target() {
    let source = format!("{}\n{}", include_str!("../../../tests/fixtures/strict/literal_eval_native_origin.php"),
        include_str!("../../../tests/fixtures/strict/literal_eval_suspended_scope.php").strip_prefix("<?php").unwrap());
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "conditionalOpenOrigin").unwrap();
        assert!(function.flags.tracks_local_binding_state, "{name}");
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::SetLocalBindingState
            && matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlot(slot))
                if function.locals[slot.as_raw() as usize].name.as_deref() == Some("value"))), "{name}");
        assert!(!module.required_runtime_features.eval_bridge, "{name}");
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("eval_scope_flush_uninitialized_or_unaccessed"), "{name}");
    }
}

/// Native re-entry shares canonical scope references before physical scalar storage or call binding.
#[test]
fn strict_eval_native_stores_share_scope_reference_storage_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/literal_eval_native_store_origin.php");
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "reverseLiteralOrigin").unwrap();
        let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("created")).unwrap();
        assert_eq!(slot.php_type, crate::types::PhpType::Mixed, "{name}: preserve the eval value's original kind");
        let owner = slot.ref_cell_owner.expect("native re-entry owns its shared canonical storage");
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::PromoteLocalRefCell
            && matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlotPair { first, second })
                if first == slot.id && second == owner)), "{name}");
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::StoreRefCell
            && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot.id))), "{name}");
        assert!(!function.instructions.iter().any(|inst| inst.op == crate::ir::Op::BindLocalRefContract
            && inst.immediate == Some(crate::ir::Immediate::LocalSlot(owner))
            && inst.result_php_type == crate::types::PhpType::Int), "{name}: no invented integer origin");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Checked native source types reach every eval backend without treating mixed frame slots as origins.
#[test]
fn strict_eval_native_producer_metadata_emits_on_every_target() {
    let source = "<?php function publishOrigin(int $input, mixed $open): void { eval('$copy=$input;'); } publishOrigin($argc, 1);";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "publishOrigin").unwrap();
        let contracts = function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::EvalScopeBindLocalType)
            .map(|inst| match inst.immediate.as_ref() {
                Some(crate::ir::Immediate::EvalScopeStore(metadata)) =>
                    (module.data.global_names[metadata.name.as_raw() as usize].as_str(), metadata.contract.as_ref()),
                _ => panic!("checked source contract requires explicit metadata"),
            }).collect::<Vec<_>>();
        assert!(contracts.contains(&("input", Some(&crate::types::PhpType::Int))), "{name}: {contracts:?}");
        assert!(!contracts.iter().any(|(name, _)| *name == "open"), "mixed has no invented exact origin");
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__elephc_eval_scope_binding_type_v1"), "{name}");
    }
}

/// Conditional source contracts use provenance guards so absent branches remain available to eval.
#[test]
fn strict_eval_conditional_native_origins_use_guarded_cfg_on_every_target() {
    let source = "<?php function conditionalMetadata(bool $bound): void { if($bound) {$value=1;} eval('$value=2.5;'); } conditionalMetadata($argc>0);";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "conditionalMetadata").unwrap();
        let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap().id;
        let guard = function.instructions.iter().find(|inst| inst.op == crate::ir::Op::GetNativeBindingState
            && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot))).expect("possible origin uses runtime provenance");
        assert!(function.blocks.iter().any(|block| matches!(&block.terminator,
            Some(crate::ir::Terminator::CondBr { cond, .. }) if Some(*cond) == guard.result)), "{name}");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Post-eval stores keep the original candidate and select old guards only for surviving native origins.
#[test]
fn strict_eval_current_origin_native_stores_emit_on_every_target() {
    let source = "<?php function currentOrigin(bool $bound, mixed $next): void { if($bound) {$value=1;} eval('$value=2.5;'); $value=$next; $value=3.5; $value=4.5; $fresh=3.5; } currentOrigin($argc>0,1);";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "currentOrigin").unwrap();
        let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap();
        assert_eq!(slot.php_type, crate::types::PhpType::Mixed, "{name}: original source kind remains boxed");
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::GetEvalBindingState
            && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot.id))), "{name}: replaced origins skip obsolete guards");
        assert!(function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::PromoteLocalRefCell
            && matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlotPair { first, .. }) if first == slot.id)).count() >= 2,
            "{name}: idempotent storage refresh handles the detached runtime predecessor");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
