//! Purpose:
//! Checks logical binding ends on owned locals in frames that expose their scope to eval.
//!
//! Called from:
//! - Focused strict-local checker and native codegen regressions.
//!
//! Key details:
//! - Old captured cells survive while a recreated name starts an independent producer contract.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/eval_binding_ends.php");
const CHECKED: &str = "1:string:after|Tinteger:1|Wstring:1|NULL:new|NULL:new|";

/// Every supported target carries logical ends across the shared native and interpreter scope.
#[test]
fn eval_binding_ends_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Conditional and reference-only births retain values and retire every scope owner.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn eval_binding_ends_native_values_and_owners() {
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

/// A future eval and unset cannot make a definitely incompatible earlier write legal.
#[test]
fn eval_binding_ends_preserve_known_store_diagnostics() {
    let source = "<?php function invalidEvalEnd():void {$value=1;$value='bad';unset($value);eval('');}";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("--strict-locals"), "{error:?}");
}

/// Isolates each eval binding-end family so retained owners identify their source transition.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn eval_binding_ends_isolated_native_values_and_owners() {
    let calls = ["evalFreshCapture();", "evalMaybeEnd(false, 'after');", "evalMaybeEnd(true, 'after');",
        "evalOnlyReferenceBirth();", "evalSingleProducer();"];
    let expected = ["1:string:after|", "Tinteger:1|", "Wstring:1|", "NULL:new|", "NULL:new|"];
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
        (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
        let mut failures = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            let mut source = SOURCE.to_string();
            for other in calls.iter().filter(|other| *other != call) {
                assert!(source.contains(other));
                source = source.replace(other, "");
            }
            let module = super::semantic_sources::lower(&source, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !output.status.success() || stdout != expected[index] || !stderr.contains("HEAP DEBUG: leak summary: clean") {
                eprintln!("case={index}: stdout={stdout}; {stderr}");
                failures.push(index);
            }
        }
        assert!(failures.is_empty(), "opt={optimize}, mode={mode:?}: failed cases={failures:?}");
    }
}
