//! Purpose:
//! Verifies initialized-read boundaries beyond named frame and property access.
//!
//! Called from:
//! - Focused staged Initialization global retirement, dynamic and reflection tests.
//!
//! Key details:
//! - Removing program storage must preserve old escaped reference cells.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/initialized_extended_reads.php");
const EXPECTED: &str = "A:missing|I|old:new|I|AE:missing|I|I:A|I|D:I|";

/// All supported targets lower the same canonical removal and dynamic read operations.
#[test]
fn initialized_extended_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Canonical removal, dynamic and reflected reads preserve failures and old escaped values.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_extended_reads_native_values_and_owners() {
    for (initialization, locals) in [(true, false), (true, true), (false, false)] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, initialization)
            .with(StrictFeature::Locals, locals);
        for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
            (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
            let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "opt={optimize}: stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), if initialization { EXPECTED }
                else { "A:missing||new:new||AE:missing||bad:P||D:|" }, "init={initialization}, locals={locals}, opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            if initialization { assert!(!stderr.contains("Warning:"), "{stderr}"); }
        }
    }
}

/// Source policies on dynamic and physical reflected reads cannot disagree with their module.
#[test]
fn initialized_extended_reads_validate_original_profiles() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for reflected in [false, true] {
        let mut module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, false, SourceMode::Php);
        let wrong = crate::source::EvalSourceProfile::new(false,
            SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true));
        let instruction = module.functions.iter_mut().flat_map(|function| function.instructions.iter_mut())
            .find(|inst| match inst.immediate {
                Some(crate::ir::Immediate::ReflectionPropertyRef { .. }) => reflected,
                Some(crate::ir::Immediate::ProfiledPropertyFetch { .. }) => !reflected,
                _ => false,
            }).unwrap();
        match instruction.immediate.as_mut().unwrap() {
            crate::ir::Immediate::ReflectionPropertyRef { profile, .. }
                | crate::ir::Immediate::ProfiledPropertyFetch { profile, .. } => *profile = wrong,
            _ => unreachable!(),
        }
        assert!(matches!(crate::ir::validate_module(&module),
            Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })));
    }
}
