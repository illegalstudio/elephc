//! Purpose:
//! Tests the constant-bound typed-array fill-loop reservation pass.
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - The positive fixture is a pure `while (i < 50000) { data[] = i % 97; i += 1; }`
//!   loop over a freshly created private array.
//! - A runtime bound, an observed induction or a prior append must keep the checked
//!   generic append and leave the initial capacity alone.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, Op, Ownership, Terminator, ValueDef,
};
use crate::ir_passes::array_fill_loop::ArrayFillLoop;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Selects the proof obligation a negative fixture breaks.
#[derive(Clone, Copy, Debug)]
enum Break {
    None,
    RuntimeBound,
    ObserveIndex,
    PriorPush,
}

/// One built fill loop plus the values the assertions inspect.
struct Fixture {
    function: Function,
    push: crate::ir::InstId,
    array_new: crate::ir::InstId,
}

/// Builds `entry -> header(i < bound) -> body(data[] = i % 97; i += 1) -> header`.
fn fixture(failure: Break) -> Fixture {
    let mut function = Function::new("fill".into(), IrType::Void, PhpType::Void);
    let data_type = PhpType::Array(Box::new(PhpType::Int));
    let data = function.add_local(
        Some("data".into()),
        IrType::Heap(IrHeapKind::Array),
        data_type.clone(),
        LocalKind::PhpLocal,
    );
    let n = function.add_local(Some("n".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
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
            Some(Immediate::Capacity(0)),
            IrType::Heap(IrHeapKind::Array),
            data_type.clone(),
            Ownership::Owned,
        )
        .unwrap();
    b.emit_store_local(data, array);
    if matches!(failure, Break::PriorPush) {
        let first = b.emit_const_i64(99);
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
    }
    let bound = b.emit_const_i64(50000);
    if matches!(failure, Break::RuntimeBound) {
        b.emit_store_local(n, bound);
    }
    let start = b.emit_const_i64(0);
    let step = b.emit_const_i64(1);
    b.terminate(Terminator::Br {
        target: header,
        args: vec![start],
    });

    b.position_at_end(header);
    let bound_value = if matches!(failure, Break::RuntimeBound) {
        b.emit_load_local(n, IrType::I64, PhpType::Int)
    } else {
        bound
    };
    let condition = b
        .emit(
            Op::ICmp,
            vec![param, bound_value],
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
    let divisor = b.emit_const_i64(97);
    let value = b
        .emit(
            Op::ISMod,
            vec![param, divisor],
            None,
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    let load = b.emit_load_local(data, IrType::Heap(IrHeapKind::Array), data_type);
    assert!(b
        .emit(
            Op::ArrayPush,
            vec![load, value],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        )
        .is_none());
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
    let array_new = defining(function.value(array).unwrap());
    let push = function
        .block(body)
        .unwrap()
        .instructions
        .iter()
        .copied()
        .find(|id| function.instruction(*id).is_some_and(|inst| inst.op == Op::ArrayPush))
        .expect("body append");
    Fixture { function, push, array_new }
}

/// Returns the instruction id that defines a value.
fn defining(value: &crate::ir::Value) -> crate::ir::InstId {
    let ValueDef::Instruction { inst, .. } = value.def else {
        panic!("expected an instruction definition");
    };
    inst
}

/// A pure constant-bound fill reserves capacity and switches to the reserved append.
#[test]
fn fill_loop_reserves_constant_capacity() {
    let Fixture { mut function, push, array_new } = fixture(Break::None);
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    assert!(ArrayFillLoop.run(&mut function, &mut DataPool::default()));
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    assert_eq!(function.instruction(push).unwrap().op, Op::ArrayPushReserved);
    let new = function.instruction(array_new).unwrap();
    assert_eq!(new.immediate, Some(Immediate::Capacity(50000)));
    assert!(matches!(
        new.result_php_type.codegen_repr(),
        PhpType::Array(element) if element.codegen_repr() == PhpType::Int
    ));
}

/// A runtime bound, an observed induction and a prior append all keep the checked push.
#[test]
fn fill_loop_rejects_unproven_fills() {
    for failure in [Break::RuntimeBound, Break::ObserveIndex, Break::PriorPush] {
        let Fixture { mut function, push, array_new } = fixture(failure);
        assert!(!ArrayFillLoop.run(&mut function, &mut DataPool::default()), "{failure:?}");
        assert_eq!(
            function.instruction(push).unwrap().op,
            Op::ArrayPush,
            "{failure:?}"
        );
        assert_eq!(
            function.instruction(array_new).unwrap().immediate,
            Some(Immediate::Capacity(0)),
            "{failure:?}"
        );
    }
}
