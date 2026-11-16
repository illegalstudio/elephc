//! Purpose:
//! Tests for the straight-line reuse of buffer-typed property reads on hand-built methods: a
//! repeated `$this->slots[$index]` read shares the property read and the record address, a read
//! of another index shares one descriptor, and property stores, calls, `buffer_free()`, `unset`,
//! index writes, other receivers and local handles keep the reads apart, also when the later read
//! repeats in a loop body.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each method reads `$this->slots[$index]` in `entry`, branches, and reads it again in `rhs`,
//!   in the shape `return $this->slots[$i]->alive && $this->slots[$i]->mask ...` lowers to, with
//!   the property reads refined to `reads_heap, may_throw` the way effect refinement leaves a
//!   declared property. `$this` is the method's first parameter and slot 0, as lowering makes it.
//!   The module pass runs directly; end-to-end behaviour lives in
//!   `tests/codegen/optimizer/buffer_property_reuse.rs`.

use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{
    validate_function, BlockId, Builder, DataId, Effects, Function, FunctionParam, Immediate,
    InstId, IrHeapKind, IrType, LocalKind, LocalSlotId, Module, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::buffer_access::hoist_buffer_resolution;
use crate::types::PhpType;

/// The PHP type of a `buffer<Vec2>` whose elements are packed records.
fn packed_buffer_type() -> PhpType {
    PhpType::Buffer(Box::new(PhpType::Packed("Vec2".to_string())))
}

/// The PHP type of the method receiver.
fn table_type() -> PhpType {
    PhpType::Object("Table".to_string())
}

/// The data-pool name of the `slots` property.
fn slots_property() -> Immediate {
    Immediate::Data(DataId::from_raw(0))
}

/// Local slots of a hand-built method.
#[derive(Clone, Copy)]
struct Locals {
    /// `$this`.
    this: LocalSlotId,
    /// `$index`.
    index: LocalSlotId,
    /// `$other_index`, for the read of another record.
    other_index: LocalSlotId,
    /// `$other`, another object of the same class.
    other: LocalSlotId,
    /// `$spare`, a second buffer to store into the property.
    spare: LocalSlotId,
}

/// Which receiver and index the second read in `rhs` uses.
#[derive(Clone, Copy, PartialEq)]
enum Second {
    /// `$this->slots[$index]` again.
    SameRecord,
    /// `$this->slots[$other_index]`.
    OtherIndex,
    /// `$other->slots[$index]`.
    OtherReceiver,
}

/// A hand-built method and the values the tests inspect.
struct Method {
    function: Function,
    entry: BlockId,
    first_property: ValueId,
    second_property: ValueId,
    first_record: ValueId,
    second_record: ValueId,
    second_field: ValueId,
}

/// Emits a property read of `slots` refined to `reads_heap, may_throw`.
fn emit_slots_read(builder: &mut Builder<'_>, receiver: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::PropGet,
            vec![receiver],
            Some(slots_property()),
            IrType::Heap(IrHeapKind::Buffer),
            packed_buffer_type(),
            Ownership::MaybeOwned,
            Effects::READS_HEAP | Effects::MAY_THROW,
            None,
        )
        .expect("prop_get produces a handle")
}

/// Emits `acquire handle; buffer_get; release handle` for a packed record and returns its address.
fn emit_record_read(builder: &mut Builder<'_>, property: ValueId, index: ValueId) -> ValueId {
    let handle = builder
        .emit(
            Op::Acquire,
            vec![property],
            None,
            IrType::Heap(IrHeapKind::Buffer),
            packed_buffer_type(),
            Ownership::Owned,
        )
        .expect("acquire produces a handle");
    let record = builder
        .emit(
            Op::BufferGet,
            vec![handle, index],
            None,
            IrType::Heap(IrHeapKind::Object),
            PhpType::Packed("Vec2".to_string()),
            Ownership::Borrowed,
        )
        .expect("buffer_get produces a record address");
    builder.emit(Op::Release, vec![handle], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    record
}

/// Emits a packed field read of one record.
fn emit_field_read(builder: &mut Builder<'_>, record: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::PropGet,
            vec![record],
            Some(Immediate::Data(DataId::from_raw(1))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            Op::PropGet.packed_field_access_effects(),
            None,
        )
        .expect("field read produces a value")
}

/// Builds the method `entry: $this->slots[$index]; interpose; cond_br rhs, exit` and
/// `rhs: <second>->slots[...]` followed by a field read of the second record, with `$this` as
/// its receiver parameter in slot 0.
fn method(second: Second, interpose: impl FnOnce(&mut Builder<'_>, Locals, ValueId)) -> Method {
    let mut function = Function::new("Table::matches".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_method = true;
    function.params.push(FunctionParam {
        name: "this".to_string(),
        ir_type: IrType::Heap(IrHeapKind::Object),
        php_type: table_type(),
        by_ref: false,
        variadic: false,
    });
    let (entry, first_property, second_property, first_record, second_record, second_field);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let rhs = b.create_named_block("rhs", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let object = IrType::Heap(IrHeapKind::Object);
        let buffer = IrType::Heap(IrHeapKind::Buffer);
        let locals = Locals {
            this: b.add_local(Some("this".to_string()), object, table_type(), LocalKind::PhpLocal),
            index: b.add_local(Some("index".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal),
            other_index: b.add_local(Some("j".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal),
            other: b.add_local(Some("other".to_string()), object, table_type(), LocalKind::PhpLocal),
            spare: b.add_local(Some("spare".to_string()), buffer, packed_buffer_type(), LocalKind::PhpLocal),
        };

        b.position_at_end(entry);
        let this = b.emit_load_local(locals.this, object, table_type());
        first_property = emit_slots_read(&mut b, this);
        let index = b.emit_load_local(locals.index, IrType::I64, PhpType::Int);
        first_record = emit_record_read(&mut b, first_property, index);
        emit_field_read(&mut b, first_record);
        interpose(&mut b, locals, first_property);
        let cond = b.emit_const_bool(true);
        b.terminate(Terminator::CondBr {
            cond,
            then_target: rhs,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        b.position_at_end(rhs);
        let receiver_slot = if second == Second::OtherReceiver { locals.other } else { locals.this };
        let receiver = b.emit_load_local(receiver_slot, object, table_type());
        let property = emit_slots_read(&mut b, receiver);
        second_property = property;
        let index_slot = if second == Second::OtherIndex { locals.other_index } else { locals.index };
        let index = b.emit_load_local(index_slot, IrType::I64, PhpType::Int);
        second_record = emit_record_read(&mut b, property, index);
        second_field = emit_field_read(&mut b, second_record);
        b.terminate(Terminator::Br { target: exit, args: vec![] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    Method { function, entry, first_property, second_property, first_record, second_record, second_field }
}

/// Runs the module pass over one method and returns the rewritten function.
fn run(method_function: Function) -> (bool, Function) {
    assert!(validate_function(&method_function).is_ok(), "the input method is valid");
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    module.class_methods.push(method_function);
    let changed = hoist_buffer_resolution(&mut module);
    let function = module.class_methods.remove(0);
    assert!(validate_function(&function).is_ok(), "the rewritten method is valid");
    (changed, function)
}

/// Counts the placed instructions of one opcode.
fn count_ops(function: &Function, op: Op) -> usize {
    function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter(|&&inst| function.instructions[inst.as_raw() as usize].op == op)
        .count()
}

/// Counts the placed reads of a buffer-typed property.
fn count_property_reads(function: &Function) -> usize {
    function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .map(|&inst| &function.instructions[inst.as_raw() as usize])
        .filter(|inst| inst.op == Op::PropGet && matches!(inst.result_php_type, PhpType::Buffer(_)))
        .count()
}

/// Returns the opcode that defines `value`.
fn defining_op(function: &Function, value: ValueId) -> Op {
    match function.value(value).expect("value exists").def {
        ValueDef::Instruction { inst, .. } => function.instruction(inst).expect("instruction").op,
        ValueDef::BlockParam { .. } => panic!("value is a block parameter"),
    }
}

/// Returns the instruction that defines `value`.
fn defining_instruction(function: &Function, value: ValueId) -> &crate::ir::Instruction {
    match function.value(value).expect("value exists").def {
        ValueDef::Instruction { inst, .. } => function.instruction(inst).expect("instruction"),
        ValueDef::BlockParam { .. } => panic!("value is a block parameter"),
    }
}

/// Returns a value's original defining instruction ID without rebuilding its scope metadata.
fn defining_id(function: &Function, value: ValueId) -> InstId {
    match function.value(value).expect("value exists").def {
        ValueDef::Instruction { inst, .. } => inst,
        ValueDef::BlockParam { .. } => panic!("value is a block parameter"),
    }
}

/// Distinct lexical scopes can resolve identically named private properties to different slots.
#[test]
fn keeps_property_record_and_descriptor_reuse_apart_across_inlined_scopes() {
    for second in [Second::SameRecord, Second::OtherIndex] {
        for scopes in [
            (Some("Parent"), Some("Table")),
            (Some("Parent"), None),
            (None, Some("Parent")),
        ] {
            let mut built = method(second, |_, _, _| {});
            let first = defining_id(&built.function, built.first_property);
            let later = defining_id(&built.function, built.second_property);
            for (id, scope) in [(first, scopes.0), (later, scopes.1)] {
                if let Some(scope) = scope {
                    built.function.inlined_scopes.insert(id, scope.to_string());
                }
            }
            let original_scopes = built.function.inlined_scopes.clone();
            let (changed, function) = run(built.function);
            assert!(!changed, "different scope entries must keep both access chains");
            assert_eq!(count_property_reads(&function), 2);
            assert_eq!(count_ops(&function, Op::BufferGet), 2);
            assert_eq!(count_ops(&function, Op::BufferResolve), 0);
            assert_eq!(function.instruction(first).expect("first read").op, Op::PropGet);
            assert_eq!(function.instruction(later).expect("second read").op, Op::PropGet);
            assert_eq!(function.inlined_scopes, original_scopes);
        }
    }
}

/// Equal explicit scopes retain the optimization, along with the surviving read's original ID.
#[test]
fn shares_property_reads_from_the_same_inlined_scope() {
    for second in [Second::SameRecord, Second::OtherIndex] {
        let mut built = method(second, |_, _, _| {});
        let first = defining_id(&built.function, built.first_property);
        let later = defining_id(&built.function, built.second_property);
        built.function.inlined_scopes.insert(first, "Parent".to_string());
        built.function.inlined_scopes.insert(later, "Parent".to_string());
        let (changed, function) = run(built.function);
        assert!(changed);
        assert_eq!(count_property_reads(&function), 1);
        assert_eq!(function.instruction(first).expect("original read").op, Op::PropGet);
        assert_eq!(function.inlined_scopes.get(&first).map(String::as_str), Some("Parent"));
        assert_eq!(function.instruction(later).expect("redundant read").op, Op::Nop);
        if second == Second::SameRecord {
            assert_eq!(count_ops(&function, Op::BufferGet), 1);
            assert_eq!(count_ops(&function, Op::BufferResolve), 0);
            assert_eq!(defining_op(&function, built.second_record), Op::Nop);
        } else {
            assert_eq!(count_ops(&function, Op::BufferResolve), 1);
            assert_eq!(count_ops(&function, Op::BufferGetResolved), 2);
        }
    }
}

/// The second `$this->slots[$index]` read reuses the first property read and record address,
/// and its no-op buffer `acquire`/`release` pair goes with it.
#[test]
fn shares_a_repeated_property_read_and_record_address() {
    let built = method(Second::SameRecord, |_, _, _| {});
    let (changed, function) = run(built.function);

    assert!(changed);
    assert_eq!(count_property_reads(&function), 1, "one read of the property");
    assert_eq!(count_ops(&function, Op::BufferGet), 1, "one bounds-checked record address");
    assert_eq!(count_ops(&function, Op::BufferResolve), 0, "a single access needs no descriptor");
    assert_eq!(defining_op(&function, built.second_record), Op::Nop);
    assert_eq!(
        defining_instruction(&function, built.second_field).operands,
        vec![built.first_record],
        "the second field read uses the first record address"
    );
    assert_eq!(count_ops(&function, Op::Acquire), 1, "the second acquire is dropped");
    assert_eq!(count_ops(&function, Op::Release), 1, "with its release");
}

/// A read of another index reuses the property read, and both accesses share one descriptor
/// resolved right before the first of them.
#[test]
fn shares_one_descriptor_across_indexes() {
    let built = method(Second::OtherIndex, |_, _, _| {});
    let (_, function) = run(built.function);

    assert_eq!(count_property_reads(&function), 1);
    assert_eq!(count_ops(&function, Op::BufferGet), 0);
    assert_eq!(count_ops(&function, Op::BufferResolve), 1);
    let first = defining_instruction(&function, built.first_record);
    let second = defining_instruction(&function, built.second_record);
    assert_eq!(first.op, Op::BufferGetResolved);
    assert_eq!(second.op, Op::BufferGetResolved);
    assert_eq!(first.operands[0], second.operands[0], "one descriptor");
    let resolve = defining_instruction(&function, first.operands[0]);
    assert_eq!(resolve.op, Op::BufferResolve);
    assert_eq!(resolve.operands, vec![built.first_property], "it resolves the property read");
    let entry = &function.block(built.entry).expect("entry").instructions;
    let position = |value: ValueId| match function.value(value).expect("value").def {
        ValueDef::Instruction { inst, .. } => entry.iter().position(|&id| id == inst),
        ValueDef::BlockParam { .. } => None,
    };
    assert_eq!(
        position(first.operands[0]).map(|index| index + 1),
        position(built.first_record),
        "the resolver sits right before the first access"
    );
    assert_eq!(count_ops(&function, Op::Acquire), 0, "both handle copies are dropped");
    assert_eq!(count_ops(&function, Op::Release), 0);
}

/// Asserts that whatever `interpose` emits between the two reads keeps them fully apart: the
/// property is read again, the record is resolved again, and no descriptor is shared.
fn assert_reads_stay_apart(name: &str, interpose: impl FnOnce(&mut Builder<'_>, Locals, ValueId)) {
    let built = method(Second::SameRecord, interpose);
    let (_, function) = run(built.function);
    assert_eq!(count_property_reads(&function), 2, "{name}: the property is read again");
    assert_eq!(count_ops(&function, Op::BufferGet), 2, "{name}: the record is resolved again");
    assert_eq!(count_ops(&function, Op::BufferResolve), 0, "{name}: no shared descriptor");
    assert_eq!(defining_op(&function, built.second_record), Op::BufferGet, "{name}");
}

/// A store to the property through another reference can replace the buffer the receiver sees.
#[test]
fn keeps_reads_apart_across_a_property_store_through_another_reference() {
    assert_reads_stay_apart("prop_set", |b, locals, _| {
        let object = IrType::Heap(IrHeapKind::Object);
        let buffer = IrType::Heap(IrHeapKind::Buffer);
        let other = b.emit_load_local(locals.other, object, table_type());
        let spare = b.emit_load_local(locals.spare, buffer, packed_buffer_type());
        b.emit(
            Op::PropSet,
            vec![other, spare],
            Some(slots_property()),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    });
}

/// `unset()` of the property makes the next read throw instead of returning the handle.
#[test]
fn keeps_reads_apart_across_unset() {
    assert_reads_stay_apart("prop_unset", |b, locals, _| {
        let this = b.emit_load_local(locals.this, IrType::Heap(IrHeapKind::Object), table_type());
        b.emit(
            Op::PropUnset,
            vec![this],
            Some(slots_property()),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    });
}

/// `buffer_free()` makes the second access report the use-after-free.
#[test]
fn keeps_reads_apart_across_buffer_free() {
    assert_reads_stay_apart("buffer_free", |b, _, property| {
        b.emit_with_effects(
            Op::RuntimeCall,
            vec![property],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::BufferFree))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            RuntimeFnId::BufferFree.effects(),
            None,
        );
    });
}

/// A property read on a receiver other than `$this` may warn on null even though effect
/// refinement leaves its effects at `reads_heap, may_throw`, and the warning can run a user error
/// handler that replaces or frees the buffer.
#[test]
fn keeps_reads_apart_across_a_property_read_of_another_receiver() {
    assert_reads_stay_apart("prop_get", |b, locals, _| {
        let other = b.emit_load_local(locals.other, IrType::Heap(IrHeapKind::Object), table_type());
        b.emit_with_effects(
            Op::PropGet,
            vec![other],
            Some(Immediate::Data(DataId::from_raw(3))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            Effects::READS_HEAP | Effects::MAY_THROW,
            None,
        );
    });
}

/// An ordinary object's immediately checked buffer reads can also share their successful path.
#[test]
fn buffer_object_loop_shares_checked_reads_without_a_method_receiver() {
    let mut built = method(Second::SameRecord, |_, _, _| {});
    built.function.flags.is_method = false;
    built.function.params.clear();
    let (changed, function) = run(built.function);
    assert!(changed);
    assert_eq!(count_property_reads(&function), 1);
    assert_eq!(count_ops(&function, Op::BufferGet), 1);
}

/// Even a user call whose effect summary is read-only keeps the reads apart: the region never
/// trusts a callee with the caller's frame.
#[test]
fn keeps_reads_apart_across_a_read_only_call() {
    assert_reads_stay_apart("call", |b, _, _| {
        b.emit_with_effects(
            Op::Call,
            vec![],
            Some(Immediate::Data(DataId::from_raw(2))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Effects::READS_HEAP,
            None,
        );
    });
}

/// Writing the index slot between the reads makes them address different records, so the second
/// keeps its own bounds-checked address (both may still share the descriptor).
#[test]
fn keeps_record_addresses_apart_when_the_index_changes() {
    let built = method(Second::SameRecord, |b, locals, _| {
        let three = b.emit_const_i64(3);
        b.emit_store_local(locals.index, three);
    });
    let (_, function) = run(built.function);
    assert_eq!(count_property_reads(&function), 1, "the property itself is unchanged");
    assert_eq!(defining_op(&function, built.second_record), Op::BufferGetResolved);
    assert_eq!(count_ops(&function, Op::BufferGetResolved), 2, "two records");
    assert_eq!(count_ops(&function, Op::BufferResolve), 1);
}

/// Another receiver's property can name another buffer, so nothing is shared.
#[test]
fn keeps_reads_of_another_receiver_apart() {
    let built = method(Second::OtherReceiver, |_, _, _| {});
    let (changed, function) = run(built.function);
    assert!(!changed);
    assert_eq!(count_property_reads(&function), 2);
    assert_eq!(count_ops(&function, Op::BufferGet), 2);
}

/// Repeated accesses through a local buffer handle in straight-line code are left alone.
#[test]
fn leaves_local_handles_alone() {
    let mut function = Function::new("local".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        let slot = b.add_local(
            Some("b".to_string()),
            IrType::Heap(IrHeapKind::Buffer),
            packed_buffer_type(),
            LocalKind::PhpLocal,
        );
        b.position_at_end(entry);
        let zero = b.emit_const_i64(0);
        for _ in 0..2 {
            let handle = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Buffer), packed_buffer_type());
            let record = b
                .emit(
                    Op::BufferGet,
                    vec![handle, zero],
                    None,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Packed("Vec2".to_string()),
                    Ownership::Borrowed,
                )
                .expect("record address");
            emit_field_read(&mut b, record);
        }
        b.terminate(Terminator::Return { value: None });
    }
    let (changed, function) = run(function);
    assert!(!changed);
    assert_eq!(count_ops(&function, Op::BufferGet), 2);
    assert_eq!(count_ops(&function, Op::BufferResolve), 0);
}

/// Builds a method that reads `$this->slots[$index]` in `entry` and again at the top of a loop
/// body, where `after_second` emits what follows the second read before the back edge, so a
/// later iteration's read runs after that code without passing `entry` again.
fn loop_method(after_second: impl FnOnce(&mut Builder<'_>, Locals, ValueId)) -> Method {
    let mut function = Function::new("Table::scan".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_method = true;
    function.params.push(FunctionParam {
        name: "this".to_string(),
        ir_type: IrType::Heap(IrHeapKind::Object),
        php_type: table_type(),
        by_ref: false,
        variadic: false,
    });
    let (entry, first_property, second_property, first_record, second_record, second_field);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let object = IrType::Heap(IrHeapKind::Object);
        let buffer = IrType::Heap(IrHeapKind::Buffer);
        let locals = Locals {
            this: b.add_local(Some("this".to_string()), object, table_type(), LocalKind::PhpLocal),
            index: b.add_local(Some("index".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal),
            other_index: b.add_local(Some("j".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal),
            other: b.add_local(Some("other".to_string()), object, table_type(), LocalKind::PhpLocal),
            spare: b.add_local(Some("spare".to_string()), buffer, packed_buffer_type(), LocalKind::PhpLocal),
        };

        b.position_at_end(entry);
        let this = b.emit_load_local(locals.this, object, table_type());
        first_property = emit_slots_read(&mut b, this);
        let index = b.emit_load_local(locals.index, IrType::I64, PhpType::Int);
        first_record = emit_record_read(&mut b, first_property, index);
        emit_field_read(&mut b, first_record);
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let counter = b.block_param(header, 0);
        let cond = b.emit_const_bool(true);
        b.terminate(Terminator::CondBr {
            cond,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        b.position_at_end(body);
        let receiver = b.emit_load_local(locals.this, object, table_type());
        let property = emit_slots_read(&mut b, receiver);
        second_property = property;
        let index = b.emit_load_local(locals.index, IrType::I64, PhpType::Int);
        second_record = emit_record_read(&mut b, property, index);
        second_field = emit_field_read(&mut b, second_record);
        after_second(&mut b, locals, property);
        let one = b.emit_const_i64(1);
        let next = b.emit_iadd(counter, one);
        b.terminate(Terminator::Br { target: header, args: vec![next] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    Method { function, entry, first_property, second_property, first_record, second_record, second_field }
}

/// A read before a loop serves the same read at the top of a body that leaves the property and
/// the buffer alone on every iteration.
#[test]
fn shares_a_read_before_a_loop_with_the_loop_body() {
    let built = loop_method(|_, _, _| {});
    let (changed, function) = run(built.function);
    assert!(changed);
    assert_eq!(count_property_reads(&function), 1);
    assert_eq!(count_ops(&function, Op::BufferGet), 1);
    assert_eq!(
        defining_instruction(&function, built.second_field).operands,
        vec![built.first_record],
    );
}

/// A cached property handle still reuses its entry record before loop resolution can rewrite it.
#[test]
fn shares_a_cached_property_record_before_loop_resolution() {
    let mut built = loop_method(|_, _, _| {});
    let handle = defining_instruction(&built.function, built.second_record).operands[0];
    let property = defining_instruction(&built.function, handle).operands[0];
    let ValueDef::Instruction { inst: property_inst, .. } = built.function.values[property.as_raw() as usize].def
        else { unreachable!("property read defines the handle"); };
    crate::ir_passes::rewrite::replace_all_uses(
        &mut built.function, &std::collections::HashMap::from([(property, built.first_property)]),
    );
    crate::ir_passes::rewrite::neutralize_to_nop(&mut built.function.instructions[property_inst.as_raw() as usize]);
    let (changed, function) = run(built.function);
    assert!(changed);
    assert_eq!(count_ops(&function, Op::BufferGet), 1);
    assert_eq!(count_ops(&function, Op::BufferGetResolved), 0);
    assert_eq!(count_ops(&function, Op::BufferResolve), 0);
    assert_eq!(defining_instruction(&function, built.second_field).operands, vec![built.first_record]);
}

/// Code after the second read in the loop body runs before that read's next iteration, so a
/// property store or a `buffer_free()` there keeps every iteration's read apart from the one
/// before the loop, although nothing separates the two on the path into the first iteration.
#[test]
fn keeps_a_loop_read_apart_from_writes_later_in_the_body() {
    let store = loop_method(|b, locals, _| {
        let other = b.emit_load_local(locals.other, IrType::Heap(IrHeapKind::Object), table_type());
        let spare =
            b.emit_load_local(locals.spare, IrType::Heap(IrHeapKind::Buffer), packed_buffer_type());
        b.emit(
            Op::PropSet,
            vec![other, spare],
            Some(slots_property()),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    });
    let free = loop_method(|b, _, property| {
        b.emit_with_effects(
            Op::RuntimeCall,
            vec![property],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::BufferFree))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            RuntimeFnId::BufferFree.effects(),
            None,
        );
    });
    for (name, built) in [("prop_set", store), ("buffer_free", free)] {
        let (_, function) = run(built.function);
        assert_eq!(count_property_reads(&function), 2, "{name}: the loop reads the property again");
        assert_eq!(count_ops(&function, Op::BufferGet), 2, "{name}: and resolves it again");
        assert_eq!(defining_op(&function, built.second_record), Op::BufferGet, "{name}");
    }
}
