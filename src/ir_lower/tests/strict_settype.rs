//! Purpose:
//! Verifies checked settype conversions retain ordinary reference/store semantics.
//!
//! Called from:
//! - Five-target and native staged semantic-profile regressions.
//!
//! Key details:
//! - Conversion failures must precede mutation and keep the original reference owner alive.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function checkedSettype(mixed &$value): void { settype($value, 'int'); }
mixed $value = '9223372036854775807.9'; $alias =& $value;
checkedSettype($value); echo $alias, '|';
foreach (['junk', '9223372036854775808'] as $source) {
    $value = $source;
    try { checkedSettype($value); echo 'X'; } catch (ValueError $e) { echo $alias, '|'; }
}
$value = 1e999;
try { settype(type: 'integer', var: $value); echo 'X'; } catch (ValueError $e) { echo is_float($alias) ? 'I|' : 'X|'; }
$plain = $argc > 0 ? 'junk' : '42';
try { settype($plain, 'float'); echo 'X'; } catch (ValueError $e) { echo $plain, '|'; }
"#;

/// Known unsupported source kinds fail at the same checker boundary as explicit cast syntax.
#[test]
fn strict_settype_frontend_original_domains_and_internal_controls() {
    let source = "<?php function castArray(array $value): void { settype($value, 'int'); }";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("--strict-casts"), "{error:?}");
    let trusted = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), crate::source::SourceMode::Internal).unwrap();
    assert!(crate::types::checker::check_staged_semantics(&trusted, Target::detect_host(), profile).is_ok());
    assert!(crate::types::checker::check_staged_semantics(&program, Target::detect_host(), SemanticProfile::PERMISSIVE).is_ok());
}

/// Every target lowers numeric mutation through typed checked conversions before ordinary stores.
#[test]
fn strict_settype_eir_checked_conversions_all_targets() {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_checked_cast_int")); assert!(asm.contains("__rt_checked_cast_float"));
    }
}

/// Both optimizer settings preserve exact values, reference aliases and original failed receivers.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_settype_native_exact_values_reference_failures_and_owners() {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
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
        assert_eq!(stdout, "9223372036854775807|junk|9223372036854775808|I|junk|", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
