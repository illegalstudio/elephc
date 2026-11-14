//! Purpose:
//! Verifies reviewed original native numeric projections on every compiler target.
//!
//! Called from:
//! - Staged EIR checking and emission regressions using the actual native preludes.
//!
//! Key details:
//! - User overrides of __toString cannot replace native numeric source semantics.

use crate::{ir::{Immediate, Op}, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Prepares the real native declarations with their ordinary cross-prelude dependencies.
fn program() -> (crate::parser::ast::Program, crate::optimize::reachability::PreludeInventory) {
    let source = "<?php function gmpCast(GMP $v): int { return (int)$v; } function gmpFloat(GMP $v): float { return floatval($v); } function xmlCast(SimpleXMLElement $v): float { return floatval($v); } function erasedCast(mixed $v): int { return intval($v); } class StrictXmlCast extends SimpleXMLElement { public function __toString(): string { return '0'; } } function inheritedCast(StrictXmlCast $v): int { return (int)$v; }";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::core_prelude::inject_if_used(program, &mut inventory);
    let program = crate::gmp_prelude::inject_if_used(program, false, &mut inventory);
    let program = crate::xml_prelude::inject_if_used(program, false, &mut inventory);
    let program = crate::dom_html_prelude::inject_if_used(program, false, &mut inventory);
    let program = crate::object_cast_prelude::inject_if_used(program, &mut inventory).unwrap();
    let program = crate::func_args::desugar(program).unwrap();
    (crate::name_resolver::resolve(program).unwrap(), inventory)
}

/// A selected ABI retains original text for typed, erased and inherited native receivers.
fn verify_original_native_projections(name: &str) {
    let (program, _) = program();
    for strict in [false, true] {
        let target = Target::parse(name).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict);
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        for function in ["gmpCast", "gmpFloat", "xmlCast", "erasedCast", "inheritedCast"] {
            let function = module.functions.iter().find(|f| f.name == function).unwrap();
            let projections = function.instructions.iter().filter(|inst| inst.op == Op::MethodCall
                && matches!(inst.immediate, Some(Immediate::Data(id)) if module.data.strings[id.as_raw() as usize] == "__elephc_numeric_source")).count();
            assert_eq!(projections > 0, strict, "{name} {} strict={strict}", function.name);
            if strict && function.name == "gmpFloat" {
                let guard = function.instructions.iter().position(|inst| matches!(inst.immediate,
                    Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CheckedNumericCast(
                        elephc_semantics::numeric_cast::NumericCastTarget::Float))))).unwrap();
                let native = function.instructions.iter().position(|inst| inst.op == Op::MethodCall
                    && matches!(inst.immediate, Some(Immediate::Data(id))
                        if module.data.strings[id.as_raw() as usize] == "__elephc_to_float")).unwrap();
                assert!(guard < native, "{name}: original domain proof precedes native floating conversion");
            }
        }
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Keeps each real-prelude target fixture within the ordinary per-test CI budget.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Verifies original native numeric projections on one first-class target.
        #[test]
        fn $name() { verify_original_native_projections($target); }
    };
}
target_test!(strict_object_casts_eir_macos, "macos-aarch64");
target_test!(strict_object_casts_eir_ios, "ios-arm64");
target_test!(strict_object_casts_eir_ios_sim, "ios-sim-arm64");
target_test!(strict_object_casts_eir_linux_arm, "linux-aarch64");
target_test!(strict_object_casts_eir_linux_x86, "linux-x86_64");

/// Reachability retains adapters referenced by the shared native contract before EIR selects them.
#[test]
fn strict_object_casts_reachability_preserves_original_source_adapters() {
    let (program, inventory) = program();
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    let mut checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let exports = ["gmpcast", "gmpfloat", "xmlcast", "erasedcast", "inheritedcast"].into_iter().map(str::to_string).collect();
    let empty = std::collections::HashSet::new();
    let program = crate::optimize::reachability::prune_unreachable_declarations(program, &mut checked,
        crate::optimize::reachability::PruneOptions { inventory: &inventory,
            forced_groups: &empty, exported_functions: &exports, eval_forced: false });
    for class in ["GMP", "SimpleXMLElement"] {
        assert!(checked.classes[class].methods.contains_key("__elephc_numeric_source"), "{class}");
    }
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}
