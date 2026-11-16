//! Purpose:
//! Checks numeric closure specialization, exact scalar gates and name resolution with explicit EIR.
//!
//! Called from:
//! - The IR pass library test harness.
//!
//! Key details:
//! - Original Mixed entry points are retained and the rewritten module remains validator-clean.

use crate::ir::{Builder, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind,
    MixedNumericOp, Module, Op, Ownership, Terminator};
use crate::codegen::platform::{Arch, Platform, Target};
use crate::types::PhpType;
use super::super::numeric_closure_calls::specialize;

/// Builds a Mixed identity-plus-one closure and a direct call with the requested physical type.
fn fixture(argument: PhpType) -> Module {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut closure = Function::new("closure_numeric".into(), mixed, PhpType::Mixed);
    closure.flags.is_closure = true;
    closure.flags.is_static = true;
    closure.params.push(FunctionParam { name: "x".into(), ir_type: mixed, php_type: PhpType::Mixed, by_ref: false, variadic: false });
    let input = closure.add_local(Some("x".into()), mixed, PhpType::Mixed, LocalKind::PhpLocal);
    let shadow = closure.add_local(Some("shadow".into()), mixed, PhpType::Mixed, LocalKind::PhpLocal);
    let mut builder = Builder::new(&mut closure);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry); builder.position_at_end(entry);
    let load = builder.emit_load_local(input, mixed, PhpType::Mixed);
    let clone = builder.emit(Op::MixedClone, vec![load], None, mixed, PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit_store_local(shadow, clone);
    let load = builder.emit_load_local(shadow, mixed, PhpType::Mixed);
    let one = builder.emit_const_i64(1);
    let result = builder.emit(Op::MixedNumericBinop, vec![load, one], Some(Immediate::MixedNumericOp(MixedNumericOp::Add)),
        mixed, PhpType::Mixed, Ownership::Owned).unwrap();
    builder.terminate(Terminator::Return { value: Some(result) });
    module.add_closure(closure);
    let mut caller = Function::new("caller".into(), mixed, PhpType::Mixed);
    let mut builder = Builder::new(&mut caller);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry); builder.position_at_end(entry);
    let argument = match argument {
        PhpType::Int => builder.emit_const_i64(7),
        PhpType::Float => builder.emit_const_f64(1.5),
        PhpType::Bool => builder.emit_const_bool(true),
        PhpType::Void => builder.emit_const_null(),
        _ => unreachable!(),
    };
    let target = module.data.intern_function_name("closure_numeric");
    let result = builder.emit(Op::Call, vec![argument], Some(Immediate::Data(target)), mixed,
        PhpType::Mixed, Ownership::Owned).unwrap();
    builder.terminate(Terminator::Return { value: Some(result) });
    module.add_function(caller);
    module
}

/// Exact int and float arguments receive private entry points without changing the descriptor ABI.
#[test]
fn numeric_closure_calls_exact_scalar_specialization_is_idempotent() {
    for argument in [PhpType::Int, PhpType::Float] {
        let mut module = fixture(argument.clone());
        crate::ir::validate_module(&module).unwrap();
        assert!(specialize(&mut module));
        crate::ir::validate_module(&module).unwrap();
        assert_eq!(module.closures.len(), 2);
        assert_eq!(module.closures[0].params[0].php_type, PhpType::Mixed);
        assert_eq!(module.closures[1].params[0].php_type, argument);
        assert_eq!(module.closures[1].display_name(), module.closures[0].name);
        assert!(module.closures[1].has_source_identity());
        assert!(module.closures[1].flags.is_static, "preserve the PHP closure flags");
        assert!(!module.closures[1].instructions.iter().any(|inst| inst.op == Op::MixedClone));
        assert!(!specialize(&mut module));
    }
}

/// Bool and null share I64 storage with integers but must never inherit an integer proof.
#[test]
fn numeric_closure_calls_reject_bool_null_and_by_reference() {
    for argument in [PhpType::Bool, PhpType::Void] {
        let mut module = fixture(argument);
        assert!(!specialize(&mut module));
        assert_eq!(module.closures.len(), 1);
    }
    let mut module = fixture(PhpType::Int);
    module.closures[0].params[0].by_ref = true;
    assert!(!specialize(&mut module));
}

/// Direct-call lookup gives a free function precedence over a same-named closure table entry.
#[test]
fn numeric_closure_calls_respect_first_callee_metadata() {
    let mut module = fixture(PhpType::Int);
    let mut shadow = module.closures[0].clone();
    shadow.flags.is_closure = false;
    module.add_function(shadow);
    assert!(!specialize(&mut module));
}

/// Every function-like host table can contain an already-direct closure call.
#[test]
fn numeric_closure_calls_cover_every_caller_table() {
    for table in 0..7 {
        let mut module = fixture(PhpType::Int);
        let mut caller = module.functions.pop().unwrap();
        match table {
            0 => { module.add_function(caller); }
            1 => {
                caller.name = "C::caller".into();
                caller.flags.is_method = true;
                caller.flags.is_static = true;
                caller.lexical_class = Some("C".into());
                module.class_methods.push(caller);
            }
            2 => { caller.flags.is_closure = true; module.add_closure(caller); }
            3 => module.fiber_wrappers.push(caller),
            4 => module.callback_wrappers.push(caller),
            5 => module.extern_callback_trampolines.push(caller),
            6 => module.runtime_callable_invokers.push(caller),
            _ => unreachable!(),
        }
        crate::ir::validate_module(&module).unwrap();
        assert!(specialize(&mut module), "caller table {table}");
        crate::ir::validate_module(&module).unwrap();
        assert!(module.closures.iter().any(|function| function.source_name.is_some()), "caller table {table}");
    }
}

/// A later optimizer round reuses an existing scalar entry instead of duplicating its body.
#[test]
fn numeric_closure_calls_reuse_variants_exposed_in_later_rounds() {
    let mut module = fixture(PhpType::Int);
    let mut later = module.functions[0].clone();
    later.name = "later_caller".into();
    assert!(specialize(&mut module));
    let first = module.functions[0].instructions.iter().find(|inst| inst.op == Op::Call).unwrap().immediate.clone();
    module.add_function(later);
    crate::ir::validate_module(&module).unwrap();
    assert!(specialize(&mut module));
    crate::ir::validate_module(&module).unwrap();
    assert_eq!(module.closures.len(), 2, "the same scalar tuple needs one implementation");
    let later = module.functions[1].instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
    assert_eq!(later.immediate, first);
}

/// A same-named free function takes precedence over a cached implementation's physical name.
#[test]
fn numeric_closure_calls_do_not_reuse_shadowed_private_entries() {
    let mut module = fixture(PhpType::Int);
    let mut later = module.functions[0].clone(); later.name = "later_caller".into();
    assert!(specialize(&mut module));
    let first = module.functions[0].instructions.iter().find(|inst| inst.op == Op::Call).unwrap().immediate.clone();
    let mut shadow = module.closures[1].clone();
    shadow.flags.is_closure = false; shadow.source_name = None;
    module.add_function(shadow); module.add_function(later);
    assert!(specialize(&mut module));
    crate::ir::validate_module(&module).unwrap();
    assert_eq!(module.closures.len(), 3);
    let later = module.functions[2].instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
    assert_ne!(later.immediate, first);
}


/// A folded reservation must never redirect another exact physical closure name's call.
#[test]
fn numeric_closure_calls_keep_case_distinct_physical_targets() {
    let mut module = fixture(PhpType::Int);
    module.closures[0].name = "Closure_Numeric".into();
    let first_target = module.data.intern_function_name("Closure_Numeric");
    module.functions[0].instructions.iter_mut().find(|inst| inst.op == Op::Call).unwrap()
        .immediate = Some(Immediate::Data(first_target));
    let mut other = module.closures[0].clone();
    other.name = "closure_numeric".into();
    let one = other.instructions.iter_mut().find(|inst| inst.op == Op::ConstI64).unwrap();
    one.immediate = Some(Immediate::I64(100));
    module.add_closure(other);
    let mut other_caller = module.functions[0].clone(); other_caller.name = "other_caller".into();
    let second_target = module.data.intern_function_name("closure_numeric");
    other_caller.instructions.iter_mut().find(|inst| inst.op == Op::Call).unwrap()
        .immediate = Some(Immediate::Data(second_target));
    module.add_function(other_caller.clone());
    crate::ir::validate_module(&module).unwrap();
    assert!(specialize(&mut module));
    crate::ir::validate_module(&module).unwrap();
    assert_eq!(module.closures.len(), 3, "the folded duplicate remains on its original entry");
    assert_ne!(module.functions[0].instructions.iter().find(|inst| inst.op == Op::Call).unwrap()
        .immediate, Some(Immediate::Data(first_target)));
    assert_eq!(module.functions[1].instructions.iter().find(|inst| inst.op == Op::Call).unwrap()
        .immediate, Some(Immediate::Data(second_target)));
    other_caller.name = "later_other_caller".into(); module.add_function(other_caller);
    assert!(!specialize(&mut module), "a later round must not reuse another exact name's variant");
}
