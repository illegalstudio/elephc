//! Purpose:
//! Verifies declared scalar contracts survive later statement and expression stores.
//!
//! Called from:
//! - Selected-profile frontend, all-target and native regressions.
//!
//! Key details:
//! - Checked declaration contracts stay separate from narrowed read types and storage layout.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function incomingStore(mixed $value): mixed { return $value; }
function storeValues(mixed $value, mixed $word): void {
    float $floating = 4.25;
    try { $floating = $value; } catch (TypeError $error) { echo 'F'; }
    echo $floating, '|';
    string $text = 'initial';
    try { echo ($text = $word), '|'; } catch (TypeError $error) { echo 'S'; }
    echo $text, '|';
    int $integer = 7;
    try { $integer = incomingStore(2.0); } catch (TypeError $error) { echo 'I'; }
    $integer = incomingStore(9007199254740993); echo $integer, '|';
    ?int $nullable = null;
    $nullable = incomingStore(9007199254740993); echo $nullable, '|';
    if ($nullable !== null) { $nullable = incomingStore(null); }
    echo $nullable === null ? 'N' : 'X';
    int|float $number = 1;
    $number = incomingStore(2.5); echo $number, '|';
    try { $number = incomingStore('bad'); } catch (TypeError $error) { echo 'U'; }
    echo $number, '|';
}
storeValues(1, 1); storeValues(2.5, 'owned-' . $argc);
$closure = function(mixed $value): void {
    float $local = 3.25;
    try { $local = $value; } catch (TypeError $error) { echo 'C'; }
    echo $local, '|';
};
$closure(1); $closure(2.5);
class StoreScope {
    static function run(mixed $value): void {
        float $local = 6.25;
        try { $local = $value; } catch (TypeError $error) { echo 'M'; }
        echo $local, '|';
    }
}
StoreScope::run(1); StoreScope::run(2.5);
"#;

/// A later store is checked against its original declaration before weak compatibility or flow joining.
#[test]
fn strict_typed_stores_reject_known_implicit_scalar_writes() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in ["<?php float $x = 1.0; $x = 1;",
        "<?php int $x = 1; $x = 2.5;", "<?php bool $x = false; $x = 1;",
        "<?php string $x = 's'; echo ($x = 1);"] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("expected strict store failure: {source}"));
        assert!(error.message.contains("strict-coercions"), "{source}: {error:?}");
    }
    for (mode, selected) in [(SourceMode::Lfc, SemanticProfile::PERMISSIVE), (SourceMode::Internal, profile)] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize("<?php float $x = 1.0; $x = 1;").unwrap(), mode).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), selected).unwrap();
        assert!(checked.local_store_contracts.is_empty(), "{mode:?}");
    }
    let source = "<?php function stableStore(mixed $v) { float $x = 1.0; $x = $v; return $x; }";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    assert_eq!(checked.functions["stableStore"].return_type, crate::types::PhpType::Float);
}

/// Checked contracts protect later writes on every supported target.
#[test]
fn strict_typed_stores_emit_original_kind_checks_on_all_targets() {
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), SourceMode::Lfc).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert!(checked.local_store_contracts.values().any(|ty| *ty == crate::types::PhpType::Float));
        assert!(checked.local_store_contracts.values().any(|ty| *ty == crate::types::PhpType::Str));
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|f| f.name == "storeValues").unwrap();
        assert!(function.instructions.iter().filter(|i| i.op == crate::ir::Op::ThrowTypeError).count() >= 3, "{name}");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Failed stores preserve destinations and successful ones preserve values and owner balances.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_typed_stores_native_values_failures_expression_results_and_owners() {
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), SourceMode::Lfc).unwrap();
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
        assert_eq!(stdout, "F4.25|Sinitial|I9007199254740993|9007199254740993|N2.5|U2.5|2.5|owned-1|owned-1|I9007199254740993|9007199254740993|N2.5|U2.5|C3.25|2.5|M6.25|2.5|", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
