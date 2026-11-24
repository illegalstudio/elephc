//! Purpose:
//! Checks separate native static storage and reached activation aliases on every target.
//!
//! Called from:
//! - Focused native static EIR and explicit assembler validation.
//!
//! Key details:
//! - Conditional declaration and initializer failure preserve the prior frame binding.

use crate::{codegen_support::platform::Target, ir::{Immediate, LocalKind, Module, Op}};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Lowers late, conditional and throwing static declarations with the production checker.
fn lower_statics(target: Target, strict: bool) -> Module {
    let source = r#"<?php
        function late_static(): void {
            $value="local";$alias=&$value;static $value="seed";
            echo $value,":",$alias,"|";$value="retained";
        }
        function conditional_static(bool $bind): int {
            $value=$bind?2:9;if($bind){static $value=1;}return $value;
        }
        function fail_static(): int {throw new RuntimeException("init");}
        function late_throw(): void {
            $value="old";try{static $value=fail_static();}catch(Throwable $error){echo $value,"|";}
        }
        function static_parameter_choice(string &$value, bool $bind): void {
            if($bind){static $value=1;}echo $value;
        }
        function static_array_owner(): void {
            $seed=[1];static $value=$seed;unset($seed);
        }
        $parameter="local";static_parameter_choice($parameter,false);
        late_static();late_static();
        echo conditional_static(false),":",conditional_static(true),"|";late_throw();
    "#;
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    crate::ir_lower::lower_program(&program, &checked, target, false).unwrap()
}

/// Every declaration binds a frame alias to a distinct process-lifetime metadata slot.
#[test]
fn strict_native_static_bindings_keep_storage_separate_on_every_target() {
    for strict in [false, true] {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = lower_statics(Target::parse(name).unwrap(), strict);
            for function in module.functions.iter().filter(|function|
                ["late_static", "conditional_static", "late_throw"].contains(&function.name.as_str())) {
                let address = function.instructions.iter().find(|inst| inst.op == Op::LoadStaticLocalRefCell).unwrap();
                let Some(Immediate::LocalSlot(storage)) = address.immediate else { panic!("static metadata slot"); };
                assert_eq!(function.locals[storage.as_raw() as usize].kind, LocalKind::StaticLocal);
                let bind = function.instructions.iter().find(|inst| inst.op == Op::BindRefCellPtr
                    && inst.operands == [address.result.unwrap()]).unwrap();
                let Some(Immediate::LocalSlotPair { first: binding, .. }) = bind.immediate else { panic!("activation alias slot"); };
                assert_ne!(binding, storage, "{name}, strict={strict}");
                assert_eq!(function.locals[binding.as_raw() as usize].kind, LocalKind::PhpLocal);
            }
            let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
            let call = main.instructions.iter().find(|inst| inst.op == Op::Call
                && inst.operands.len() == 2).unwrap();
            let source = main.value(call.operands[0]).unwrap();
            let crate::ir::ValueDef::Instruction { inst, .. } = source.def else { panic!("reference place"); };
            assert!(matches!(main.instruction(inst).unwrap().op, Op::LoadLocal | Op::LoadRefCell),
                "{name}: string reference arguments preserve their place through canonical promotion");
            let array_owner = module.functions.iter()
                .find(|function| function.name == "static_array_owner")
                .expect("array static owner fixture");
            let initializer = array_owner.instructions.iter()
                .find(|instruction| instruction.op == Op::InitStaticLocal)
                .expect("array static initializer");
            let source = array_owner.value(initializer.operands[0]).expect("initializer source");
            let crate::ir::ValueDef::Instruction { inst, .. } = source.def else {
                panic!("{name}: array initializer owner must be an instruction result");
            };
            assert_eq!(
                array_owner.instruction(inst).expect("initializer owner instruction").op,
                Op::Acquire,
                "{name}: publishing a local array as static storage needs an independent owner",
            );
            crate::ir::validate_module(&module).unwrap();
            let mut invalid = module.clone();
            let function = invalid.functions.iter_mut().find(|function| function.name == "late_static").unwrap();
            let storage = function.locals.iter_mut().find(|local| local.kind == LocalKind::StaticLocal).unwrap();
            storage.kind = LocalKind::PhpLocal;
            assert!(crate::ir::validate_module(&invalid).is_err(), "{name}: static addresses require persistent metadata");
            for invalid_main in [false, true] {
                let mut invalid = module.clone();
                let function = invalid.functions.iter_mut().find(|function| function.name == "late_static").unwrap();
                if invalid_main { function.flags.is_main = true; }
                else {
                    let storage = function.locals.iter_mut().find(|local| local.kind == LocalKind::StaticLocal).unwrap();
                    storage.php_type = crate::types::PhpType::Int;
                }
                assert!(crate::ir::validate_module(&invalid).is_err(),
                    "{name}: canonical static references require nonmain Mixed storage");
            }
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Real ELF and Apple assemblers accept static alias address selection on every ABI.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn strict_native_static_bindings_assemble_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        for strict in [false, true] {
            let module = lower_statics(Target::parse(name).unwrap(), strict);
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
                .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for target validation");
            child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}, strict={strict}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Every target keeps the persistent static while unset gives its activation name fresh storage.
#[test]
fn strict_native_static_detachment_keeps_persistent_owners_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/native_static_detachment.php");
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    for strict in [false, true] {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict);
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            let function = module.functions.iter().find(|function| function.name == "native_detach_static").unwrap();
            let bind = function.instructions.iter().find(|inst| inst.op == Op::BindRefCellPtr).unwrap();
            let Some(Immediate::LocalSlotPair { first: old, .. }) = bind.immediate else { panic!("owned activation binding"); };
            assert!(function.instructions.iter().any(|inst| inst.op == Op::UnsetLocal
                && inst.immediate == Some(Immediate::LocalSlot(old))), "{name}: detach the old activation");
            assert!(function.instructions.iter().any(|inst| inst.op == Op::ZeroLocalSlot
                && inst.immediate == Some(Immediate::LocalSlot(old))), "{name}: retire the old frame storage");
            assert!(function.locals.iter().any(|local| local.name.as_deref() == Some("value")
                && local.kind == LocalKind::PhpLocal && local.id != old), "{name}: fresh local storage");
            assert_eq!(function.locals.iter().filter(|local| local.kind == LocalKind::StaticLocal).count(), 1);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// A fresh binding after static detachment takes its own kind instead of the persistent origin.
#[test]
fn strict_native_static_detachment_reports_the_fresh_local_contract() {
    let source = "<?php function detached_origin(): void { static $value=1; unset($value); $value=2.5; $value='bad'; }";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let target = Target::parse("linux-x86_64").unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let error = crate::types::checker::check_staged_semantics(&program, target, profile).err().expect("fresh binding rejects another kind");
    assert!(error.message.contains("--strict-locals") && error.message.contains("from float"), "{error:?}");
}
