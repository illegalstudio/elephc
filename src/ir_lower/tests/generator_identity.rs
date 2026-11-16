//! Purpose:
//! Verifies generator identity survives optimized expression arms and closure bodies.
//!
//! Called from:
//! - The AST-to-EIR unit-test harness.
//!
//! Key details:
//! - All supported targets share these frontend and EIR invariants.

use super::*;

/// Closure classification comes from checking even if a later rewrite erases every yield.
#[test]
fn generator_identity_closure_uses_checker_classification() {
    let tokens = crate::lexer::tokenize("<?php $closure = function () { if (false) { yield 1; } return 7; };").unwrap();
    let mut program = crate::parser::parse(&tokens).unwrap();
    let checked = crate::types::check(&program).unwrap();
    let crate::parser::ast::StmtKind::Assign { value, .. } = &mut program[0].kind
        else { panic!("expected closure assignment") };
    let crate::parser::ast::ExprKind::Closure { body, .. } = &mut value.kind
        else { panic!("expected closure") };
    body.remove(0);
    let module = crate::ir_lower::lower_program(&program, &checked, Target::detect_host(), false).unwrap();
    assert!(module.closures[0].flags.is_generator);
    assert_eq!(module.closures[0].signature.as_ref().unwrap().return_type,
        crate::types::PhpType::Object("Generator".into()));
}

/// Dead match and short-ternary yields lower without emitting after an EIR terminator.
#[test]
fn generator_identity_lowers_folded_expression_yields_on_every_target() {
    for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
        let module = lower_source_at_for_target("<?php
            function match_dead() { $value = match (1) { 0 => yield 1, default => 2 }; return $value; }
            function short_dead() { $value = 2 ?: yield 1; return $value; }
            enum Choice { case One; public function values() { if (false) { yield 1; } return 3; } }
            $closure = function () { $value = match (1) { 0 => yield 1, default => 4 }; return $value; };
            Choice::One->values();
            ", Path::new("main.php"), Path::new("."), Target::parse(target).unwrap());
        for name in ["match_dead", "short_dead", "Choice::values"] {
            let function = module.functions.iter().chain(&module.class_methods)
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("missing {name}"));
            assert!(function.flags.is_generator, "{target}: {name}");
        }
        assert!(module.closures.iter().any(|function| function.flags.is_generator), "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
