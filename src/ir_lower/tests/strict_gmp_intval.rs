//! Purpose:
//! Verifies profile-selected original GMP integer projections on every target.
//!
//! Called from:
//! - Selected-profile checker, reachability and EIR emitter regressions.
//!
//! Key details:
//! - Public direct and callable wrappers root the same checked prelude helper.

use crate::{codegen_support::platform::Target, ir::{Immediate, Op}};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checks direct helper selection, reachability and complete target emission for both profiles.
fn verify(name: &str) {
    let (program, inventory) = super::strict_object_casts::prepare(
        "<?php function originalGmpInteger(mixed $value): int { return gmp_intval($value); } $cast = gmp_intval(...);");
    let target = Target::parse(name).unwrap();
    for strict in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict);
        let mut checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let exports = ["originalgmpinteger".to_string()].into_iter().collect();
        let empty = std::collections::HashSet::new();
        let program = crate::optimize::reachability::prune_unreachable_declarations(program.clone(), &mut checked,
            crate::optimize::reachability::PruneOptions { inventory: &inventory, forced_groups: &empty,
                exported_functions: &exports, eval_forced: false });
        assert!(checked.functions.contains_key("__elephc_gmp_checked_intval"));
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "originalGmpInteger").unwrap();
        let helper = if strict { "__elephc_gmp_checked_intval" } else { "__elephc_gmp_intval" };
        assert!(function.instructions.iter().any(|inst| inst.op == Op::Call && matches!(inst.immediate,
            Some(Immediate::Data(id)) if module.data.function_names[id.as_raw() as usize] == helper)), "{name} strict={strict}");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Keeps each native-prelude target fixture inside the ordinary test timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Verifies checked GMP builtin helper selection on one supported target.
        #[test]
        fn $name() { verify($target); }
    };
}
target_test!(strict_gmp_intval_eir_macos, "macos-aarch64");
target_test!(strict_gmp_intval_eir_ios, "ios-arm64");
target_test!(strict_gmp_intval_eir_ios_sim, "ios-sim-arm64");
target_test!(strict_gmp_intval_eir_linux_arm, "linux-aarch64");
target_test!(strict_gmp_intval_eir_linux_x86, "linux-x86_64");
