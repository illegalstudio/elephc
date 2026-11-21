//! Purpose:
//! Checks producer-copy metadata across optimization and every supported target ABI.
//!
//! Called from:
//! - Focused strict local origin tests and explicit clang target validation.
//!
//! Key details:
//! - Original copy facts survive replacement of a variable read with a constant.

use crate::{codegen_support::platform::Target, ir::{Immediate, Module, Op}, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Lowers copies of eval and native producers after exercising AST constant propagation.
fn lower_copies(target: Target) -> Module {
    let source = "<?php function copiedProducer(int|float $input): void { array<int> $typed=[1]; eval('$created=2.5;'); $copy=$created; $copy=3.5; $second=$copy; $native=$input; $arrayCopy=$typed; echo $second,$native,$arrayCopy[0]; } function conditionalProducer(bool $run,int $seed): void { if($run) {eval('$created=2.5;');} $copy=$seed; $second=$copy; echo $second; } copiedProducer(1); conditionalProducer(false,$argc);";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
        .with(StrictFeature::DynamicCode, true);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    assert!(checked.local_store_contracts.copies.iter().any(|((_, _, name), contract)|
        name == "second" && contract.source == "copy"));
    assert!(checked.local_store_contracts.copies.iter().any(|((_, _, name), contract)|
        name == "arrayCopy" && contract.contract == crate::types::PhpType::php_array()));
    let optimized = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(),
        checked.buffer_read_sites.clone());
    crate::ir_lower::lower_program(&optimized, &checked, target, false).unwrap()
}

/// Immutable checked copies retain provenance, canonical owners and serialized types on every target.
#[test]
fn strict_native_copy_origins_survive_propagation_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_copies(Target::parse(name).unwrap());
        let function = module.functions.iter().find(|function| function.name == "copiedProducer").unwrap();
        let copies = function.instructions.iter().filter(|inst| inst.op == Op::EvalScopeCopyBindingType)
            .collect::<Vec<_>>();
        for wanted in ["copy", "second", "native", "arrayCopy"] {
            assert!(copies.iter().any(|inst| match inst.immediate {
                Some(Immediate::EvalBindingCopy { destination, .. }) =>
                    function.locals[destination.as_raw() as usize].name.as_deref() == Some(wanted),
                _ => false,
            }), "{name}: missing source contract for {wanted}");
        }
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__elephc_reference_scope_type_v1"), "{name}");
        assert!(assembly.contains("__elephc_eval_scope_copy_binding_type_v1"), "{name}");
        assert!(assembly.contains("_native_reference_local_contract_"), "{name}");
        let mut invalid = module.clone();
        invalid.semantic_profile = invalid.semantic_profile.with(StrictFeature::Locals, false);
        assert!(crate::ir::validate_module(&invalid).is_err(), "{name}: copy metadata needs Locals");
    }
}

/// Real ELF and Apple assemblers accept the new copy ABI and extended local metadata.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn strict_native_copy_origins_assemble_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let module = lower_copies(Target::parse(name).unwrap());
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
            .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for explicit target validation");
        process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
        let output = process.wait_with_output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
