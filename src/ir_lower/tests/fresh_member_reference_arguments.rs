//! Purpose:
//! Checks source-order reference creation through known member and expression call signatures.
//!
//! Called from:
//! - Focused strict-local checker and native codegen regressions.
//!
//! Key details:
//! - Signature queries retain the shared planner without creating callable descriptors.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/fresh_member_reference_arguments.php");
const PREFIX: &str = "method|static|present:NULL|seen|constructor|immediate|constructor|method|method|absent|present|first:last|formed|changed:string|seen|null:absent|null:present|";

/// Every supported target lowers caller bindings for methods, constructors and expression calls.
#[test]
fn fresh_member_reference_arguments_emit_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for locals in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Actual source-order bindings survive native calls and drain their reference and object owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn fresh_member_reference_arguments_native_values_and_owners() {
    for locals in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{PREFIX}{}", if locals { "Tinteger|" } else { "Wstring|" }), "locals={locals} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}

/// Runtime-declared methods and closures preserve the same observable caller scope and owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn fresh_member_reference_arguments_opaque_eval_values_and_owners() {
    let body = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $text=$argc > 0 ? '{body}' : ''; eval($text);");
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, true, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "locals={locals}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{PREFIX}{}", if locals { "Tinteger|" } else { "Wstring|" }), "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Reference preparation cannot retroactively initialize an earlier by-value operand.
#[test]
fn fresh_member_reference_arguments_preserve_earlier_read_diagnostics() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let declarations = "<?php class ReadBeforeRef { public function __construct(mixed $read=null, mixed &$target=null) {} public function read(mixed $read, mixed &$target):void {} public static function readStatic(mixed $read, mixed &$target):void {} } $box=new ReadBeforeRef(); ";
    for call in ["$box->read($missing,$missing);", "ReadBeforeRef::readStatic($missing,$missing);",
        "new ReadBeforeRef($missing,$missing);", "(function(mixed $read,mixed &$target):void {})($missing,$missing);"] {
        let source = format!("{declarations}{call}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("Undefined variable: $missing"), "{call}: {error:?}");
    }
}
