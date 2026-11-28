//! Purpose:
//! Verifies checked numeric builtin conversions use the shared EIR graphs across call surfaces.
//!
//! Called from:
//! - Five-target and native staged semantic-profile regressions.
//!
//! Key details:
//! - Direct, named/spread, descriptor, dynamic and callback calls retain the same cast policy.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function builtinCastInt(mixed $value, int $base = 10): int { return intval($value, $base); }
function builtinCastFloat(mixed $value): float { return \FLOATVAL(value: $value); }
echo builtinCastInt('9223372036854775807.9'), '|', builtinCastInt('-8000000000000000', 16), '|';
echo intval(base: 16, value: 'ff'), '|', intval(...['value' => '0b101', 'base' => 0]), '|';
echo builtinCastInt(7.9, 1), '|';
echo is_float(builtinCastFloat(1)) ? 'F' : 'X';
echo is_infinite(floatval('1e999')) ? 'I|' : 'X|';
$integer = intval(...); $floating = floatval(...);
echo $integer('ff', 16), '|', $floating('1.5'), '|';
$name = $argc > 0 ? 'INTVAL' : 'FLOATVAL'; echo $name('77', 8), '|';
$mapped = array_map('floatval', ['1.5', '2.5']); echo $mapped[0], ':', $mapped[1], '|';
foreach (['junk', '9223372036854775808'] as $bad) {
    try { builtinCastInt($bad); echo 'X'; } catch (ValueError $error) { echo 'V'; }
    try { $integer($bad); echo 'X'; } catch (ValueError $error) { echo 'D'; }
}
try { $floating('junk'); echo 'X'; } catch (ValueError $error) { echo 'F'; }
try { $integer(str_repeat('f', 32), 16); echo 'X'; } catch (ValueError $error) { echo 'R'; }
try { array_map('floatval', ['1.5', 'junk']); echo 'X'; } catch (ValueError $error) { echo 'C'; }
try { $floating([1]); echo 'X'; } catch (TypeError $error) { echo 'T'; }
echo '|';
"#;

/// Keeps trusted implementation casts separate from public descriptor invocation contracts.
fn program() -> crate::parser::ast::Program {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(
        "<?php function trustedBuiltinCast(mixed $v): int { return intval($v); } function trustedBuiltinDescriptor(): callable { return intval(...); }").unwrap(),
        crate::source::SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap());
    program.extend(crate::parser::parse(&crate::lexer::tokenize(
        "<?php echo trustedBuiltinCast('12tail'), '|'; $trustedDescriptor = trustedBuiltinDescriptor(); try { $trustedDescriptor('junk'); echo 'X'; } catch (ValueError $e) { echo 'P'; }").unwrap()).unwrap());
    program
}

/// Selects explicit conversions alongside independent implicit-value and arithmetic policies.
fn profile() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)
        .with(StrictFeature::Coercions, true).with(StrictFeature::Arithmetic, true)
}

/// Declared original kinds and known string radix domains fail at the shared checker boundary.
#[test]
fn strict_builtin_casts_frontend_declared_kinds_and_known_base_domains() {
    for source in ["<?php function f(array $v): int { return intval($v); }",
        "<?php function f(string $v): int { return intval($v, 1); }",
        "<?php class C {} function f(C $v): float { return floatval($v); }"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile()).unwrap_err();
        assert!(error.message.contains("--strict-casts"), "{source}: {error:?}");
    }
}

/// Prepared builtin bodies preserve checked results and exact radix arguments on every target.
#[test]
fn strict_builtin_casts_eir_all_targets_and_callable_bodies() {
    let program = program();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile()).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_checked_radix_to_int")); assert!(asm.contains("__rt_checked_cast_float"));
    }
}

/// Numeric builtin calls keep exact values and retire owners after direct and callback failures.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_builtin_casts_native_values_call_surfaces_and_exception_owners() {
    let program = program();
    let target = Target::detect_host();
    for selected in [profile(), SemanticProfile::PERMISSIVE] {
        let checked = crate::types::checker::check_staged_semantics(&program, target, selected).unwrap();
        for optimize in [false, true] {
            let _policy = crate::optimize::scoped_semantic_profile(selected);
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
            let expected = if selected.contains(StrictFeature::Casts) {
                "9223372036854775807|-9223372036854775808|255|5|7|FI|255|1.5|63|1.5:2.5|VDVDFRCT|12|P"
            } else { "9223372036854775807|-9223372036854775808|255|5|7|FI|255|1.5|63|1.5:2.5|XXXXXXXX|12|X" };
            assert_eq!(stdout, expected, "{selected:?} opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
        }
    }
}
