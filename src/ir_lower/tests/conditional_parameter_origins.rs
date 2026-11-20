//! Purpose:
//! Checks incoming parameter contracts independently of conditionally recreated local bindings.
//!
//! Called from:
//! - Focused strict-local checker and native codegen regressions.
//!
//! Key details:
//! - Only recreatable by-value parameters acquire a separate owning slot; their ABI stays unchanged.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/conditional_parameter_origins.php");
const CHECKED: &str = "Tstring:after|Tinteger:1|Wstring:ready|Winteger:3|Winteger|TNULL|Tstring|Wdouble|Tstring|Wstring|Tstring|Wobject|Tobject|Tstring|Warray|Tstring|1|Winteger|1|Tstring|closed|Tstring|Tinteger|Tstring|Tinteger|outer|present|absent|Warray|Tstring|";

/// Every supported target retains incoming and recreated parameter contracts with a stable ABI.
#[test]
fn conditional_parameter_origins_emit_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Conditional parameter rebinding preserves declared unions, open values and balanced owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_parameter_origins_native_values_and_owners() {
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

/// A future unset cannot relax the incoming contract or a newly established replacement.
#[test]
fn conditional_parameter_origins_preserve_known_store_diagnostics() {
    for source in [
        "<?php function invalidParameter(bool $reset,int $value):void {$value='bad';if($reset){unset($value);}}",
        "<?php function invalidParameter(bool $reset,int $value):void {if($reset){unset($value);$value='new';$value=2;}}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals"), "{error:?}");
    }
}

/// An inferred ABI never closes an unannotated input, including frames with no typed origins.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_untyped_parameters_keep_open_entry_contracts() {
    let cases = [
        ("<?php function openInput(bool $reset,$value):void {$value='early';if($reset){unset($value);$value='after';}try{$value=2;echo 'W';}catch(TypeError $error){echo 'T';}echo gettype($value),'|';} openInput(false,$argc);openInput(true,$argc);", "Winteger|Tstring|"),
        ("<?php function openScope(bool $clear,$value):void {if($clear){unset($value);}echo array_key_exists('value',get_defined_vars())?'set|':'unset|';}openScope(false,$argc);openScope(true,$argc);", "set|unset|"),
    ];
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for (source, expected) in cases {
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "opt={optimize}: {source}: {stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{source}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
