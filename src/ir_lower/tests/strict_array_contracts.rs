//! Purpose:
//! Verifies stable source array contracts before indexed, append and replacement widening.
//!
//! Called from:
//! - Staged ArrayTypes checker and EIR regression tests.
//!
//! Key details:
//! - First informative writes and explicitly mixed/union elements retain their contracts.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Inferred arrays reject key/element changes, including fresh append-created arrays.
#[test]
fn strict_array_contracts_reject_inferred_key_and_element_changes() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    for source in ["<?php $items=[1];$items[]='changed';", "<?php $items=[1];$items[0]='changed';",
        "<?php $items=[1];$items['key']=2;", "<?php $items=['key'=>1];$items['other']='changed';",
        "<?php $items[]=1;$items[]='changed';", "<?php $items=[1];$items=['changed'];"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-array-types"), "{source}: {error:?}");
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), SemanticProfile::PERMISSIVE).unwrap();
    }
}

/// Empty arrays, explicit element alternatives and PHP's ordinary key normalization stay valid.
#[test]
fn strict_array_contracts_preserve_first_writes_and_explicit_open_elements() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    for source in ["$items=[];$items['key']=1;$items['other']=2;",
        "array<mixed> $items=[1];$items[]='changed';",
        "array<int|string> $items=[1];$items[]='changed';",
        "function f(array $items):array{$items[]='changed';return $items;}f([1]);",
        "$items=[1];$items=[];$items[]=2;",
        "$items=[1];$items=[2=>3];$items[3]=4;",
        "$items=[1];$items['0']=2;", "$items=[1];$copy=$items;$copy[]=2;",
        "$items=['key'=>1];array_splice($items,0,0,[]);",
        "$items=['key'=>1];array_splice($items,0,0,null);"] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&format!("<?php {source}")).unwrap(), SourceMode::Lfc).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Contract-preserving writes lower through the same EIR on every target.
#[test]
fn strict_array_contracts_emit_on_every_target() {
    let source = format!("<?php {}", include_str!("../../../examples/strict-array-types/main.lfc"));
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(), profile, false, SourceMode::Lfc);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Native writes preserve explicit mixed storage, values and copy ownership in both optimizer modes.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_array_contracts_preserve_native_values_and_owners() {
    let source = format!("<?php {}", include_str!("../../../examples/strict-array-types/main.lfc"));
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Lfc);
        let output = super::native_module::run(&module);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2|2", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }
}

/// Successful union writes keep the original declaration rather than widening it to mixed.
#[test]
fn strict_array_contracts_keep_explicit_unions_after_valid_writes() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    let source = "array<int|string> $items=[1];$items[]='valid';$items[]=2.5;";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&format!("<?php {source}")).unwrap(), SourceMode::Lfc).unwrap();
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("strict-array-types"), "{error:?}");
}

/// Mutating builtin homes and property writers share the original value and key contract.
#[test]
fn strict_array_contracts_reject_builtin_and_property_widening() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    for source in ["<?php $items=[1];array_push($items,'changed');",
        "<?php $items=[1];array_unshift($items,'changed');",
        "<?php $items=[1];array_splice($items,0,0,['changed']);",
        "<?php class C{public $items=[1];} $value=new C();$value->items[]='changed';",
        "<?php class C{public static $items=[1];} C::$items[]='changed';"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-array-types"), "{source}: {error:?}");
    }
}

/// Array contracts do not enable other flags or restrict trusted implementation writes.
#[test]
fn strict_array_contracts_respect_independent_profiles_and_trusted_sources() {
    let source = "<?php $items=[1];$items[]='changed';";
    for (profile, mode) in [(SemanticProfile::PERMISSIVE, SourceMode::Php),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true), SourceMode::Php),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true), SourceMode::Internal)] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    }
}

/// Nested stores validate the selected container before replacing its original elements.
#[test]
fn strict_array_contracts_reject_nested_element_changes() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    for source in ["<?php $items=[[1]];$items[0][0]='changed';",
        "<?php $items=['row'=>[1]];$items['row'][0]='changed';"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-array-types"), "{source}: {error:?}");
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), SemanticProfile::PERMISSIVE).unwrap();
    }
    for source in ["<?php $items=[[1]];$items[0][0]=2;",
        "<?php function nestedOpen(array $items):void{$items[0][0]='changed';}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}
