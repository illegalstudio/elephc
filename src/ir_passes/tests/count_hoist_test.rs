//! Purpose:
//! Tests the loop-invariant typed-array count hoist over hand-built EIR.
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - A read-only array loop hoists the receiver load and count into the preheader.
//! - A slot store, an array push, or a call consuming the receiver keeps them in the loop.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, Op, Ownership, RuntimeCallTarget, RuntimeFnId, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::count_hoist::CountHoist;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Selects the loop mutation the negative cases insert.
#[derive(Clone, Copy, Debug)]
enum Mutation {
    None,
    SlotStore,
    ArrayPush,
    Call,
}

/// One built count loop plus the instruction/block ids the assertions inspect.
struct Fixture {
    function: Function,
    load: ValueId,
    load_inst: crate::ir::InstId,
    count_inst: crate::ir::InstId,
    entry: crate::ir::BlockId,
    header: crate::ir::BlockId,
}

/// Builds `entry -> header(count(data)) -> body(array_get) -> header`.
fn fixture(mutation: Mutation) -> Fixture {
    let mut function = Function::new("count_loop".into(), IrType::Void, PhpType::Void);
    let data_type = PhpType::Array(Box::new(PhpType::Int));
    let data = function.add_local(
        Some("data".into()),
        IrType::Heap(IrHeapKind::Array),
        data_type.clone(),
        LocalKind::PhpLocal,
    );
    let i = function.add_local(Some("i".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let mut b = Builder::new(&mut function);
    let entry = b.create_named_block("entry", vec![]);
    let header = b.create_named_block("header", vec![]);
    let body = b.create_named_block("body", vec![]);
    let exit = b.create_named_block("exit", vec![]);
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
    let zero = b.emit_const_i64(0);
    b.emit_store_local(i, zero);
    b.terminate(Terminator::Br { target: header, args: vec![] });

    b.position_at_end(header);
    let load = b.emit_load_local(data, IrType::Heap(IrHeapKind::Array), data_type.clone());
    let count = b
        .emit(
            Op::RuntimeCall,
            vec![load],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Count,
            ))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    let condition = b.emit_load_local(i, IrType::I64, PhpType::Int);
    let cmp = b
        .emit(
            Op::ICmp,
            vec![condition, count],
            Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .unwrap();
    b.terminate(Terminator::CondBr {
        cond: cmp,
        then_target: body,
        then_args: vec![],
        else_target: exit,
        else_args: vec![],
    });

    b.position_at_end(body);
    let body_data = b.emit_load_local(data, IrType::Heap(IrHeapKind::Array), data_type.clone());
    let body_i = b.emit_load_local(i, IrType::I64, PhpType::Int);
    b.emit(
        Op::ArrayGet,
        vec![body_data, body_i],
        None,
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
    )
    .unwrap();
    match mutation {
        Mutation::None => {}
        Mutation::SlotStore => {
            let fresh = b
                .emit(
                    Op::ArrayNew,
                    vec![],
                    Some(Immediate::I64(0)),
                    IrType::Heap(IrHeapKind::Array),
                    data_type,
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(data, fresh);
        }
        Mutation::ArrayPush => {
            let one = b.emit_const_i64(1);
            assert!(b
                .emit(
                    Op::ArrayPush,
                    vec![body_data, one],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                )
                .is_none());
        }
        Mutation::Call => {
            assert!(b
                .emit(
                    Op::Call,
                    vec![body_data],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                )
                .is_none());
        }
    }
    let one = b.emit_const_i64(1);
    let next = b
        .emit(
            Op::IAdd,
            vec![body_i, one],
            None,
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    b.emit_store_local(i, next);
    b.terminate(Terminator::Br { target: header, args: vec![] });

    b.position_at_end(exit);
    b.terminate(Terminator::Return { value: None });

    let count_inst = defining(function.value(count).unwrap());
    let load_inst = defining(function.value(load).unwrap());
    Fixture { function, load, load_inst, count_inst, entry, header }
}

/// Returns the instruction id that defines a value.
fn defining(value: &crate::ir::Value) -> crate::ir::InstId {
    let ValueDef::Instruction { inst, .. } = value.def else {
        panic!("expected an instruction definition");
    };
    inst
}

/// A read-only array loop moves the receiver load and count into the preheader.
#[test]
fn count_hoist_moves_read_only_array_count() {
    let Fixture { mut function, load, load_inst, count_inst, entry, header } =
        fixture(Mutation::None);
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    assert!(CountHoist.run(&mut function, &mut DataPool::default()));
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    assert!(function.block(entry).unwrap().instructions.contains(&load_inst));
    assert!(function.block(entry).unwrap().instructions.contains(&count_inst));
    assert!(!function.block(header).unwrap().instructions.contains(&count_inst));
    assert!(function.value(load).is_some());
}

/// A slot store, an array push, or a call consuming the receiver blocks the hoist.
#[test]
fn count_hoist_rejects_mutated_or_escaped_receiver() {
    for mutation in [Mutation::SlotStore, Mutation::ArrayPush, Mutation::Call] {
        let Fixture { mut function, count_inst, header, .. } = fixture(mutation);
        assert!(!CountHoist.run(&mut function, &mut DataPool::default()), "{mutation:?}");
        assert!(function.block(header).unwrap().instructions.contains(&count_inst), "{mutation:?}");
    }
}
