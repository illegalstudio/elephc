//! Purpose:
//! Tests the loop-bound proof that rewrites typed indexed reads to unchecked reads.
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - The positive fixture mirrors a lowered `while ($i < count($data)) { $sum += $data[$i]; }`
//!   loop: a header block parameter, a private int array and a `TaggedScalar -> Int` cast.
//! - Negative cases break the seed, the step, the receiver construction, the index identity or
//!   the cast-only use set, and must leave the plain `array_get` in place.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind, IrType,
    LocalKind, LocalSlotId, Op, Ownership, PassOrigin, RuntimeCallTarget, RuntimeFnId, Terminator,
    ValueDef, ValueId,
};
use crate::ir_passes::array_get_bounds::ArrayGetBounds;
use crate::ir_passes::count_hoist::CountHoist;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Selects the proof obligation a negative fixture breaks.
#[derive(Clone, Copy, Debug)]
enum Break {
    None,
    NegativeSeed,
    NegativeStep,
    ArrayPush,
    ForeignIndex,
    NoCast,
}

/// One built loop plus the ids the assertions inspect.
struct Fixture {
    function: Function,
    read: crate::ir::InstId,
    cast: crate::ir::InstId,
    read_result: ValueId,
}

/// Builds `entry -> header(i < count(data)) -> body(array_get(data, i)) -> header`.
fn fixture(failure: Break) -> Fixture {
    let mut function = Function::new("bounds".into(), IrType::Void, PhpType::Void);
    let data_type = PhpType::Array(Box::new(PhpType::Int));
    let data = function.add_local(
        Some("data".into()),
        IrType::Heap(IrHeapKind::Array),
        data_type.clone(),
        LocalKind::PhpLocal,
    );
    let sum = function.add_local(Some("sum".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
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
    let seed = b.emit_const_i64(if matches!(failure, Break::NegativeSeed) { -1 } else { 0 });
    b.terminate(Terminator::Br {
        target: header,
        args: vec![seed],
    });

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
    let body_data = b.emit_load_local(data, IrType::Heap(IrHeapKind::Array), data_type.clone());
    if matches!(failure, Break::ArrayPush) {
        let pushed = b.emit_const_i64(1);
        assert!(b
            .emit(
                Op::ArrayPush,
                vec![body_data, pushed],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            )
            .is_none());
    }
    let index = if matches!(failure, Break::ForeignIndex) {
        b.emit_const_i64(0)
    } else {
        param
    };
    let read = b
        .emit(
            Op::ArrayGet,
            vec![body_data, index],
            None,
            IrType::TaggedScalar,
            PhpType::TaggedScalar,
            Ownership::NonHeap,
        )
        .unwrap();
    let (cast_value, has_cast) = if matches!(failure, Break::NoCast) {
        let tagged = b.add_local(
            Some("tagged".into()),
            IrType::TaggedScalar,
            PhpType::TaggedScalar,
            LocalKind::PhpLocal,
        );
        b.emit_store_local(tagged, read);
        (read, false)
    } else {
        let value = b
            .emit(
                Op::Cast,
                vec![read],
                Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.emit_store_local(sum, value);
        (value, true)
    };
    let one = b.emit_const_i64(if matches!(failure, Break::NegativeStep) { -1 } else { 1 });
    let next = b
        .emit(
            Op::IAdd,
            vec![param, one],
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
    b.terminate(Terminator::Return { value: None });

    drop(b);
    let read_id = defining(function.value(read).unwrap());
    let cast = if has_cast {
        defining(function.value(cast_value).unwrap())
    } else {
        read_id
    };
    Fixture {
        function,
        read: read_id,
        cast,
        read_result: cast_value,
    }
}

/// Returns the instruction id that defines a value.
fn defining(value: &crate::ir::Value) -> crate::ir::InstId {
    let ValueDef::Instruction { inst, .. } = value.def else {
        panic!("expected an instruction definition");
    };
    inst
}

/// A private int array with a bounded induction read becomes an unchecked int read.
#[test]
fn bounds_rewrites_proven_read() {
    let Fixture { mut function, read, cast, read_result } = fixture(Break::None);
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    let mut data = DataPool::default();
    CountHoist.run(&mut function, &mut data);
    assert!(ArrayGetBounds.run(&mut function, &mut data));
    assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
    let read_inst = function.instruction(read).unwrap();
    assert_eq!(read_inst.op, Op::ArrayGetUnchecked);
    assert_eq!(read_inst.origin, Some(PassOrigin::ArrayGetBounds));
    assert_eq!(read_inst.result_type, IrType::I64);
    assert_eq!(read_inst.result_php_type, PhpType::Int);
    assert_eq!(function.value(read_result).unwrap().php_type, PhpType::Int);
    assert_eq!(function.instruction(cast).unwrap().op, Op::Nop);
}

/// Every broken proof obligation keeps the plain read and its narrowing cast.
#[test]
fn bounds_rejects_unproven_reads() {
    for failure in [
        Break::NegativeSeed,
        Break::NegativeStep,
        Break::ArrayPush,
        Break::ForeignIndex,
        Break::NoCast,
    ] {
        let Fixture { mut function, read, .. } = fixture(failure);
        let mut data = DataPool::default();
        CountHoist.run(&mut function, &mut data);
        assert!(!ArrayGetBounds.run(&mut function, &mut data), "{failure:?}");
        assert_eq!(
            function.instruction(read).unwrap().op,
            Op::ArrayGet,
            "{failure:?}"
        );
    }
}

/// A slot whose own type is not a plain local array never qualifies.
#[test]
fn bounds_rejects_mixed_receiver_storage() {
    let Fixture { mut function, read, .. } = fixture(Break::None);
    let mut data = DataPool::default();
    CountHoist.run(&mut function, &mut data);
    function.locals[LocalSlotId::from_raw(0).as_raw() as usize].php_type = PhpType::Mixed;
    assert!(!ArrayGetBounds.run(&mut function, &mut data));
    assert_eq!(function.instruction(read).unwrap().op, Op::ArrayGet);
}
