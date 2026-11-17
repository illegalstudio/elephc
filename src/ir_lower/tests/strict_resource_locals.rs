//! Purpose:
//! Checks resource local guards, copies and native metadata on every supported target.
//!
//! Called from:
//! - Focused EIR resource binding validation and explicit assembler checks.
//!
//! Key details:
//! - Resource value contracts stay distinct from the integer ABI of raw handles.

use crate::{codegen_support::platform::Target, ir::{Immediate, Module, Op, PhpTypePredicate}, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Lowers raw and boxed resource bindings using production checking and existing prelude hooks.
fn lower_resources(target: Target) -> Module {
    let source = "<?php function resourceTarget(mixed &$value,mixed $next): void {$value=$next;} \
        function resourceBinding(mixed $next): void {$stream=tmpfile();$owner=$stream;$copy=$stream; \
        try {resourceTarget($stream,$next);}catch(TypeError $error){} fclose($owner);} \
        function rawResourceBinding(mixed $next): void {$value=stream_context_get_default(); \
        try {resourceTarget($value,$next);}catch(TypeError $error){} echo gettype($value);} \
        resourceBinding(1);rawResourceBinding(1);";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::object_cast_prelude::inject_with_semantics(program, &mut inventory, profile).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    assert!(checked.local_store_contracts.values().any(|contract| contract.ty == PhpType::Resource(None)));
    crate::ir_lower::lower_program(&program, &checked, target, false).unwrap()
}

/// Every resource guard validates tag membership and original descriptor metadata before publication.
#[test]
fn strict_resource_locals_share_guards_and_metadata_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_resources(Target::parse(name).unwrap());
        assert!(module.functions.iter().filter(|function| function.native_local_reference_contract.is_some())
            .any(|function| function.instructions.iter().any(|inst| inst.op == Op::TypePredicate
                && inst.immediate == Some(Immediate::TypePredicate(PhpTypePredicate::Resource)))));
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// ELF and Apple assemblers accept resource callbacks, copying and raw-handle boxing.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn strict_resource_locals_assemble_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let module = lower_resources(Target::parse(name).unwrap());
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
            .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for target validation");
        child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
