//! Purpose:
//! Tests representation-sensitive consumers of proven or constant boxed arithmetic.
//!
//! Called from:
//! - The EIR pass unit-test harness.
//!
//! Key details:
//! - Range proofs and constant folding must both preserve unsupported boxed consumers.
//! - Integer and float constants have different typed-store conversion contracts.

use crate::ir::{validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, Op, Ownership, Terminator};
use crate::ir_passes::{const_fold::ConstFold, driver::IrPass, integer_range::IntegerRange};
use crate::types::PhpType;

/// Restores the thread-local null representation even when a regression assertion panics.
fn with_null_repr(repr: crate::codegen::NullRepr, run: impl FnOnce()) {
    use crate::codegen::{set_null_repr, NullRepr};
    struct Restore(NullRepr);
    impl Drop for Restore {
        /// Reinstalls the representation that was active before this fixture.
        fn drop(&mut self) { set_null_repr(self.0); }
    }
    let previous = if crate::codegen_support::sentinels::null_repr_is_tagged() {
        NullRepr::Tagged
    } else { NullRepr::Sentinel };
    let _restore = Restore(previous);
    set_null_repr(repr);
    run();
}

/// Builds a bounded checked sum followed by a cast requiring a boxed input cell.
fn array_cast_fixture() -> Function {
    let mut function = Function::new("array_cast".to_string(), IrType::Void, PhpType::Void);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let sum = builder.emit(
        Op::ICheckedAdd, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
    ).unwrap();
    builder.emit(
        Op::Cast, vec![sum], Some(Immediate::CastTarget(IrType::Heap(IrHeapKind::Mixed))),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
    );
    builder.terminate(Terminator::Return { value: None });
    function
}

/// Neither the range pass nor subsequent folding may unbox an array-cast input.
#[test]
fn array_cast_requires_boxed_arithmetic() {
    for pass in [&IntegerRange as &dyn IrPass, &ConstFold as &dyn IrPass] {
        let mut function = array_cast_fixture();
        assert!(validate_function(&function).is_ok());
        assert!(!pass.run(&mut function, &mut DataPool::default()), "{}", pass.name());
        assert_eq!(function.instructions[1].op, Op::ICheckedAdd);
        assert!(validate_function(&function).is_ok());
    }
}

/// Typed ref-cell stores cannot silently lose their Mixed-to-bool/float/string coercion.
#[test]
fn typed_ref_cell_keeps_required_conversion() {
    for target in [PhpType::Bool, PhpType::Float, PhpType::Str] {
        for pass in [&IntegerRange as &dyn IrPass, &ConstFold as &dyn IrPass] {
            let mut function = Function::new("ref_store".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", vec![]);
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(Some("target".to_string()), IrType::I64,
                    target.clone(), LocalKind::RefCell);
                let one = builder.emit_const_i64(1);
                let sum = builder.emit(Op::ICheckedAdd, vec![one, one], None,
                    IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
                builder.emit(Op::StoreRefCell, vec![sum], Some(Immediate::LocalSlot(slot)),
                    IrType::Void, target.clone(), Ownership::NonHeap);
                builder.terminate(Terminator::Return { value: None });
            }
            assert!(validate_function(&function).is_ok());
            assert!(!pass.run(&mut function, &mut DataPool::default()), "{target:?}, {}", pass.name());
        }
    }
}

/// Overflow folded to a float must not bypass an integer slot's explicit numeric conversion.
#[test]
fn folded_float_keeps_integer_store_conversion() {
    let mut function = Function::new("float_store".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(Some("target".to_string()), IrType::I64,
            PhpType::Int, LocalKind::PhpLocal);
        let max = builder.emit_const_i64(i64::MAX);
        let sum = builder.emit(Op::ICheckedAdd, vec![max, max], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit_store_local(slot, sum);
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(validate_function(&function).is_ok());
    assert!(!ConstFold.run(&mut function, &mut DataPool::default()));
}

/// Runtime ordering must retain tagged inputs and bool/null spaceship coercion rules.
#[test]
fn ordering_consumers_preserve_required_boxed_operands() {
    for op in [Op::PhpRelCmp, Op::Spaceship] {
        for null_rhs in [false, true] {
            for overflow in [false, true] {
                for pass in [&IntegerRange as &dyn IrPass, &ConstFold as &dyn IrPass] {
                    let mut function = Function::new("ordering".to_string(), IrType::Void, PhpType::Void);
                    let mut builder = Builder::new(&mut function);
                    let entry = builder.create_named_block("entry", vec![]);
                    builder.set_entry(entry);
                    builder.position_at_end(entry);
                    let lhs = builder.emit_const_i64(if overflow { i64::MAX } else { 1 });
                    let sum = builder.emit(Op::ICheckedAdd, vec![lhs, lhs], None,
                        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
                    let rhs = if null_rhs { builder.emit_const_null() } else { builder.emit_const_bool(true) };
                    let (immediate, ty) = if op == Op::PhpRelCmp {
                        (Some(Immediate::CmpPredicate(CmpPredicate::Slt)), PhpType::Bool)
                    } else { (None, PhpType::Int) };
                    builder.emit(op, vec![sum, rhs], immediate, IrType::I64, ty, Ownership::NonHeap);
                    builder.terminate(Terminator::Return { value: None });
                    assert!(validate_function(&function).is_ok());
                    assert!(!pass.run(&mut function, &mut DataPool::default()), "{op:?}, {}", pass.name());
                }
            }
        }
    }
}

/// Only boxed collision payloads require retention; neighboring values still specialize.
#[test]
fn null_sentinel_collision_preserves_boxed_payloads() {
    use crate::codegen::NullRepr;
    for repr in [NullRepr::Sentinel, NullRepr::Tagged] {
        with_null_repr(repr, || {
            for payload in [i64::MAX - 2, i64::MAX - 1, i64::MAX] {
                for pass in [&IntegerRange as &dyn IrPass, &ConstFold as &dyn IrPass] {
                    let mut function = Function::new("sentinel".to_string(), IrType::Void, PhpType::Void);
                    let sum;
                    {
                        let mut builder = Builder::new(&mut function);
                        let entry = builder.create_named_block("entry", vec![]);
                        builder.set_entry(entry);
                        builder.position_at_end(entry);
                        let base = builder.emit_const_i64(payload - 1);
                        let one = builder.emit_const_i64(1);
                        sum = builder.emit(Op::ICheckedAdd, vec![base, one], None,
                            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
                        builder.emit(Op::EchoValue, vec![sum], None, IrType::Void,
                            PhpType::Void, Ownership::NonHeap);
                        builder.terminate(Terminator::Return { value: None });
                    }
                    assert!(validate_function(&function).is_ok());
                    pass.run(&mut function, &mut DataPool::default());
                    let should_narrow = repr == NullRepr::Tagged || payload != i64::MAX - 1;
                    assert_eq!(function.value(sum).unwrap().ir_type == IrType::I64, should_narrow,
                        "{repr:?}, {payload}, {}", pass.name());
                    assert!(validate_function(&function).is_ok());
                }
            }
        });
    }
}

/// A checked integer-sink result already uses scalar storage in both optimizer modes.
#[test]
fn null_sentinel_does_not_block_existing_scalar_proofs() {
    with_null_repr(crate::codegen::NullRepr::Sentinel, || {
        let mut function = Function::new("scalar".to_string(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let base = builder.emit_const_i64(i64::MAX - 2);
        let one = builder.emit_const_i64(1);
        let sum = builder.emit(Op::ICheckedAddToInt, vec![base, one], None,
            IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
        builder.terminate(Terminator::Return { value: Some(sum) });
        assert!(validate_function(&function).is_ok());
        assert!(IntegerRange.run(&mut function, &mut DataPool::default()));
        assert!(validate_function(&function).is_ok());
        assert!(function.instructions.iter().any(|inst| inst.op == Op::IAdd));
    });
}

/// Null-predicate folding retains ambiguous raw integer and floating-point sentinel bit patterns.
#[test]
fn scalar_sentinel_null_checks_remain_runtime_predicates() {
    use crate::codegen::NullRepr;
    for repr in [NullRepr::Sentinel, NullRepr::Tagged] {
        with_null_repr(repr, || {
            for float in [false, true] {
                let mut function = Function::new("is_null".to_string(), IrType::I64, PhpType::Bool);
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", vec![]);
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let value = if float {
                    builder.emit_const_f64(f64::from_bits((i64::MAX - 1) as u64))
                } else { builder.emit_const_i64(i64::MAX - 1) };
                let predicate = builder.emit(Op::IsNull, vec![value], None,
                    IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
                builder.terminate(Terminator::Return { value: Some(predicate) });
                assert!(validate_function(&function).is_ok());
                assert_eq!(ConstFold.run(&mut function, &mut DataPool::default()),
                    !float && repr == NullRepr::Tagged);
                assert!(validate_function(&function).is_ok());
            }
        });
    }
}
