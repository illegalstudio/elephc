//! Purpose:
//! Verifies exact string key storage through the actual selected-profile frontend.
//!
//! Called from:
//! - Focused staged-profile compiler tests before public ArrayKeys admission.
//!
//! Key details:
//! - Covers declarations, loops, closures, unknown string keys and builtin result metadata.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_key_shapes.php");

/// Checks with the actual profile rather than replacing the profile on a permissive module.
fn lower(target: Target, strict: bool, mode: SourceMode) -> crate::ir::Module {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
    let tokens = crate::lexer::tokenize(SOURCE).unwrap();
    let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    crate::ir_lower::lower_program(&program, &checked, target, false).unwrap()
}

/// Exact key metadata stays materializable on all supported targets and physical source modes.
#[test]
fn strict_array_key_shapes_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::parse(name).unwrap(), strict, mode);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Numeric-looking string keys survive storage decisions and typed native builtin helpers.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_array_key_shapes_native_preserve_string_slots() {
    for strict in [false, true] {
        for optimize in [false, true] {
            let mut module = lower(Target::detect_host(), strict, SourceMode::Php);
            if optimize { crate::ir_passes::optimize_module(&mut module); }
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "strict={strict}, opt={optimize}: {stdout} {stderr}");
            let expected = if strict { "SS2SSSSSSSSSS" } else { "II1IIIIIIIIII" };
            assert_eq!(stdout, expected, "strict={strict}, opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
