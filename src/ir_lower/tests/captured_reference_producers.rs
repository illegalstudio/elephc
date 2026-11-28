//! Purpose:
//! Checks executed first producers before exposing canonical cells to reference captures.
//!
//! Called from:
//! - Focused strict-local checker and native capture regressions.
//!
//! Key details:
//! - Outgoing captures share the executed producer without retyping foreign incoming storage.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/captured_reference_producers.php");
const CHECKED: &str = "Tinteger:1|integer:1|Tstring:old|string:old|Winteger:3|integer:3|Wstring:ready|string:ready|1:new|old:new|1|";

/// Every target supports captured entry contracts and detachable reference handles.
#[test]
fn captured_reference_producers_emit_on_every_target() {
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
fn captured_reference_producers_native_values_and_owners() {
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

/// An outgoing capture does not legalize writes that conflict with a definitely live origin.
#[test]
fn captured_reference_producers_preserve_known_store_diagnostics() {
    let source = "<?php function invalidCaptured193(bool $flag):void {$value=1;$read=function()use(&$value):void{};if($flag){$value='bad';}}";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("--strict-locals"), "{error:?}");
}

/// Opaque evaluation retains executed contracts for both interpreted and native reference captures.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn captured_reference_producers_opaque_scope_values_and_owners() {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let native = r#"
        function nativeSelectedReference193(bool $numeric,string $source):void {
            if($numeric){$value=1;}else{$value='old';}
            $read=function()use(&$value,$source):void {
                eval($source);echo gettype($value),':',$value,'|';
            };
            $read();echo gettype($value),':',$value,'|';
        }
        $text=$argc>0?'try{$value="new";echo "W";}catch(TypeError $error){echo "T";}':'';
        $number=$argc>0?'try{$value=2;echo "W";}catch(TypeError $error){echo "T";}':'';
        nativeSelectedReference193(true,$text);nativeSelectedReference193(false,$number);
    "#;
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);{native}");
    let expected = format!("{CHECKED}Tinteger:1|integer:1|Tstring:old|string:old|");
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
