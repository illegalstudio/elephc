//! Purpose:
//! Keeps persistent static origins separate from conditionally recreated activation bindings.
//!
//! Called from:
//! - Focused strict-local checker and native capture regressions.
//!
//! Key details:
//! - A reached declaration replaces the activation origin without replacing persistent storage.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/conditional_static_origins.php");
const CHECKED: &str = "string:new|integer:1|string:new|integer:1|1|string:new|array:1|1:string:new|1:integer:1|Tinteger:1|Tstring:new|string:new|integer:1|";

/// Every target preserves the persistent source when an activation-local static name detaches.
#[test]
fn conditional_static_origins_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Static declarations, prior parameters and escaped aliases retain their independent owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_static_origins_native_values_and_owners() {
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

/// Runtime eval observes the reached static or the replacement activation-local producer.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_static_origins_opaque_scope_values_and_owners() {
    let source = r#"<?php
        function opaqueStatic197(bool $reset, string $source): void {
            static $value = 1;
            $old = function() use (&$value): mixed { return $value; };
            if ($reset) { unset($value); $value = 'new'; }
            try { eval($source); echo 'W'; } catch (TypeError $error) { echo 'T'; }
            echo gettype($value), ':', $value, ':', $old(), '|';
        }
        $source = $argc > 0 ? '$value=2;' : '';
        opaqueStatic197(false, $source); opaqueStatic197(true, $source);
    "#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        assert!(module.required_runtime_features.eval_bridge);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "Winteger:2:2|Tstring:new:2|", "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// A future unset does not legalize incompatible writes before or after a fresh producer.
#[test]
fn conditional_static_origins_preserve_known_store_diagnostics() {
    for body in [
        "static $value=1;$value='bad';if($reset){unset($value);}",
        "static $value=1;if($reset){unset($value);$value='new';$value=2;}",
    ] {
        let source = format!("<?php function invalidStatic197(bool $reset):void{{{body}}}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals"), "{body}: {error:?}");
    }
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let error = crate::types::checker::check_staged_semantics(
        &program, Target::detect_host(), SemanticProfile::PERMISSIVE).unwrap_err();
    assert!(error.message.contains("Type error: cannot reassign"), "{error:?}");
}
