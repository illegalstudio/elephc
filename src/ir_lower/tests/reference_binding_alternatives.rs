//! Purpose:
//! Checks executed first-producer contracts on locals exposed to reference-taking calls.
//!
//! Called from:
//! - Focused strict-local checker and native codegen regressions.
//!
//! Key details:
//! - Owned scalar alternatives receive canonical reference cells before escaping their frame.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/reference_binding_alternatives.php");
const CHECKED: &str = "Tinteger:1|Tstring:before|Winteger:2|Wstring:after|Tinteger:1|Tstring:before|Tinteger:1|Tstring:before|Tinteger:1|Tstring:before|Tinteger:1|Tdouble:2.5|Wdouble:3.5|Wstring:after|Tinteger:1|Tinteger:1|Tstring:before|Winteger:2|";

/// Every supported target retains first producers on owned reference-exposed frame bindings.
#[test]
fn reference_binding_alternatives_emit_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Reference writes preserve executed producer contracts and drain native reference owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reference_binding_alternatives_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// Dynamic call exposure cannot turn nullable pointer arguments into boxed null payloads.
#[test]
fn reference_origin_scan_preserves_nullable_pointer_call_storage() {
    let source = "<?php packed class PointerRecord186 {public int $value;} function preservePointer186(?ptr<PointerRecord186> &$target):void {} $reference=preservePointer186(...); ?ptr<PointerRecord186> $source=null; $reference($source); echo $source===null?'null|':'bad|'; $read=function(?ptr<PointerRecord186> $target):bool{return $target===null;}; $input=null; echo $read($input)?'value-null|':'bad-value|';";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(source, Target::parse(target).unwrap(), profile, false, SourceMode::Lfc);
        let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
        for name in ["source", "input"] {
            let local = main.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            assert_ne!(local.php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}: {name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        if module.target == Target::detect_host() {
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), "null|value-null|", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}

/// Exposure cannot erase a statically impossible store to an already established origin.
#[test]
fn reference_binding_alternatives_keep_known_invalid_store_diagnostics() {
    let declarations = "<?php function exposeOrigin(mixed &$value):void {} ";
    for body in [
        "function invalid(bool $pick):void {if($pick){$value=1;}else{$value='s';} exposeOrigin($value);$value=2.5;}",
        "function invalid(bool $pick):void {if($pick){$value=1;$value='bad';}exposeOrigin($value);}",
    ] {
        let source = format!("{declarations}{body}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals"), "{error:?}");
    }
}
