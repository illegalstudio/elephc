//! Purpose:
//! Tests the typed int-array reduction-loop rewrite over hand-built EIR.
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - The positive fixture mirrors the lowered sum loop, including the transparent
//!   clone/acquire/release scaffold around the boxed accumulator.
//! - Negative cases break the unit step, observe the induction after the loop, or
//!   add an unsupported body op; they must keep the boxed loop.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, MixedNumericOp, Op, Ownership, PassOrigin, RuntimeCallTarget, RuntimeFnId,
    Terminator, ValueDef,
};
use crate::ir_passes::array_sum_loop::ArraySumLoop;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Selects the proof obligation a negative fixture breaks.
#[derive(Clone, Copy, Debug)]
enum Break {
    None,
    StepTwo,
    ObserveIndex,
    ExtraBodyOp,
}

/// One built reduction loop plus the block ids the assertions inspect.
struct Fixture {
    function: Function,
    entry: crate::ir::BlockId,
    exit: crate::ir::BlockId,
}

/// Builds `entry -> header(i < count(data)) -> body(sum += data[i]; i += 1) -> header`.
fn fixture(failure: Break) -> Fixture {
    let mut function = Function::new("sum".into(), IrType::Void, PhpType::Void);
    let data_type = PhpType::Array(Box::new(PhpType::Int));
    let data = function.add_local(
        Some("data".into()),
        IrType::Heap(IrHeapKind::Array),
        data_type.clone(),
        LocalKind::PhpLocal,
    );
    let sum = function.add_local(
        Some("sum".into()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    let observed = function.add_local(
        Some("observed".into()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    let mut b = Builder::new(&mut function);
    let entry = b.create_named_block("entry", vec![]);
    let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
    let body = b.create_named_block("body", vec![]);
    let exit = b.create_named_block("exit", vec![]);
    let param = b.function().block(header).unwrap().params[0];
    b.set_entry(entry);
    b.position_at_end(entry);
    let array = b
        .emit(
            Op::ArrayNew,
            vec![],
            Some(Immediate::I64(0)),
            IrType::Heap(IrHeapKind::Array),
            data_type.clone(),
            Ownership::Owned,
        )
        .unwrap();
    b.emit_store_local(data, array);
    let first = b.emit_const_i64(1);
    let load = b.emit_load_local(data, IrType::Heap(IrHeapKind::Array), data_type.clone());
    assert!(b
        .emit(
            Op::ArrayPush,
            vec![load, first],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        )
        .is_none());
    let seed = b.emit_const_i64(0);
    b.emit_store_local(sum, seed);
    let count_load = b.emit_load_local(data, IrType::Heap(IrHeapKind::Array), data_type);
    let count = b
        .emit(
            Op::RuntimeCall,
            vec![count_load],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Count,
            ))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    let start = b.emit_const_i64(0);
    let step = b.emit_const_i64(if matches!(failure, Break::StepTwo) { 2 } else { 1 });
    b.terminate(Terminator::Br {
        target: header,
        args: vec![start],
    });

    b.position_at_end(header);
    let condition = b
        .emit(
            Op::ICmp,
            vec![param, count],
            Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .unwrap();
    b.terminate(Terminator::CondBr {
        cond: condition,
        then_target: body,
        then_args: vec![],
        else_target: exit,
        else_args: vec![],
    });

    b.position_at_end(body);
    let sum_load = b.emit_load_local(sum, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let array_load = b.emit_load_local(
        data,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Int)),
    );
    let read = b
        .emit(
            Op::ArrayGetUnchecked,
            vec![array_load, param],
            None,
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    let add = b
        .emit(
            Op::MixedNumericBinop,
            vec![sum_load, read],
            Some(Immediate::MixedNumericOp(MixedNumericOp::Add)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .unwrap();
    let clone = b
        .emit(
            Op::MixedClone,
            vec![add],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .unwrap();
    assert!(b
        .emit(
            Op::Release,
            vec![add],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        )
        .is_none());
    let acquired = b
        .emit(
            Op::Acquire,
            vec![clone],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .unwrap();
    assert!(b
        .emit(
            Op::ReleaseLocalSlot,
            vec![],
            Some(Immediate::LocalSlot(sum)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        )
        .is_none());
    b.emit_store_local(sum, acquired);
    assert!(b
        .emit(
            Op::Release,
            vec![clone],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        )
        .is_none());
    if matches!(failure, Break::ExtraBodyOp) {
        let _ = b.emit_const_i64(7);
    }
    b.emit(
        Op::ConcatReset,
        vec![],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let next = b
        .emit(
            Op::IAdd,
            vec![param, step],
            None,
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    b.terminate(Terminator::Br {
        target: header,
        args: vec![next],
    });

    b.position_at_end(exit);
    if matches!(failure, Break::ObserveIndex) {
        b.emit_store_local(observed, param);
    }
    b.terminate(Terminator::Return { value: None });

    drop(b);
    let count_inst = defining(function.value(count).unwrap());
    if let Some(inst) = function.instruction_mut(count_inst) {
        inst.origin = Some(PassOrigin::CountHoist);
    }
    Fixture { function, entry, exit }
}

/// Returns the instruction id that defines a value.
fn defining(value: &crate::ir::Value) -> crate::ir::InstId {
    let ValueDef::Instruction { inst, .. } = value.def else {
        panic!("expected an instruction definition");
    };
    inst
}

/// The exact boxed reduction scaffold becomes one helper call in the preheader.
#[test]
fn sum_loop_rewrites_boxed_reduction() {
    let Fixture { mut function, entry, exit } = fixture(Break::None);
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    assert!(ArraySumLoop.run(&mut function, &mut DataPool::default()));
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    let entry_block = function.block(entry).unwrap();
    let helper = entry_block
        .instructions
        .iter()
        .filter_map(|id| function.instruction(*id))
        .find(|inst| inst.op == Op::ArraySumInt)
        .expect("helper call in the preheader");
    assert_eq!(helper.origin, Some(PassOrigin::ArraySumLoop));
    assert_eq!(helper.result_ownership, Ownership::Owned);
    assert!(matches!(
        entry_block.terminator,
        Some(Terminator::Br { target, .. }) if target == exit
    ));
    assert!(entry_block
        .instructions
        .iter()
        .filter_map(|id| function.instruction(*id))
        .any(|inst| inst.op == Op::StoreLocal));
}

/// Step two, an observed induction and an extra body op all keep the boxed loop.
#[test]
fn sum_loop_rejects_unproven_reductions() {
    for failure in [Break::StepTwo, Break::ObserveIndex, Break::ExtraBodyOp] {
        let Fixture { mut function, .. } = fixture(failure);
        assert!(!ArraySumLoop.run(&mut function, &mut DataPool::default()), "{failure:?}");
        assert!(
            !function
                .instructions
                .iter()
                .any(|inst| inst.op == Op::ArraySumInt),
            "{failure:?}"
        );
    }
}
