//! Purpose:
//! Verifies public descriptor arguments retain original scalar kinds before ABI conversion.
//!
//! Called from:
//! - Selected-profile all-target emission and native callable regressions.
//!
//! Key details:
//! - Runtime-selected callbacks cover indexed, named and unpacked argument containers.
//! - Defaults are producer values, while every explicit regular or variadic argument is checked.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function descriptorValue(mixed $value): mixed { return $value; }
function descriptorFloat(float $value): float { return $value; }
function descriptorInvoke(callable $callback, mixed $value): mixed { return $callback($value); }
function descriptorSpread(callable $callback, array $values): mixed { return $callback(...$values); }
function descriptorMembers(int|false|null $value): string {
    return $value === null ? 'N' : ($value === false ? 'F' : 'I');
}
function descriptorPair(float $first, string $last): void {}
function descriptorLater(): string { echo 'A'; return 'last'; }
function descriptorVariadic(float ...$values): float { return $values[0]; }
$float = descriptorFloat(...);
echo descriptorInvoke($float, descriptorValue(2.5)), '|';
try { descriptorInvoke($float, descriptorValue(1)); } catch (TypeError $error) { echo 'C'; }
try { descriptorSpread($float, [1]); } catch (TypeError $error) { echo 'S'; }
try { descriptorSpread($float, ['value' => 1]); } catch (TypeError $error) { echo 'K'; }
try { descriptorInvoke($float, descriptorValue(str_repeat('x', 3))); } catch (TypeError $error) { echo 'H'; }
$pair = descriptorPair(...);
try { $pair(last: descriptorLater(), first: descriptorValue(1)); } catch (TypeError $error) { echo 'P'; }
$members = descriptorMembers(...);
echo '|', descriptorInvoke($members, descriptorValue(null)), descriptorInvoke($members, descriptorValue(false)), descriptorInvoke($members, descriptorValue(9007199254740993)), '|';
try { descriptorInvoke($members, descriptorValue(true)); } catch (TypeError $error) { echo 'B'; }
$variadic = descriptorVariadic(...);
echo descriptorInvoke($variadic, descriptorValue(3.5)), '|';
try { descriptorInvoke($variadic, descriptorValue(1)); } catch (TypeError $error) { echo 'V'; }
try { descriptorSpread($variadic, ['named' => 1]); } catch (TypeError $error) { echo 'W'; }
"#;

/// Checks the selected production frontend before native or target-specific validation.
fn checked(target: Target) -> (crate::parser::ast::Program, crate::types::CheckResult) {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let result = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    (program, result)
}

/// Every scalar helper is ordinary valid EIR, with exact union and false predicates.
#[test]
fn strict_descriptor_parameters_helpers_are_valid_exact_eir() {
    let target = Target::detect_host();
    let (program, result) = checked(target);
    let mut module = crate::ir_lower::lower_program(&program, &result, target, false).unwrap();
    for expected in [crate::types::PhpType::Float, crate::types::PhpType::Str,
        crate::types::PhpType::Union(vec![crate::types::PhpType::Int, crate::types::PhpType::False, crate::types::PhpType::Void])] {
        let guard = crate::ir_lower::lower_callable_parameter_guard(&mut module, "parameter_guard", &expected);
        crate::ir::validate_function(&guard).unwrap();
        assert!(guard.instructions.iter().any(|inst| inst.op == crate::ir::Op::TypePredicate));
        assert!(guard.blocks.iter().any(|block| matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))));
    }
}

/// Dynamic invokers reference one shared guard per contract on every supported backend.
#[test]
fn strict_descriptor_parameters_share_guards_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let (program, result) = checked(target);
        let module = crate::ir_lower::lower_program(&program, &result, target, false).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert_eq!(assembly.matches("__eir_scalar_argument_guard_0:\n").count(), 1, "{name}");
    }
}

/// Kind failures preserve source evaluation order and retire all boxed/hash/variadic owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_descriptor_parameters_native_order_unions_variadics_and_owners() {
    let target = Target::detect_host();
    let (program, result) = checked(target);
    for optimize in [false, true] {
        let _policy = crate::optimize::scoped_semantic_profile(result.semantic_profile);
        let mut program = program.clone();
        if optimize {
            program = crate::optimize::fold_constants_for_target(program, target);
            program = crate::optimize::propagate_constants(program, result.mixed_storage_local_names(), result.buffer_read_sites.clone());
            program = crate::optimize::prune_constant_control_flow(program, result.local_binding_decision_spans());
            program = crate::optimize::normalize_control_flow(program, result.local_binding_decision_spans());
            program = crate::optimize::eliminate_dead_code(program, result.local_binding_decision_spans());
        }
        let mut module = crate::ir_lower::lower_program(&program, &result, target, false).unwrap();
        if optimize { crate::ir_passes::optimize_module(&mut module); }
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        assert_eq!(stdout, "2.5|CSKHAP|NFI|B3.5|VW", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
