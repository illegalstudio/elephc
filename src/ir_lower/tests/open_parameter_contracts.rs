//! Purpose:
//! Keeps semantic parameter contracts independent of native specialization without requiring unset.
//!
//! Called from:
//! - Focused strict-local checker and native parameter regressions.
//!
//! Key details:
//! - Untyped by-value inputs already use the boxed ABI; ordinary stores need no reference cells.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/open_parameter_contracts.php");
const CHECKED: &str = "integer:2|string:ready|double:2.5|double:3.5|integer:2|Tinteger:1|Tinteger:1|string:new|Tinteger:1|method|static|closure|local:1|stream|1|closed|merged|merged|";

/// Every target supports open inputs without adding reference storage for same-kind writes.
#[test]
fn open_parameter_contracts_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        let same = module.functions.iter().find(|function| function.name == "sameKindOpenInput").unwrap();
        assert_eq!(same.locals[0].php_type, same.params[0].php_type, "{name}");
        assert!(!same.locals.iter().any(|local| local.kind == crate::ir::LocalKind::RefCell), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Open entry bindings accept changing PHP kinds without relaxing declared or caller contracts.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn open_parameter_contracts_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// Declared inputs and fresh bindings remain closed even when other inputs in the frame are open.
#[test]
fn open_parameter_contracts_preserve_known_store_diagnostics() {
    for source in [
        "<?php function closedInput191(int $value):void {$value='bad';}",
        "<?php function closedLocal191($input):void {$value=1;$value='bad';}",
        "<?php function closedNew191($value):void {unset($value);$value='new';$value=2;}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals"), "{error:?}");
    }
}

/// Opaque source sees the original open contract after earlier native stores changed its PHP kind.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn open_parameter_contracts_opaque_scope_values_and_owners() {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let native = r#"
        function nativeOpenInput191($value,string $source):void {
            $value='early';eval($source);echo gettype($value),':',$value,'|';
        }
        $source=$argc>0?'$value=2.5;':'';nativeOpenInput191(1,$source);
    "#;
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);{native}");
    let expected = format!("{CHECKED}double:2.5|");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
        assert!(module.required_runtime_features.eval_bridge, "exercise the interpreter bridge");
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}
