//! Purpose:
//! Verifies declared scalar returns retain their source and original value contracts.
//!
//! Called from:
//! - Focused selected-profile frontend and all-target lowering tests.
//!
//! Key details:
//! - Return-site provenance survives restoration of function and method checker scopes.

use elephc_semantics::{SemanticProfile, StrictFeature};
use crate::{source::SourceMode, codegen_support::platform::Target};

/// Functions, methods, closures and arrows reject proven implicit return conversions.
#[test]
fn strict_conversion_returns_frontend_source_sites_and_explicit_casts() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in [
        "<?php declare(strict_types=0); function f(): float { return 1; } f();",
        "<?php function f(): int { return true; } f();",
        "<?php function f(): string { return 1; } f();",
        "<?php function f(): bool { return 1; } f();",
        "<?php function f(): ?float { return 1; } f();",
        "<?php class C { public function f(): float { return 1; } } (new C())->f();",
        "<?php class C { public static function f(): float { return 1; } } C::f();",
        "<?php $f = function(): float { return 1; }; $f();",
        "<?php $f = fn(): float => 1; $f();",
    ] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("expected noncoercive return failure: {source}"));
            assert!(error.message.contains("strict-coercions"), "{mode:?}, {source}: {error:?}");
        }
    }
    let source = "<?php function f(): float { return (float) 1; } function g(): int|string { return 1; } echo f(); echo g();";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
}

/// Dynamic returns guard original tags before ABI extraction while trusted implementation returns stay permissive.
#[test]
fn strict_conversion_returns_guards_and_trusted_storage_on_all_targets() {
    let source = "<?php function returnFloat(mixed $v): float { return $v; } function returnMembers(mixed $v): int|false|null { return $v; } echo returnFloat(2.0); returnMembers(false);";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                for function in ["returnFloat", "returnMembers"] {
                    let function = module.functions.iter().find(|body| body.name == function).unwrap();
                    assert_eq!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::ThrowTypeError),
                        mode != SourceMode::Internal, "{name} {mode:?}, opt={optimize}");
                }
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}
