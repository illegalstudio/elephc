//! Purpose:
//! Preserves captured producer contracts across conditional and unconditional binding ends.
//!
//! Called from:
//! - Focused strict-local checker and native capture regressions.
//!
//! Key details:
//! - Repeated activations preserve the closure environment and incoming reference identity.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/captured_binding_origins.php");
const CHECKED: &str = "string:new|integer:1|1|string:new|integer:1|1|string:new|string:new|1|string:new|string:new|1|Winteger|Tstring|W2:integer:2|2|T2:string:new|2|";

/// Every target supports captured entry contracts and detachable reference handles.
#[test]
fn captured_binding_origins_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Repeated closure activations retain their enclosing storage and retire detached owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn captured_binding_origins_native_values_and_owners() {
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


/// Future binding ends cannot relax an existing capture or its first fresh replacement.
#[test]
fn captured_binding_origins_preserve_known_store_diagnostics() {
    for capture in ["$value", "&$value"] {
        for body in [
            "$value='bad';if($reset){unset($value);}",
            "if($reset){unset($value);$value='new';$value=2;}",
        ] {
            let source = format!("<?php $value=1;$read=function(bool $reset)use({capture}):void{{{body}}};");
            let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
            assert!(error.message.contains("--strict-locals"), "{capture}: {body}: {error:?}");
        }
    }
}

/// Actual opaque evaluation preserves capture origins in both native and interpreted closures.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn captured_binding_origins_opaque_scope_values_and_owners() {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let native = r#"
        function nativeCapturedValue192(int $value,string $source):void {
            $read=function(bool $reset)use($value,$source):void {
                if($reset){unset($value);$value='new';}
                eval($source);echo gettype($value),':',$value,'|';
            };
            $read(false);$read(true);echo $value,'|';
        }
        function nativeCapturedReference192(int $value,string $source):void {
            $read=function(bool $reset)use(&$value,$source):void {
                if($reset){unset($value);$value='new';}
                eval($source);echo gettype($value),':',$value,'|';
            };
            $read(false);$read(true);echo $value,'|';
        }
        $source=$argc>0?'try{$value=2;echo "W";}catch(TypeError $error){echo "T";}':'';
        nativeCapturedValue192(1,$source);nativeCapturedReference192(1,$source);
    "#;
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);{native}");
    let expected = format!("{CHECKED}Winteger:2|Tstring:new|1|Winteger:2|Tstring:new|2|");
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
