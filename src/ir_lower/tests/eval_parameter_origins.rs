//! Purpose:
//! Checks incoming binding contracts on by-value parameters in eval-visible native frames.
//!
//! Called from:
//! - Focused strict-local checker and native codegen regressions.
//!
//! Key details:
//! - The ABI prefix remains unchanged; a frame-owned shadow carries the recreated binding.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/eval_parameter_origins.php");
const CHECKED: &str = "Tinteger:1|Tstring:1|1:new|Winteger|Tstring|integer|string|integer|string|new|1|";

/// All target ABIs preserve incoming types and independent shadows in eval-visible frames.
#[test]
fn eval_parameter_origins_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Open inputs, escaped aliases and caller containers keep their independent value ownership.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn eval_parameter_origins_native_values_and_owners() {
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

/// Entry contracts reject a known invalid write before any eval or binding end occurs.
#[test]
fn eval_parameter_origins_preserve_known_store_diagnostics() {
    for source in [
        "<?php function badEvalParameter(bool $reset,int $value):void {$value='bad';if($reset){unset($value);}eval('');}",
        "<?php function badEvalParameter(int $value):void {unset($value);$value='new';$value=2;eval('');}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals"), "{error:?}");
    }
}

/// Opaque fragments synchronize the active shadow and preserve old aliases on both origin paths.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn eval_parameter_origins_opaque_scope_values_and_owners() {
    let source = r#"<?php
        function opaqueParameterEnd(bool $reset,int $value,string $source,mixed $next):void {
            $old=function()use(&$value):mixed{return $value;};
            if($reset){unset($value);}
            try{eval($source);echo 'W';}catch(TypeError $error){echo 'T';}
            try{$value=$next;echo 'W';}catch(TypeError $error){echo 'T';}
            echo gettype($value),':',$value,':',$old(),'|';
        }
        function opaqueOwnedEnd(bool $reset,string $source,mixed $next):void {
            $value=1;
            $old=function()use(&$value):mixed{return $value;};
            if($reset){unset($value);}
            try{eval($source);echo 'W';}catch(TypeError $error){echo 'T';}
            try{$value=$next;echo 'W';}catch(TypeError $error){echo 'T';}
            echo gettype($value),':',$value,':',$old(),'|';
        }
        $source=$argc>0?'$value="new";':'';
        opaqueParameterEnd(false,1,$source,2);opaqueParameterEnd(true,1,$source,2);
        opaqueOwnedEnd(false,$source,2);opaqueOwnedEnd(true,$source,2);
    "#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout),
            "TWinteger:2:2|WTstring:new:1|TWinteger:2:2|WTstring:new:1|", "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}
