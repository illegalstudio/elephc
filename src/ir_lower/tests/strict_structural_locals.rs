//! Purpose:
//! Checks callable and iterable guard lowering across all supported target ABIs.
//!
//! Called from:
//! - Focused EIR strict local contract validation.
//!
//! Key details:
//! - Runtime callability is semantic; native heap regressions separately check descriptor owners.

use crate::{codegen_support::platform::Target, ir::{Immediate, Module, Op, PhpTypePredicate,
    RuntimeCallTarget, RuntimeFnId}, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Lowers the production structural fixture with its late-callable prelude dependencies.
fn lower_structural(target: Target) -> Module {
    let source = include_str!("../../../tests/fixtures/strict/structural_local_bindings.php");
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::object_cast_prelude::inject_with_semantics(program, &mut inventory, profile).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    for ty in [PhpType::Callable, PhpType::Iterable] {
        assert!(checked.local_store_contracts.values().any(|contract| contract.ty == ty), "{ty:?}");
    }
    crate::ir_lower::lower_program(&program, &checked, target, false).unwrap()
}

/// Structural references use actual callable lookup and iterable predicates on every backend.
#[test]
fn strict_structural_locals_share_capability_guards_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_structural(Target::parse(name).unwrap());
        let callable = module.functions.iter().find(|function|
            function.native_local_reference_contract == Some(PhpType::Callable)).unwrap();
        assert!(callable.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
            && inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::IsCallable)))));
        let iterable = module.functions.iter().find(|function|
            function.native_local_reference_contract == Some(PhpType::Iterable)).unwrap();
        assert!(iterable.instructions.iter().any(|inst| inst.op == Op::TypePredicate
            && inst.immediate == Some(Immediate::TypePredicate(PhpTypePredicate::Iterable))));
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Real ELF and Apple assemblers accept structural local guards and callable metadata.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn strict_structural_locals_assemble_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let module = lower_structural(Target::parse(name).unwrap());
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
            .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for target validation");
        child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
