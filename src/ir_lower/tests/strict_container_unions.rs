//! Purpose:
//! Verifies non-scalar alternatives cannot weaken exact scalar union boundaries.
//!
//! Called from:
//! - Selected-profile array, iterable and callable contract regressions.
//!
//! Key details:
//! - PHP array representations and callable normalization remain storage adaptations.
//! - Runtime-selected values exercise the same direct and descriptor membership rules.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function containerOriginal(mixed $value): mixed { return $value; }
function containerArray(array|float $value): string { return is_array($value) ? 'A' : 'F'; }
function containerIterable(iterable|float $value): string { return is_iterable($value) ? 'I' : 'F'; }
function containerCallback(): string { return 'C'; }
function containerCallable(callable|float $value): string { return is_callable($value) ? 'C' : 'F'; }
function containerConcrete(callable $value): string { return $value(); }
function containerIterableValue(iterable $value): iterable { return $value; }
function containerCallableValue(mixed $value): callable { return $value; }
function containerArrayValue(mixed $value): array|float { return $value; }
function containerDescriptor(callable $callback, mixed $value): mixed { return $callback($value); }
echo containerArray(containerOriginal([1])), containerArray(containerOriginal(['key' => 1]));
echo containerArray(containerOriginal(2.5)), containerIterable(containerOriginal([1]));
echo containerCallable(containerOriginal('containerCallback')), containerConcrete(containerOriginal('containerCallback')), '|';
echo is_iterable(containerIterableValue(containerOriginal([1, 2]))) ? 'I' : 'X';
echo containerCallableValue(containerOriginal('containerCallback'))();
echo is_array(containerArrayValue(containerOriginal([1]))) ? 'R|' : 'X|';
try { containerArray(containerOriginal(1)); } catch (TypeError $error) { echo 'A'; }
try { containerIterable(containerOriginal(1)); } catch (TypeError $error) { echo 'I'; }
try { containerCallable(containerOriginal(1)); } catch (TypeError $error) { echo 'C'; }
try { containerCallable(containerOriginal('not_a_callable')); } catch (TypeError $error) { echo 'N'; }
$array = containerArray(...); $iterable = containerIterable(...); $callable = containerCallable(...);
echo containerDescriptor($array, containerOriginal([1])), containerDescriptor($iterable, containerOriginal([1])), containerDescriptor($callable, containerOriginal('containerCallback'));
try { containerDescriptor($array, containerOriginal(1)); } catch (TypeError $error) { echo 'D'; }
try { containerDescriptor($iterable, containerOriginal(1)); } catch (TypeError $error) { echo 'E'; }
try { containerDescriptor($callable, containerOriginal(1)); } catch (TypeError $error) { echo 'F'; }
"#;

/// Each independent non-scalar alternative preserves the exact float member of a declaration.
#[test]
fn strict_container_unions_reject_known_scalar_conversions() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for hint in ["array|float", "iterable|float", "callable|float"] {
        let source = format!("<?php function exactContainer({hint} $value): void {{}} exactContainer(1);");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("expected exact scalar rejection: {hint}"));
        assert!(error.message.contains("strict-coercions"), "{hint}: {error:?}");
    }
}

/// Membership and callable storage adaptation emit on the complete supported target matrix.
#[test]
fn strict_container_unions_emit_membership_and_storage_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Guard failures and successful callable adaptation keep every caller/container owner balanced.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_container_unions_native_kinds_callables_and_owners() {
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
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
        assert_eq!(stdout, "AAFICC|ICR|AICNAICDEF", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
