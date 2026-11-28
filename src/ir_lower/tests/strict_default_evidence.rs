//! Purpose:
//! Verifies declaration kind proofs before folding and nullable scalar store failure effects.
//!
//! Called from:
//! - Selected-profile source, all-target and native optimizer regressions.
//!
//! Key details:
//! - Constant selection preserves original kinds, while named null remains a scalar union member.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function scopedUnion(mixed $value): string {
    try { int|false|null $local = $value; return 'V'; }
    catch (TypeError $error) { return 'C'; }
}
echo scopedUnion(true), scopedUnion('owned-' . $argc), scopedUnion(1), scopedUnion(false), scopedUnion(null);
"#;

/// Proven branch results and trait constants are checked before declaration inference can widen them.
#[test]
fn strict_default_evidence_static_branches_trait_constants_and_false_members() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in ["<?php function f(float $v = true ? 1 : 2.0) {}",
        "<?php const CHOOSE = true; function f(float $v = CHOOSE ? 1 : 2.0) {}",
        "<?php trait T { const VALUE = 1; public function f(float $v = self::VALUE) {} }",
        "<?php class C { const CHOOSE = true; public float $v = self::CHOOSE ? 1 : 2.0; }",
        "<?php class C { public const float VALUE = true ? 1 : 2.0; }",
        "<?php function f(false|null $value = true) {}",
        "<?php function f(float $v = 1 ?: 2.0) {}",
        "<?php function f(float $v = 1 ?? 2.0) {}",
        "<?php const CHOOSE = '00'; function f(float $v = CHOOSE ? 1 : 2.0) {}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("expected default source kind failure: {source}"));
        assert!(error.message.contains("strict-coercions"), "{source}: {error:?}");
    }
    for source in ["<?php function f(float $v = false ? 1 : 2.0) {}",
        "<?php const CHOOSE = true; function f(float $v = CHOOSE ? 2.0 : 1) {}",
        "<?php trait T { const VALUE = 2.0; public function f(float $v = self::VALUE) {} }",
        "<?php function f(false|null $value = false) {}",
        "<?php function f(float $v = 0 ?: 2.0) {}",
        "<?php function f(float $v = null ?? 2.0) {}",
        "<?php const CHOOSE = '0'; function f(float $v = CHOOSE ? 1 : 2.0) {}",
        "<?php trait T { public function f(float $v = self::VALUE) {} } class C { use T; const VALUE = 2.0; }"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    }
}

/// A typed float constant's value keeps PHP floating arithmetic under Overflow alone.
#[test]
fn strict_default_evidence_typed_constant_kind_prevents_false_integer_overflow() {
    let source = "<?php class C { public const float VALUE = 1; } echo C::VALUE + PHP_INT_MAX;";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
}

/// A nullable scalar union initializer retains its catch after source optimization on every target.
#[test]
fn strict_default_evidence_union_initializer_effects_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), SourceMode::Lfc).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let _policy = crate::optimize::scoped_semantic_profile(profile);
        let program = crate::optimize::eliminate_dead_code(program.clone(), checked.local_binding_decision_spans());
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|f| f.name == "scopedUnion").unwrap();
        assert!(function.instructions.iter().any(|i| i.op == crate::ir::Op::CatchBind), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Pure RHS reads can fail exact union membership and must retain their catch and owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_default_evidence_native_pure_union_failures_and_owners() {
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), SourceMode::Lfc).unwrap();
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
        assert_eq!(stdout, "CCVVV", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
