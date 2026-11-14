//! Purpose:
//! Verifies checked numeric casts preserve original domains and exact results through native EIR.
//!
//! Called from:
//! - Focused staged checker, five-target emitter and native ownership regressions.
//!
//! Key details:
//! - Dynamic operands retain casts through optimization and expose catchable failure cleanup.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function checkedCastInt(mixed $value): int { return (int)$value; }
function checkedCastFloat(mixed $value): float { return (float)$value; }
class CheckedCastObject { public string $value = 'owned'; }
echo checkedCastInt('9223372036854775807.9'), '|', checkedCastInt('-9223372036854775808.9'), '|';
echo checkedCastInt('92233720368547758070e-1'), '|', checkedCastInt('0e999999999999999999999'), '|';
echo checkedCastInt('1e-99999999999999999999'), '|', checkedCastInt('-7.9'), '|';
echo checkedCastInt(true), checkedCastInt(null), '|';
echo is_float(checkedCastFloat(1)) ? 'F' : 'X';
echo is_float(checkedCastFloat('1e999')) ? 'I' : 'X';
echo is_float(checkedCastFloat(NAN)) ? 'N|' : 'X|';
foreach (['12tail', '1e', '9223372036854775808', '-9223372036854775809', '1e99', "1\0"] as $bad) {
    try { checkedCastInt($bad); echo 'X'; } catch (ValueError $error) { echo 'V'; } finally { echo '.'; }
}
foreach ([INF, -INF, NAN, 1.0e20] as $badFloat) {
    try { checkedCastInt($badFloat); echo 'X'; } catch (ValueError $error) { echo 'R'; }
}
try { checkedCastInt([str_repeat('fresh', 2)]); echo 'X'; } catch (TypeError $error) { echo 'A'; }
try { checkedCastFloat(new CheckedCastObject()); echo 'X'; } catch (TypeError $error) { echo 'O'; }
echo '|', checkedCastInt(7.9), '|', checkedCastInt(-7.9), '|';
"#;

/// Combines explicit conversion checks with independent implicit and arithmetic contracts.
fn profile() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)
        .with(StrictFeature::Coercions, true).with(StrictFeature::Arithmetic, true)
}

/// Rejects proven non-scalar casts while leaving dynamic alternatives to runtime guards.
#[test]
fn strict_checked_casts_frontend_known_domains_and_trusted_controls() {
    for source in ["<?php function f(array $v): int { return (int)$v; }",
        "<?php class C {} function f(C $v): float { return (float)$v; }", "<?php echo (int)STDIN;"] {
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile()).unwrap_err();
        assert!(error.message.contains("--strict-casts"), "{source} {error:?}");
        let internal = crate::parser::parse_with_mode(&tokens, SourceMode::Internal).unwrap();
        assert!(crate::types::checker::check_staged_semantics(&internal, Target::detect_host(), profile()).is_ok());
    }
}

/// Every target validates typed checked results and rejects a module missing the cast capability.
#[test]
fn strict_checked_casts_eir_targets_preserve_typed_results_and_capabilities() {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile()).unwrap();
        let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_checked_cast_int")); assert!(asm.contains("__rt_checked_cast_float"));
        let mut malformed = module.functions.iter().find(|function| function.instructions.iter().any(|inst|
            matches!(inst.immediate, Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CheckedNumericCast(_)))))).unwrap().clone();
        let cast = malformed.instructions.iter_mut().find(|inst|
            matches!(inst.immediate, Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CheckedNumericCast(_))))).unwrap();
        cast.effects = crate::ir::Effects::PURE;
        assert!(crate::ir::validate_function(&malformed).is_err());
        module.semantic_profile = profile().with(StrictFeature::Casts, false);
        assert!(matches!(crate::ir::validate_module(&module), Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })));
    }
}

/// Dynamic strings and floats agree with folding and retire original owners through catches/finally.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_checked_casts_native_exact_results_failures_and_owners() {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let target = Target::detect_host(); let selected = profile();
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
        assert_eq!(stdout, "9223372036854775807|-9223372036854775808|9223372036854775807|0|0|-7|10|FIN|V.V.V.V.V.V.RRRRAO|7|-7|", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}

/// Native decimal parsing agrees with the neutral exact kernel across shifts and endpoint spelling.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_checked_casts_native_decimal_matrix_matches_neutral_kernel() {
    let mut inputs: Vec<String> = ["0009223372036854775807.999e0", "-0009223372036854775808.999e0",
        ".9223372036854775807e19", ".9223372036854775808e19", "+.0", "\u{b}+42\u{c}",
        "99999999999999999999999999999999999999e-25", "0e999999999999999999999999999",
        "1e-999999999999999999999999999", "1e999999999999999999999999999"].into_iter().map(str::to_string).collect();
    for digits in ["0", "1", "7.9", "9223372036854775807", "9223372036854775808", "00000000123.456"] {
        for exponent in [-25, -19, -1, 0, 1, 19, 25] {
            for sign in ["", "-"] { inputs.push(format!("{sign}{digits}e{exponent}")); }
        }
    }
    inputs.push(format!("0.{}1e513", "0".repeat(512)));
    let mut source = String::from("<?php function matrixCast(mixed $v): int { return (int)$v; }");
    let mut expected = String::new();
    for text in inputs {
        source.push_str(&format!("try {{ echo matrixCast('{text}'); }} catch (ValueError $e) {{ echo 'E'; }} echo '|';"));
        match elephc_semantics::numeric::checked_string_to_int(&text) {
            Ok(value) => expected.push_str(&value.to_string()), Err(_) => expected.push('E'),
        }
        expected.push('|');
    }
    let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
    let target = Target::detect_host();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile()).unwrap();
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    let output = super::native_module::run(&module);
    let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout} {stderr}");
    assert_eq!(stdout, expected); assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
}
