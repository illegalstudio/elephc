//! Purpose:
//! Verifies the complete nonescaping numeric-cell lifetime proof with explicit EIR graphs.
//!
//! Called from:
//! - The library test harness through `codegen::stack_numeric`.
//!
//! Key details:
//! - Negative cases cover owners escaping, aliases, delayed reads and loop-carried pointers.

use super::*;
use crate::ir::{Builder, Ownership, Terminator};

/// Extracts selected cells for concise gate assertions.
fn candidates(function: &Function) -> Vec<ValueId> {
    plan(function).cells
}

/// Builds a local numeric update with a scalar read and an optional escaping observation.
fn fixture(escape: bool, delayed: bool) -> (Function, ValueId) {
    let mut function = Function::new("numeric".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(
        Some("sum".into()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let zero = builder.emit_const_i64(0);
    builder.emit_store_local(slot, zero);
    let old = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let one = builder.emit_const_i64(1);
    let result = builder
        .emit(
            Op::ICheckedAdd,
            vec![one, one],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .unwrap();
    let acquired = builder
        .emit(
            Op::Acquire,
            vec![result],
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Owned,
        )
        .unwrap();
    builder.emit_store_local(slot, acquired);
    builder.emit(
        Op::Release,
        vec![old],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    builder.emit(
        Op::Release,
        vec![result],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    if delayed {
        builder.emit(
            Op::Cast,
            vec![old],
            Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        );
    }
    let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
    builder.terminate(Terminator::Return {
        value: Some(if escape { acquired } else { loaded }),
    });
    if escape {
        function.return_type = IrType::Heap(IrHeapKind::Mixed);
        function.return_php_type = PhpType::Mixed;
    }
    (function, result)
}

/// The ordinary local update stays in its frame, but returning its pointer is an escape.
#[test]
fn stack_numeric_local_and_return_gates() {
    let (function, value) = fixture(false, false);
    assert_eq!(candidates(&function), vec![value]);
    let (function, _) = fixture(true, false);
    assert!(candidates(&function).is_empty());
}

/// Main's private numeric slots obey the same complete lifetime proof as ordinary frames.
#[test]
fn stack_numeric_private_main_and_global_escape_gates() {
    let (mut function, value) = fixture(false, false);
    function.flags.is_main = true;
    function.return_type = IrType::Void;
    function.return_php_type = PhpType::Void;
    function.blocks[0].terminator = Some(Terminator::Return { value: None });
    assert_eq!(candidates(&function), vec![value]);
    function.locals[0].kind = LocalKind::GlobalAlias;
    assert!(candidates(&function).is_empty());
}

/// A numeric loop input can stay on the frame after its int-to-float promotion.
#[test]
fn stack_numeric_mixed_numeric_domain_and_escape() {
    let (mut function, value) = fixture(false, false);
    let load = function.instructions.iter().find(|inst| inst.op == Op::LoadLocal).unwrap().result.unwrap();
    let ValueDef::Instruction { inst, .. } = function.value(value).unwrap().def else { panic!("producer"); };
    let producer = &mut function.instructions[inst.as_raw() as usize];
    producer.op = Op::MixedNumericBinop;
    producer.immediate = Some(Immediate::MixedNumericOp(crate::ir::MixedNumericOp::Add));
    producer.operands[0] = load;
    assert_eq!(candidates(&function), vec![value]);
    let mut unseeded = function.clone();
    let seed = unseeded.instructions.iter_mut().find(|inst| inst.op == Op::StoreLocal).unwrap();
    seed.op = Op::Nop;
    seed.immediate = None;
    seed.operands.clear();
    seed.effects = Op::Nop.default_effects();
    assert!(candidates(&unseeded).is_empty(), "a cycle cannot prove its own entry tag");
    let mut escaping = function.clone();
    escaping.blocks[0].terminator = Some(Terminator::Return { value: Some(value) });
    escaping.return_type = IrType::Heap(IrHeapKind::Mixed);
    escaping.return_php_type = PhpType::Mixed;
    assert!(candidates(&escaping).is_empty());
    let slot = function.locals[0].id;
    function.locals[slot.as_raw() as usize].kind = LocalKind::StaticLocal;
    assert!(candidates(&function).is_empty());
}

/// Converting an old pointer after the cell has been reused would read the new payload.
#[test]
fn stack_numeric_rejects_delayed_borrowed_read() {
    let (function, _) = fixture(false, true);
    assert!(candidates(&function).is_empty());
}

/// A final string observation reads the numeric payload without keeping the cell address.
#[test]
fn stack_numeric_allows_string_observation_of_numeric_slot() {
    let (mut function, value) = fixture(false, false);
    let slot = function.locals[0].id;
    let _ = function.blocks[0].terminator.take();
    let mut builder = Builder::new(&mut function);
    builder.position_at_end(crate::ir::BlockId::from_raw(0));
    let loaded = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let text = builder
        .emit(
            Op::Cast,
            vec![loaded],
            Some(Immediate::CastTarget(IrType::Str)),
            IrType::Str,
            PhpType::Str,
            Ownership::Owned,
        )
        .unwrap();
    builder.terminate(Terminator::Return { value: Some(text) });
    function.return_type = IrType::Str;
    function.return_php_type = PhpType::Str;
    assert_eq!(candidates(&function), vec![value]);
}

/// A reference or an eval context can expose a local through a side channel.
#[test]
fn stack_numeric_rejects_reference_eval_and_suspension() {
    for kind in [
        LocalKind::RefCell,
        LocalKind::EvalContext,
        LocalKind::EvalScope,
        LocalKind::EvalGlobalScope,
        LocalKind::GeneratorState,
    ] {
        let (mut function, _) = fixture(false, false);
        function.add_local(None, IrType::I64, PhpType::Int, kind);
        assert!(candidates(&function).is_empty(), "{kind:?}");
    }
    let (mut function, _) = fixture(false, false);
    function.flags.is_generator = true;
    assert!(candidates(&function).is_empty());
}

/// A second destination would alias mutable stack storage across later updates.
#[test]
fn stack_numeric_rejects_shared_destination() {
    let (mut function, result) = fixture(false, false);
    let second = function.add_local(
        Some("other".into()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    let term = function.blocks[0].terminator.take().unwrap();
    let mut builder = Builder::new(&mut function);
    builder.position_at_end(crate::ir::BlockId::from_raw(0));
    builder.emit_store_local(second, result);
    builder.terminate(term);
    assert!(candidates(&function).is_empty());
}

/// Unknown consumers, cloning, child-bearing assignments and conditional local identities fail closed.
#[test]
fn stack_numeric_rejects_unknown_consumers_and_writers() {
    for op in [Op::MixedClone, Op::MixedBox, Op::EchoValue, Op::VarDump] {
        let (mut function, result) = fixture(false, false);
        let term = function.blocks[0].terminator.take().unwrap();
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(crate::ir::BlockId::from_raw(0));
        let (ir, php, owner) = if matches!(op, Op::MixedClone | Op::MixedBox) {
            (
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
        } else {
            (IrType::Void, PhpType::Void, Ownership::NonHeap)
        };
        builder.emit(op, vec![result], None, ir, php, owner);
        builder.terminate(term);
        assert!(candidates(&function).is_empty(), "{op:?}");
    }
    let (mut function, _) = fixture(false, false);
    function.locals[0].kind = LocalKind::GlobalAlias;
    assert!(candidates(&function).is_empty());
}

/// Block parameters can carry a previous iteration's pointer, so they keep real heap cells.
#[test]
fn stack_numeric_rejects_pointer_block_arguments() {
    let (mut function, result) = fixture(false, false);
    let mut builder = Builder::new(&mut function);
    let exit = builder.create_named_block(
        "exit",
        vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
    );
    let pointer = builder.block_param(exit, 0);
    builder.position_at_end(exit);
    let scalar = builder
        .emit(
            Op::Cast,
            vec![pointer],
            Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    builder.terminate(Terminator::Return {
        value: Some(scalar),
    });
    function.blocks[0].terminator = Some(Terminator::Br {
        target: exit,
        args: vec![result],
    });
    assert!(candidates(&function).is_empty());
}

/// Inlined borrowed owners and function parameters do not acquire a new local lifetime.
#[test]
fn stack_numeric_rejects_cleanup_excluded_local() {
    let (mut function, _) = fixture(false, false);
    function
        .no_epilogue_cleanup_slots
        .insert(LocalSlotId::from_raw(0));
    assert!(candidates(&function).is_empty());
}

/// Reusing both inputs of a deep numeric DAG must not expand the same proof exponentially.
#[test]
fn stack_numeric_shared_arithmetic_dag_has_bounded_analysis() {
    let mut function = Function::new("numeric_dag".into(), IrType::I64, PhpType::Int);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry); builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let mut value = builder.emit(Op::MixedBox, vec![one], None, IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, Ownership::Owned).unwrap();
    let mut expected = Vec::new();
    for _ in 0..36 {
        let next = builder.emit(Op::MixedNumericBinop, vec![value, value],
            Some(Immediate::MixedNumericOp(crate::ir::MixedNumericOp::Add)),
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        value = next;
        expected.push(next);
    }
    let scalar = builder.emit(Op::Cast, vec![value], Some(Immediate::CastTarget(IrType::I64)),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(scalar) });
    crate::ir::validate_function(&function).unwrap();
    assert_eq!(candidates(&function), expected);
    let crate::ir::ValueDef::Instruction { inst, .. } = function.value(one).unwrap().def else { panic!("seed"); };
    let seed = &mut function.instructions[inst.as_raw() as usize];
    seed.op = Op::ConstNull; seed.immediate = None; seed.result_php_type = PhpType::Void;
    function.values[one.as_raw() as usize].php_type = PhpType::Void;
    crate::ir::validate_function(&function).unwrap();
    assert!(candidates(&function).is_empty(), "a cached subgraph cannot erase a nonnumeric input");
}

/// A unary numeric update needs a numeric seed; strings and boolean writers keep the runtime path.
#[test]
fn stack_numeric_increment_requires_closed_numeric_domain() {
    let (mut function, result) = fixture(false, false);
    let (id, _) = definition(&function, result).unwrap();
    let old = function.instructions.iter().find(|inst| inst.op == Op::LoadLocal).unwrap().result.unwrap();
    let inst = &mut function.instructions[id.as_raw() as usize];
    inst.op = Op::StrIncDec; inst.operands = vec![old]; inst.immediate = Some(Immediate::I64(1));
    inst.effects = Op::StrIncDec.default_effects();
    crate::ir::validate_function(&function).unwrap();
    assert_eq!(candidates(&function), vec![result]);
    let zero = function.instructions.iter().find(|inst| inst.op == Op::ConstI64).unwrap().result.unwrap();
    function.values[zero.as_raw() as usize].php_type = PhpType::Bool;
    let ValueDef::Instruction { inst, .. } = function.values[zero.as_raw() as usize].def else { panic!("seed"); };
    let seed = &mut function.instructions[inst.as_raw() as usize];
    seed.op = Op::ConstBool; seed.immediate = Some(Immediate::Bool(false));
    seed.result_php_type = PhpType::Bool;
    crate::ir::validate_function(&function).unwrap();
    assert!(candidates(&function).is_empty(), "a nonnumeric seed must fail closed");
}

/// A checked product can feed another numeric producer without exposing either cell address.
#[test]
fn stack_numeric_chained_producers_copy_numeric_payloads() {
    let mut function = Function::new("chain".into(), IrType::I64, PhpType::Int);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry); builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let product = builder.emit(Op::ICheckedMul, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let sum = builder.emit(Op::MixedNumericBinop, vec![product, product],
        Some(Immediate::MixedNumericOp(crate::ir::MixedNumericOp::Add)),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::Release, vec![product], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    let result = builder.emit(Op::ReturnBoundaryMixedToInt, vec![sum],
        Some(Immediate::Data(crate::ir::DataId::from_raw(0))),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.emit(Op::Release, vec![sum], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(result) });
    crate::ir::validate_function(&function).unwrap();
    assert_eq!(candidates(&function), vec![product, sum]);
}

/// A published temporary retains only a numeric cell, and its borrow copies a scalar payload.
#[test]
fn stack_numeric_scoped_operand_keeps_numeric_cell_private() {
    let mut function = Function::new("scoped_numeric".into(), IrType::I64, PhpType::Int);
    let slot = function.add_local(Some("operand".into()), IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, LocalKind::OwnedTemp);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry); builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let value = builder.emit(Op::ICheckedMul, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let owner = builder.emit(Op::Acquire, vec![value], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit_store_local(slot, owner);
    builder.emit(Op::PushCallOperandOwner, vec![], Some(Immediate::LocalSlot(slot)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    let borrowed = builder.emit(Op::Borrow, vec![owner], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Borrowed).unwrap();
    let result = builder.emit(Op::Cast, vec![borrowed], Some(Immediate::CastTarget(IrType::I64)),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.emit(Op::PopCallOperandOwner, vec![], Some(Immediate::LocalSlot(slot)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.emit(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(result) });
    crate::ir::validate_function(&function).unwrap();
    assert_eq!(candidates(&function), vec![value]);
    function.locals[slot.as_raw() as usize].kind = LocalKind::HiddenTemp;
    assert_eq!(candidates(&function), vec![value], "published hidden operands use the same contract");
    let mut unbalanced = function.clone();
    let pop = unbalanced.instructions.iter_mut().find(|inst| inst.op == Op::PopCallOperandOwner).unwrap();
    pop.op = Op::Nop; pop.immediate = None;
    assert!(candidates(&unbalanced).is_empty(), "unbalanced cleanup records stay excluded");
    function.locals[slot.as_raw() as usize].kind = LocalKind::NamedArgTemp;
    assert!(candidates(&function).is_empty(), "other temporary contracts stay excluded");
}

/// Independent cells do not invalidate a typed return read, but replacing its slot still does.
#[test]
fn stack_numeric_typed_return_keeps_the_borrowed_read_epoch() {
    for replaced in [false, true] {
        let (mut function, producer) = fixture(false, false);
        let result = function.blocks[0].terminator.take().unwrap().used_values()[0];
        let (id, _) = definition(&function, result).unwrap();
        function.values[result.as_raw() as usize].ir_type = IrType::Heap(IrHeapKind::Mixed);
        function.values[result.as_raw() as usize].php_type = PhpType::Mixed;
        function.values[result.as_raw() as usize].ownership = Ownership::MaybeOwned;
        let load = &mut function.instructions[id.as_raw() as usize];
        load.result_type = IrType::Heap(IrHeapKind::Mixed); load.result_php_type = PhpType::Mixed;
        load.result_ownership = Ownership::MaybeOwned;
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(crate::ir::BlockId::from_raw(0));
        let one = builder.emit_const_i64(1);
        let other = builder.emit(Op::ICheckedAdd, vec![one, one], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        builder.emit(Op::Cast, vec![other], Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64, PhpType::Int, Ownership::NonHeap);
        if replaced { builder.emit_store_local(LocalSlotId::from_raw(0), other); }
        builder.emit(Op::Release, vec![other], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        let converted = builder.emit(Op::ReturnBoundaryMixedToInt, vec![result],
            Some(Immediate::Data(crate::ir::DataId::from_raw(0))),
            IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
        builder.terminate(Terminator::Return { value: Some(converted) });
        crate::ir::validate_function(&function).unwrap();
        assert_eq!(candidates(&function).contains(&producer), !replaced);
        if !replaced { assert!(candidates(&function).contains(&other)); }
    }
}

/// A producer can overwrite last iteration's cell before its new pointer reaches the slot.
#[test]
fn stack_numeric_backing_cell_write_invalidates_before_store() {
    let mut function = Function::new("backing_cell".into(), IrType::Void, PhpType::Void);
    let slot = function.add_local(Some("number".into()), IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, LocalKind::PhpLocal);
    let index = function.add_local(Some("index".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    let body = builder.create_named_block("body", vec![]);
    builder.set_entry(entry); builder.position_at_end(entry);
    let zero = builder.emit_const_i64(0);
    builder.emit_store_local(slot, zero); builder.emit_store_local(index, zero);
    builder.terminate(Terminator::Br { target: body, args: vec![] });
    builder.position_at_end(body);
    let old = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let step = builder.emit_load_local(index, IrType::I64, PhpType::Int);
    let one = builder.emit_const_i64(1);
    let next = builder.emit(Op::ICheckedAdd, vec![step, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::Cast, vec![old], Some(Immediate::CastTarget(IrType::I64)),
        IrType::I64, PhpType::Int, Ownership::NonHeap);
    let owner = builder.emit(Op::Acquire, vec![next], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.emit_store_local(slot, owner);
    builder.emit(Op::Release, vec![next], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    let next_index = builder.emit(Op::IAdd, vec![step, one], None,
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.emit_store_local(index, next_index);
    builder.terminate(Terminator::Br { target: body, args: vec![] });
    crate::ir::validate_function(&function).unwrap();
    assert!(!candidates(&function).contains(&next), "the old payload must survive until its cast");
}
