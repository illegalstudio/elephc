//! Purpose:
//! Verifies conditional result contracts survive closure ABI selection.
//!
//! Called from:
//! - Selected-profile all-target and native regression tests.
//!
//! Key details:
//! - Scalar alternatives keep their original kinds, including null and large integers.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function conditionalScalar(bool $pick) { return $pick ? 9007199254740993 : 2.5; }
function conditionalMatch(bool $pick) { return match ($pick) { true => 9007199254740993, false => 2.5 }; }
function conditionalLocal(bool $pick) { $local = fn(bool $flag) => $flag ? 9007199254740993 : 2.5; return $local($pick); }
function showConditional(mixed $value): void {
    if (is_int($value)) { echo 'int:'; }
    elseif (is_float($value)) { echo 'float:'; }
    elseif (is_bool($value)) { echo 'bool:'; }
    elseif (is_string($value)) { echo 'string:'; }
    else { echo 'null:'; }
    echo $value, '|';
}
$pick = $argc > 0;
$arrow = fn(bool $value) => $value ? 9007199254740993 : 2.5;
$short = fn(int $value) => $value ?: 2.5;
$match = fn(bool $value) => match ($value) { true => 9007199254740993, false => 2.5 };
$text = fn(bool $value) => $value ? true : 'text-' . $argc;
$nullable = fn(bool $value) => $value ? null : 'text';
showConditional(conditionalScalar($pick)); showConditional(conditionalScalar(!$pick));
showConditional(conditionalMatch($pick)); showConditional(conditionalMatch(!$pick));
showConditional(conditionalLocal($pick)); showConditional(conditionalLocal(!$pick));
showConditional($arrow($pick)); showConditional($arrow(!$pick));
showConditional($short(9007199254740993)); showConditional($short(0));
showConditional($match($pick)); showConditional($match(!$pick));
showConditional($text($pick)); showConditional($text(!$pick));
showConditional($nullable($pick)); showConditional($nullable(!$pick));
"#;

/// Conditional results expose exact scalar unions instead of a scalar-biased fallback.
#[test]
fn strict_conditional_results_frontend_contracts_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for mode in [SourceMode::Php, SourceMode::Lfc] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), mode).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            for function in ["conditionalScalar", "conditionalMatch", "conditionalLocal"] {
                assert!(checked.functions[function].return_type.is_int_float_union(), "{name} {mode:?} {function}");
            }
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Ambiguous source identities choose Mixed storage without losing either closure's value.
#[test]
fn strict_conditional_results_conflicting_source_sites_keep_safe_storage() {
    use crate::types::PhpType;
    let parse = |source| crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let mut program = parse("<?php $one = fn() => 2.5;");
    program.extend(parse("<?php $two = fn() => 's';"));
    program.extend(parse("<?php function showCollision(mixed $v): void { echo is_float($v) ? 'F' : 'S'; echo $v, '|'; } showCollision($one()); showCollision($two());"));
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert_eq!(checked.closure_result_types.len(), 1, "{name}");
        assert_eq!(checked.closure_result_types.values().next(), Some(&PhpType::Mixed), "{name}");
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    #[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
    {
        let target = Target::detect_host();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let output = super::native_module::run(&module);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"F2.5|Ss|");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// Trusted closures never consume a same-span user contract from a neighboring source fragment.
#[test]
fn strict_conditional_results_trusted_sources_keep_separate_policies() {
    let parse = |source, mode| crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
    let mut program = parse("<?php $usr = fn(bool $pick) => $pick ? 9007199254740993 : 2.5;", SourceMode::Php);
    program.extend(parse("<?php $lib = fn(bool $pick) => $pick ? 9007199254740993 : 2.5;", SourceMode::Internal));
    program.extend(parse("<?php echo is_int($usr(true)) ? 'I' : 'X'; echo is_float($lib(true)) ? 'F' : 'X';", SourceMode::Php));
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert_eq!(checked.closure_result_types.len(), 1, "{name}");
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    #[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
    {
        let target = Target::detect_host();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let output = super::native_module::run(&module);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"IF");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// Native arrow, match and short-ternary results preserve original kinds through both optimizers.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_conditional_results_native_kinds_exact_values_and_owners() {
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
        assert_eq!(stdout, format!("{}bool:1|string:text-1|null:|string:text|", "int:9007199254740993|float:2.5|".repeat(6)), "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
