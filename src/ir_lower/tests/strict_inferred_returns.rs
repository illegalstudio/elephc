//! Purpose:
//! Verifies noncoercive inferred scalar return contracts and their native boxed ABI.
//!
//! Called from:
//! - Selected-profile frontend, all-target emitter and host-native library regressions.
//!
//! Key details:
//! - The real checker produces the return metadata used by native execution.
//! - A large integer distinguishes value preservation from rounding through a float.

use crate::{source::SourceMode, types::PhpType, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function inferredScalar(bool $pick) { if ($pick) { return 9007199254740993; } return 2.5; }
class InferredScalarBox {
    public function read(bool $pick) { if ($pick) { return 9007199254740993; } return 2.5; }
    public static function take(bool $pick) { if ($pick) { return 9007199254740993; } return 2.5; }
}
function showInferred(mixed $value): void { echo is_int($value) ? 'I' : 'F', $value, '|'; }
$pick = $argc > 0;
showInferred(inferredScalar($pick)); showInferred(inferredScalar(!$pick));
$box = new InferredScalarBox(); showInferred($box->read($pick)); showInferred($box->read(!$pick));
showInferred(InferredScalarBox::take($pick)); showInferred(InferredScalarBox::take(!$pick));
$closure = function(bool $pick) { if ($pick) { return 9007199254740993; } return 2.5; };
showInferred($closure($pick)); showInferred($closure(!$pick));
"#;

/// User return alternatives become exact unions while default and trusted inference retain their behavior.
#[test]
fn strict_inferred_returns_preserve_scalar_contracts_on_all_targets() {
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), mode).unwrap();
        for strict in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, strict);
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = Target::parse(name).unwrap();
                let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
                let class = &checked.classes["InferredScalarBox"];
                for result in [&checked.functions["inferredScalar"].return_type,
                    &class.methods["read"].return_type, &class.static_methods["take"].return_type] {
                    if strict && mode != SourceMode::Internal {
                        assert!(result.is_int_float_union(), "{name} {mode:?}: {result:?}");
                    } else { assert_eq!(result, &PhpType::Float, "{name} {mode:?}"); }
                }
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}

/// The selected-profile frontend and native backend preserve actual integer bits and float values.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_inferred_returns_native_kinds_exact_integers_and_owners() {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    for optimize in [false, true] {
        let _policy = crate::optimize::scoped_semantic_profile(profile);
        let mut program = program.clone();
        if optimize {
            program = crate::optimize::fold_constants_for_target(program, target);
            program = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
            program = crate::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
            program = crate::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
            program = crate::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
        }
        let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        if optimize { crate::ir_passes::optimize_module(&mut module); }
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        assert_eq!(stdout, "I9007199254740993|F2.5|".repeat(4), "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
