//! Purpose:
//! Verifies deterministic textual EIR printer output.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - Printer tests intentionally assert substrings instead of full snapshots
//!   so focused additions do not duplicate the complete textual module.

use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{
    print_module, Builder, Function, Immediate, IrType, Module, Op, Ownership,
    Terminator,
};
use crate::types::PhpType;

/// Prints a minimal integer-returning function.
#[test]
fn prints_minimal_function() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let mut function = Function::new("ret_seven".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    module.add_function(function);

    let printed = print_module(&module);
    assert!(printed.contains("module target=macos-aarch64"));
    assert!(printed.contains("function ret_seven"));
    assert!(printed.contains("const_i64 7"));
    assert!(printed.contains("return v0"));
}

/// Prints block parameters and branch arguments in a stable order.
#[test]
fn prints_block_params_and_branch_args() {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut function = Function::new("branch".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(3);
        builder.terminate(Terminator::Br {
            target: body,
            args: vec![value],
        });
        let param = builder.block_param(body, 0);
        builder.position_at_end(body);
        builder.terminate(Terminator::Return { value: Some(param) });
    }
    module.add_function(function);

    let printed = print_module(&module);
    assert!(printed.contains("body(v0: I64 php=int):"));
    assert!(printed.contains("br bb1(v1)"));
}

/// Distinguishes source-level casts from implicit coercions in textual EIR.
#[test]
fn prints_explicit_cast_semantics() {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut function =
        Function::new("explicit_cast".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let source = builder.emit_const_i64(7);
        let result = builder
            .emit(
                Op::Cast,
                vec![source],
                Some(Immediate::ExplicitCastTarget(IrType::I64)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("explicit cast result");
        builder.terminate(Terminator::Return {
            value: Some(result),
        });
    }
    module.add_function(function);

    let printed = print_module(&module);
    assert!(printed.contains("cast v0 explicit I64"));
}
