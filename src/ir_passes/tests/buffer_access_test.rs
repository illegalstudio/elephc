//! Purpose:
//! Tests for buffer resolution hoisting and packed record address sharing on hand-built loops:
//! the preheader descriptor for an SSA or local-slot handle, loops with several back edges,
//! handles that change between iterations, address sharing across a join, and the loops and
//! modules the pass must leave alone. Same-buffer induction proofs cover accepted and refused
//! bounds-check elimination shapes, and preheader placement tests pin where the resolver goes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder` and run through
//!   `hoist_buffer_resolution` directly, so each test controls the exact loop shape the pass
//!   sees. The end-to-end behaviour (fatals, outputs, `--ir-opt=off` parity) lives in
//!   `tests/codegen/optimizer/buffer_access.rs`.
//! - Loop headers branch on a constant so no other pass is needed to keep the loop alive.

use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{
    validate_function, BlockId, Builder, CmpPredicate, Function, Immediate, InstId, IrHeapKind,
    IrType, LocalKind, LocalSlotId, Module, Op, Ownership, PcntlRuntime, RuntimeCallTarget,
    RuntimeFnId, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::buffer_access::hoist_buffer_resolution;
use crate::types::PhpType;

/// The PHP type of a `buffer<int>`.
fn int_buffer_type() -> PhpType {
    PhpType::Buffer(Box::new(PhpType::Int))
}

/// The PHP type of a `buffer<Vec2>` whose elements are packed records.
fn packed_buffer_type() -> PhpType {
    PhpType::Buffer(Box::new(PhpType::Packed("Vec2".to_string())))
}

/// Declares the `$b` buffer local and returns its slot.
fn add_buffer_local(builder: &mut Builder<'_>, php_type: PhpType) -> LocalSlotId {
    builder.add_local(
        Some("b".to_string()),
        IrType::Heap(IrHeapKind::Buffer),
        php_type,
        LocalKind::PhpLocal,
    )
}

/// Emits `load_local $b` for a buffer slot.
fn load_buffer(builder: &mut Builder<'_>, slot: LocalSlotId, php_type: PhpType) -> ValueId {
    builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Buffer), php_type)
}

/// Emits a scalar `buffer_get handle, index`.
fn emit_int_get(builder: &mut Builder<'_>, handle: ValueId, index: ValueId) -> ValueId {
    builder
        .emit(Op::BufferGet, vec![handle, index], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
        .expect("buffer_get produces a value")
}

/// Emits a packed `buffer_get handle, index`, which yields the record address.
fn emit_record_get(builder: &mut Builder<'_>, handle: ValueId, index: ValueId) -> ValueId {
    builder
        .emit(
            Op::BufferGet,
            vec![handle, index],
            None,
            IrType::Heap(IrHeapKind::Object),
            PhpType::Packed("Vec2".to_string()),
            Ownership::Borrowed,
        )
        .expect("buffer_get produces a record address")
}

/// Emits `buffer_set handle, index, value`.
fn emit_set(builder: &mut Builder<'_>, handle: ValueId, index: ValueId, value: ValueId) {
    builder.emit(
        Op::BufferSet,
        vec![handle, index, value],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// Emits the production typed `runtime.buffer_len handle` call as a scalar integer.
fn emit_buffer_len(builder: &mut Builder<'_>, handle: ValueId) -> ValueId {
    builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![handle],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::BufferLen,
            ))),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            RuntimeFnId::BufferLen.effects(),
            None,
        )
        .expect("buffer_len produces a value")
}

/// Source used for a hand-built loop's upper bound.
#[derive(Clone, Copy)]
enum LoopBound {
    /// Length of the buffer being accessed.
    SameBufferLength,
    /// Length of a distinct buffer.
    OtherBufferLength,
    /// Unrelated scalar local value.
    UnrelatedValue,
}

/// Builds a canonical buffer loop with controllable induction, bound source, and access index.
fn bounded_buffer_loop(
    start: i64,
    step: i64,
    predicate: CmpPredicate,
    index_offset: i64,
    loop_bound: LoopBound,
    cached_length: bool,
    write: bool,
) -> Function {
    bounded_buffer_loop_with_slot_write(
        start,
        step,
        predicate,
        index_offset,
        loop_bound,
        cached_length,
        write,
        false,
    )
}

/// Builds a canonical buffer loop and optionally writes its handle slot after caching the bound.
fn bounded_buffer_loop_with_slot_write(
    start: i64,
    step: i64,
    predicate: CmpPredicate,
    index_offset: i64,
    loop_bound: LoopBound,
    cached_length: bool,
    write: bool,
    write_handle_slot: bool,
) -> Function {
    let mut function = Function::new("bounded".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, int_buffer_type());
        let other_slot = b.add_local(
            Some("other".to_string()),
            IrType::Heap(IrHeapKind::Buffer),
            int_buffer_type(),
            LocalKind::PhpLocal,
        );
        let unrelated_slot = b.add_local(
            Some("n".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );

        b.position_at_end(entry);
        let handle = load_buffer(&mut b, slot, int_buffer_type());
        let bound_handle = match loop_bound {
            LoopBound::SameBufferLength => Some(handle),
            LoopBound::OtherBufferLength => {
                Some(load_buffer(&mut b, other_slot, int_buffer_type()))
            }
            LoopBound::UnrelatedValue => None,
        };
        let length = if let Some(bound_handle) = bound_handle {
            cached_length.then(|| emit_buffer_len(&mut b, bound_handle))
        } else {
            Some(b.emit_load_local(unrelated_slot, IrType::I64, PhpType::Int))
        };
        if write_handle_slot {
            b.emit_store_local(slot, handle);
        }
        let initial = b.emit_const_i64(start);
        b.terminate(Terminator::Br { target: header, args: vec![initial] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        let bound = length.unwrap_or_else(|| {
            emit_buffer_len(&mut b, bound_handle.expect("buffer length bound has a handle"))
        });
        let condition = b
            .emit(
                Op::ICmp,
                vec![index, bound],
                Some(Immediate::CmpPredicate(predicate)),
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            )
            .expect("icmp produces a value");
        b.terminate(Terminator::CondBr {
            cond: condition,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        b.position_at_end(body);
        let access_index = if index_offset == 0 {
            index
        } else {
            let offset = b.emit_const_i64(index_offset);
            b.emit_iadd(index, offset)
        };
        let access_handle = if cached_length {
            load_buffer(&mut b, slot, int_buffer_type())
        } else {
            handle
        };
        let value = emit_int_get(&mut b, access_handle, access_index);
        if write {
            emit_set(&mut b, access_handle, access_index, value);
        }
        let delta = b.emit_const_i64(step);
        let next = b.emit_iadd(index, delta);
        b.terminate(Terminator::Br { target: header, args: vec![next] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    function
}

/// Returns the proven-in-range flags attached to every resolved buffer access.
fn resolved_in_bounds_flags(function: &Function) -> Vec<bool> {
    function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.op, Op::BufferGetResolved | Op::BufferSetResolved))
        .map(|inst| inst.immediate == Some(Immediate::Bool(true)))
        .collect()
}

/// Terminates the current block with `cond_br true -> then_target(then_args) / else_target`.
fn branch(
    builder: &mut Builder<'_>,
    then_target: BlockId,
    then_args: Vec<ValueId>,
    else_target: BlockId,
    else_args: Vec<ValueId>,
) {
    let cond = builder.emit_const_bool(true);
    builder.terminate(Terminator::CondBr { cond, then_target, then_args, else_target, else_args });
}

/// Ends the current block with `i + 1` passed back to the loop header.
fn back_edge(builder: &mut Builder<'_>, header: BlockId, index: ValueId) {
    let one = builder.emit_const_i64(1);
    let next = builder.emit_iadd(index, one);
    builder.terminate(Terminator::Br { target: header, args: vec![next] });
}

/// Wraps one function in a fresh module so the module-level pass can run on it.
fn module_with(function: Function) -> Module {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    module.functions.push(function);
    module
}

/// Returns the instruction ids of one block, in order.
fn block_instructions(function: &Function, block: BlockId) -> Vec<InstId> {
    function.block(block).expect("block exists").instructions.clone()
}

/// Returns the instruction that defines `value`.
fn defining_op(function: &Function, value: ValueId) -> Op {
    match function.value(value).expect("value exists").def {
        ValueDef::Instruction { inst, .. } => function.instruction(inst).expect("instruction exists").op,
        ValueDef::BlockParam { .. } => panic!("value is a block parameter"),
    }
}

/// Counts the instructions of one opcode across the whole function.
fn count_ops(function: &Function, op: Op) -> usize {
    function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter(|&&inst| function.instructions[inst.as_raw() as usize].op == op)
        .count()
}

/// Builds `entry -> header(i) -> body -> latch -> header`, with `body` filled by `fill`.
///
/// `fill` receives the builder positioned in `body`, the buffer handle loaded in `entry` and the
/// header's index parameter, and must leave the block unterminated.
fn simple_loop(
    php_type: PhpType,
    fill: impl FnOnce(&mut Builder<'_>, ValueId, ValueId),
) -> (Function, BlockId, BlockId) {
    let mut function = Function::new("loop".to_string(), IrType::Void, PhpType::Void);
    let (entry, body);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        body = b.create_named_block("body", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, php_type.clone());

        b.position_at_end(entry);
        let handle = load_buffer(&mut b, slot, php_type);
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        fill(&mut b, handle, index);
        b.terminate(Terminator::Br { target: latch, args: vec![] });

        b.position_at_end(latch);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    (function, entry, body)
}

/// A loop whose handle is defined before the loop gets one `buffer_resolve` in the preheader,
/// and both its read and its write go through that descriptor.
#[test]
fn hoists_an_invariant_handle_into_the_preheader() {
    let (function, entry, body) = simple_loop(int_buffer_type(), |b, handle, index| {
        let value = emit_int_get(b, handle, index);
        emit_set(b, handle, index, value);
    });
    assert!(validate_function(&function).is_ok(), "the input loop is valid");
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module), "the invariant handle is hoisted");
    let function = &module.functions[0];
    let resolves: Vec<InstId> = block_instructions(function, entry)
        .into_iter()
        .filter(|&inst| function.instructions[inst.as_raw() as usize].op == Op::BufferResolve)
        .collect();
    assert_eq!(resolves.len(), 1, "one descriptor in the preheader");
    let descriptor = function.instructions[resolves[0].as_raw() as usize].result.expect("descriptor");
    let accesses: Vec<_> = block_instructions(function, body)
        .into_iter()
        .map(|inst| &function.instructions[inst.as_raw() as usize])
        .filter(|inst| matches!(inst.op, Op::BufferGetResolved | Op::BufferSetResolved))
        .collect();
    assert_eq!(accesses.len(), 2, "the read and the write are rewritten");
    assert!(accesses.iter().all(|inst| inst.operands[0] == descriptor), "both use the descriptor");
    assert_eq!(count_ops(function, Op::BufferGet) + count_ops(function, Op::BufferSet), 0);
    assert!(validate_function(function).is_ok(), "the rewritten loop is valid");
    assert!(!hoist_buffer_resolution(&mut module), "a second run finds nothing to rewrite");
}

/// Builds `entry -> header(i) -> body -> header` whose preheader `entry` is filled by
/// `fill_entry` before it branches to the header.
///
/// `fill_entry` receives the builder positioned in `entry` and the `$b` buffer slot, and returns
/// the SSA handle the body should read through (or `None` to reload `$b` inside the body) and a
/// float the body uses on every iteration, in the role of a `float $dt` load LICM hoisted into the
/// preheader. Returns the function, the preheader, and that float value.
fn preheader_loop(
    fill_entry: impl FnOnce(&mut Builder<'_>, LocalSlotId) -> (Option<ValueId>, ValueId),
) -> (Function, BlockId, ValueId) {
    let mut function = Function::new("preheader".to_string(), IrType::Void, PhpType::Void);
    let (entry, invariant);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, int_buffer_type());

        b.position_at_end(entry);
        let (ssa_handle, float) = fill_entry(&mut b, slot);
        invariant = float;
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        let handle = ssa_handle.unwrap_or_else(|| load_buffer(&mut b, slot, int_buffer_type()));
        emit_int_get(&mut b, handle, index);
        b.emit(Op::FAdd, vec![float, float], None, IrType::F64, PhpType::Float, Ownership::NonHeap);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    (function, entry, invariant)
}

/// Emits `load_local $dt` for a fresh float local, in the role of a hoisted float parameter.
fn load_float_local(builder: &mut Builder<'_>) -> ValueId {
    let slot = builder.add_local(Some("dt".to_string()), IrType::F64, PhpType::Float, LocalKind::PhpLocal);
    builder.emit_load_local(slot, IrType::F64, PhpType::Float)
}

/// Returns the position of `value`'s defining instruction inside `block`.
fn position_in_block(function: &Function, block: BlockId, value: ValueId) -> usize {
    let ValueDef::Instruction { inst, .. } = function.value(value).expect("value exists").def else {
        panic!("value is a block parameter");
    };
    block_instructions(function, block)
        .iter()
        .position(|id| *id == inst)
        .expect("value is defined in the block")
}

/// Returns the position of the first instruction of `op` inside `block`.
fn op_position(function: &Function, block: BlockId, op: Op) -> usize {
    block_instructions(function, block)
        .iter()
        .position(|id| function.instructions[id.as_raw() as usize].op == op)
        .unwrap_or_else(|| panic!("block has no {op:?}"))
}

/// With nothing in the preheader able to free or rebind a buffer, the slot load and the resolver
/// call go in front of it, ahead of the loop-invariant float load, so that float does not live
/// across the call. Every shifted instruction keeps a definition index that validates.
#[test]
fn resolves_ahead_of_invariant_preheader_loads() {
    let (function, entry, dt) = preheader_loop(|b, _| {
        let dt = load_float_local(b);
        b.emit_const_f64(0.5);
        (None, dt)
    });
    assert!(validate_function(&function).is_ok(), "the input loop is valid");
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    let resolve = op_position(function, entry, Op::BufferResolve);
    assert_eq!(resolve, 1, "the slot load and the resolver open the preheader");
    assert_eq!(op_position(function, entry, Op::LoadLocal), 0, "the handle load comes first");
    assert!(resolve < position_in_block(function, entry, dt), "the float is loaded after the call");
    assert_eq!(count_ops(function, Op::BufferGetResolved), 1);
    assert!(validate_function(function).is_ok(), "shifted definitions keep their indices");
}

/// A preheader store to the handle slot rebinds the buffer, so the resolver follows it and still
/// precedes the float load placed after it.
#[test]
fn resolves_after_a_preheader_write_of_the_handle_slot() {
    let (function, entry, dt) = preheader_loop(|b, slot| {
        let other = b.add_local(
            Some("c".to_string()),
            IrType::Heap(IrHeapKind::Buffer),
            int_buffer_type(),
            LocalKind::PhpLocal,
        );
        let replacement = load_buffer(b, other, int_buffer_type());
        b.emit_store_local(slot, replacement);
        (None, load_float_local(b))
    });
    assert!(validate_function(&function).is_ok(), "the input loop is valid");
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    let store = op_position(function, entry, Op::StoreLocal);
    let resolve = op_position(function, entry, Op::BufferResolve);
    assert_eq!(resolve, store + 2, "the slot is reloaded and resolved right after the store");
    assert!(resolve < position_in_block(function, entry, dt), "the float is loaded after the call");
    assert!(validate_function(function).is_ok());
}

/// An SSA handle defined in the preheader is resolved right after its definition: earlier would
/// read an undefined value, later would keep the following float live across the call.
#[test]
fn resolves_an_ssa_handle_right_after_its_definition() {
    let mut handle = None;
    let (function, entry, dt) = preheader_loop(|b, slot| {
        b.emit_const_i64(7);
        let loaded = load_buffer(b, slot, int_buffer_type());
        handle = Some(loaded);
        (Some(loaded), load_float_local(b))
    });
    let handle = handle.expect("the preheader defined the handle");
    assert!(validate_function(&function).is_ok(), "the input loop is valid");
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    let resolve = op_position(function, entry, Op::BufferResolve);
    assert_eq!(resolve, position_in_block(function, entry, handle) + 1);
    assert!(resolve < position_in_block(function, entry, dt), "the float is loaded after the call");
    assert!(validate_function(function).is_ok());
}

/// Returns the instruction defining the descriptor operand of one resolved access result.
fn descriptor_definition(function: &Function, access: ValueId) -> &crate::ir::Instruction {
    let ValueDef::Instruction { inst, .. } = function.value(access).expect("access").def else {
        panic!("access is an instruction result");
    };
    let descriptor = function.instructions[inst.as_raw() as usize].operands[0];
    let ValueDef::Instruction { inst, .. } = function.value(descriptor).expect("descriptor").def
    else {
        panic!("descriptor is an instruction result");
    };
    &function.instructions[inst.as_raw() as usize]
}

/// A loop over two buffers and the values its tests inspect.
struct TwoHandleLoop {
    function: Function,
    entry: BlockId,
    /// Handle of the second buffer, loaded in the preheader after a float.
    ssa_handle: ValueId,
    /// Read through the first buffer, whose slot the body reloads.
    slot_access: ValueId,
    /// Read through the preheader SSA handle.
    ssa_access: ValueId,
}

/// Builds `entry: $dt; $c; -> header(i) -> body -> header` whose body reads a reloaded `$b` slot
/// handle and the preheader `$c` handle, in the order `ssa_first` selects.
fn two_handle_loop(ssa_first: bool) -> TwoHandleLoop {
    let mut function = Function::new("two_buffers".to_string(), IrType::Void, PhpType::Void);
    let (entry, ssa_handle, slot_access, ssa_access);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let reloaded_slot = add_buffer_local(&mut b, int_buffer_type());
        let ssa_slot = b.add_local(
            Some("c".to_string()),
            IrType::Heap(IrHeapKind::Buffer),
            int_buffer_type(),
            LocalKind::PhpLocal,
        );

        b.position_at_end(entry);
        let dt = load_float_local(&mut b);
        ssa_handle = load_buffer(&mut b, ssa_slot, int_buffer_type());
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        if ssa_first {
            ssa_access = emit_int_get(&mut b, ssa_handle, index);
            let reloaded = load_buffer(&mut b, reloaded_slot, int_buffer_type());
            slot_access = emit_int_get(&mut b, reloaded, index);
        } else {
            let reloaded = load_buffer(&mut b, reloaded_slot, int_buffer_type());
            slot_access = emit_int_get(&mut b, reloaded, index);
            ssa_access = emit_int_get(&mut b, ssa_handle, index);
        }
        b.emit(Op::FAdd, vec![dt, dt], None, IrType::F64, PhpType::Float, Ownership::NonHeap);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    TwoHandleLoop { function, entry, ssa_handle, slot_access, ssa_access }
}

/// Hoists a two-handle loop and checks what both access orders share: each access reads its own
/// handle's descriptor, and every definition shifted by both insertions still validates. Returns
/// the rewritten function with the preheader positions of its resolvers.
fn hoist_two_handle_loop(ssa_first: bool) -> (Function, TwoHandleLoop, Vec<usize>) {
    let built = two_handle_loop(ssa_first);
    assert!(validate_function(&built.function).is_ok(), "the input loop is valid");
    let mut module = module_with(built.function.clone());

    assert!(hoist_buffer_resolution(&mut module));
    let function = module.functions.remove(0);
    let resolves: Vec<usize> = block_instructions(&function, built.entry)
        .iter()
        .enumerate()
        .filter(|(_, id)| function.instructions[id.as_raw() as usize].op == Op::BufferResolve)
        .map(|(position, _)| position)
        .collect();
    let slot_resolve = descriptor_definition(&function, built.slot_access);
    let ssa_resolve = descriptor_definition(&function, built.ssa_access);
    assert_eq!(slot_resolve.op, Op::BufferResolve);
    assert_eq!(defining_op(&function, slot_resolve.operands[0]), Op::LoadLocal);
    assert_eq!(ssa_resolve.op, Op::BufferResolve);
    assert_eq!(ssa_resolve.operands, vec![built.ssa_handle], "the SSA handle resolves itself");
    assert!(validate_function(&function).is_ok(), "shifted definitions keep their indices");
    (function, built, resolves)
}

/// When the slot handle is read first, its reload and resolver open the preheader, and the SSA
/// handle defined later resolves right after its own definition.
#[test]
fn resolves_a_slot_handle_first_when_the_loop_reads_it_first() {
    let (function, built, resolves) = hoist_two_handle_loop(false);
    assert_eq!(resolves, vec![1, 4], "slot load and resolver first, then after the SSA handle");
    assert_eq!(position_in_block(&function, built.entry, built.ssa_handle), 3);
}

/// When the SSA handle is read first, its resolver follows its definition and the floor keeps the
/// later slot reload and resolver after it, instead of at the preheader start where nothing else
/// would stop them.
#[test]
fn keeps_a_later_slot_resolver_after_an_earlier_group() {
    let (function, built, resolves) = hoist_two_handle_loop(true);
    assert_eq!(position_in_block(&function, built.entry, built.ssa_handle), 1);
    assert_eq!(resolves, vec![2, 4], "the SSA resolver first, then the slot reload and resolver");
    let entry = block_instructions(&function, built.entry);
    let reload = &function.instructions[entry[3].as_raw() as usize];
    assert_eq!(reload.op, Op::LoadLocal, "the slot reload sits right before its resolver");
    assert_eq!(
        descriptor_definition(&function, built.slot_access).operands,
        vec![reload.result.expect("the reload defines the handle")],
    );
}

/// Zero-based unit-step loops bounded by the same buffer length mark reads and writes in range,
/// including when the length is computed once before the loop.
#[test]
fn proves_same_buffer_length_bounds_for_resolved_accesses() {
    for (cached_length, write) in [(false, false), (true, true)] {
        let function = bounded_buffer_loop(
            0,
            1,
            CmpPredicate::Slt,
            0,
            LoopBound::SameBufferLength,
            cached_length,
            write,
        );
        assert!(validate_function(&function).is_ok(), "the input loop is valid");
        let mut module = module_with(function);

        assert!(hoist_buffer_resolution(&mut module));
        let function = &module.functions[0];
        let flags = resolved_in_bounds_flags(function);
        assert!(!flags.is_empty(), "the loop accesses were rewritten");
        assert!(flags.iter().all(|flag| *flag), "every exact induction access is proven");
        assert!(validate_function(function).is_ok(), "the proven loop remains valid");
    }
}

/// A self-loop compares its index only after the header access, so reflexive dominance must not
/// mark that access in range even when the back edge is guarded by the same buffer's length.
#[test]
fn self_loop_header_access_is_not_proven_in_bounds() {
    let mut function = Function::new("self_loop".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, int_buffer_type());

        b.position_at_end(entry);
        let handle = load_buffer(&mut b, slot, int_buffer_type());
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        emit_int_get(&mut b, handle, index);
        let length = emit_buffer_len(&mut b, handle);
        let condition = b
            .emit(
                Op::ICmp,
                vec![index, length],
                Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            )
            .expect("icmp produces a value");
        let one = b.emit_const_i64(1);
        let next = b.emit_iadd(index, one);
        b.terminate(Terminator::CondBr {
            cond: condition,
            then_target: header,
            then_args: vec![next],
            else_target: exit,
            else_args: vec![],
        });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    assert!(validate_function(&function).is_ok(), "the input self-loop is valid");
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module), "resolution is still hoisted");
    let function = &module.functions[0];
    assert_eq!(resolved_in_bounds_flags(function), vec![false]);
    assert!(validate_function(function).is_ok(), "the rewritten self-loop remains valid");
}

/// Inclusive, negative-start, non-unit, decreasing, offset, and other-buffer loops retain their
/// resolved-access bounds checks.
#[test]
fn refuses_unsound_buffer_index_proofs() {
    let cases = [
        (
            "unrelated bound",
            bounded_buffer_loop(
                0,
                1,
                CmpPredicate::Slt,
                0,
                LoopBound::UnrelatedValue,
                false,
                false,
            ),
        ),
        (
            "other buffer length",
            bounded_buffer_loop(
                0,
                1,
                CmpPredicate::Slt,
                0,
                LoopBound::OtherBufferLength,
                false,
                false,
            ),
        ),
        (
            "inclusive bound",
            bounded_buffer_loop(
                0,
                1,
                CmpPredicate::Sle,
                0,
                LoopBound::SameBufferLength,
                false,
                false,
            ),
        ),
        (
            "negative start",
            bounded_buffer_loop(
                -1,
                1,
                CmpPredicate::Slt,
                0,
                LoopBound::SameBufferLength,
                false,
                false,
            ),
        ),
        (
            "non-unit step",
            bounded_buffer_loop(
                0,
                2,
                CmpPredicate::Slt,
                0,
                LoopBound::SameBufferLength,
                false,
                false,
            ),
        ),
        (
            "decreasing step",
            bounded_buffer_loop(
                0,
                -1,
                CmpPredicate::Slt,
                0,
                LoopBound::SameBufferLength,
                false,
                false,
            ),
        ),
        (
            "offset index",
            bounded_buffer_loop(
                0,
                1,
                CmpPredicate::Slt,
                1,
                LoopBound::SameBufferLength,
                false,
                false,
            ),
        ),
        (
            "cached length from written slot",
            bounded_buffer_loop_with_slot_write(
                0,
                1,
                CmpPredicate::Slt,
                0,
                LoopBound::SameBufferLength,
                true,
                true,
                true,
            ),
        ),
    ];
    for (name, function) in cases {
        assert!(validate_function(&function).is_ok(), "{name}: the input loop is valid");
        let mut module = module_with(function);

        assert!(hoist_buffer_resolution(&mut module), "{name}: resolution is still hoisted");
        let function = &module.functions[0];
        let flags = resolved_in_bounds_flags(function);
        assert!(!flags.is_empty(), "{name}: the loop access was rewritten");
        assert!(flags.iter().all(|flag| !*flag), "{name}: the bounds check must remain");
        assert!(validate_function(function).is_ok(), "{name}: the rewritten loop remains valid");
    }
}

/// A handle reloaded from a slot inside the loop is loaded once in the preheader instead, and
/// the in-loop load it replaced becomes dead.
#[test]
fn hoists_a_handle_loaded_from_an_unwritten_slot() {
    let mut function = Function::new("slot".to_string(), IrType::Void, PhpType::Void);
    let (entry, in_loop_load);
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, int_buffer_type());

        b.position_at_end(entry);
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        in_loop_load = load_buffer(&mut b, slot, int_buffer_type());
        emit_int_get(&mut b, in_loop_load, index);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    let entry_ops: Vec<Op> = block_instructions(function, entry)
        .into_iter()
        .map(|inst| function.instructions[inst.as_raw() as usize].op)
        .collect();
    assert!(entry_ops.contains(&Op::LoadLocal), "the handle is loaded in the preheader");
    assert!(entry_ops.contains(&Op::BufferResolve), "and resolved there");
    assert_eq!(defining_op(function, in_loop_load), Op::Nop, "the in-loop load is neutralized");
    assert_eq!(count_ops(function, Op::BufferGetResolved), 1);
    assert!(validate_function(function).is_ok());
}

/// A loop with two back edges (a `continue` path and the normal latch) has one preheader, and
/// the accesses on both paths share the hoisted descriptor.
#[test]
fn rewrites_a_loop_with_two_back_edges() {
    let mut function = Function::new("continue".to_string(), IrType::Void, PhpType::Void);
    let entry;
    {
        let mut b = Builder::new(&mut function);
        entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let skip = b.create_named_block("skip", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, int_buffer_type());

        b.position_at_end(entry);
        let handle = load_buffer(&mut b, slot, int_buffer_type());
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        let value = emit_int_get(&mut b, handle, index);
        branch(&mut b, skip, vec![], latch, vec![]);

        // `continue`: a second back edge straight to the header.
        b.position_at_end(skip);
        emit_set(&mut b, handle, index, value);
        back_edge(&mut b, header, index);

        b.position_at_end(latch);
        emit_set(&mut b, handle, index, value);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    assert_eq!(count_ops(function, Op::BufferResolve), 1, "one descriptor for the whole loop");
    assert_eq!(count_ops(function, Op::BufferGetResolved), 1);
    assert_eq!(count_ops(function, Op::BufferSetResolved), 2, "both back-edge paths rewritten");
    let in_preheader = block_instructions(function, entry)
        .into_iter()
        .any(|inst| function.instructions[inst.as_raw() as usize].op == Op::BufferResolve);
    assert!(in_preheader, "the descriptor sits in the preheader, not in the header");
    assert!(validate_function(function).is_ok());
}

/// A handle carried by a header parameter can name a different buffer on every iteration, so
/// its accesses keep resolving per access.
#[test]
fn keeps_a_handle_that_changes_between_iterations() {
    let mut function = Function::new("rotating".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block(
            "header",
            vec![(IrType::I64, PhpType::Int), (IrType::Heap(IrHeapKind::Buffer), int_buffer_type())],
        );
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let first = add_buffer_local(&mut b, int_buffer_type());
        let second = b.add_local(
            Some("c".to_string()),
            IrType::Heap(IrHeapKind::Buffer),
            int_buffer_type(),
            LocalKind::PhpLocal,
        );

        b.position_at_end(entry);
        let a = load_buffer(&mut b, first, int_buffer_type());
        let c = load_buffer(&mut b, second, int_buffer_type());
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero, a] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        let handle = b.block_param(header, 1);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        emit_int_get(&mut b, handle, index);
        let one = b.emit_const_i64(1);
        let next = b.emit_iadd(index, one);
        // The next iteration reads the other buffer.
        b.terminate(Terminator::Br { target: header, args: vec![next, c] });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    assert!(validate_function(&function).is_ok());
    let mut module = module_with(function);

    assert!(!hoist_buffer_resolution(&mut module), "a varying handle is not hoisted");
    let function = &module.functions[0];
    assert_eq!(count_ops(function, Op::BufferGet), 1);
    assert_eq!(count_ops(function, Op::BufferResolve), 0);
}

/// Builds a packed loop whose body reads `$b[$i]`, branches, and reads `$b[$j]` again after the
/// join, where `$j` is the join parameter fed by both arms. With `vary_else` the else arm
/// passes `$i + 1` instead of `$i`. Returns the function, both record values and the join block.
fn packed_join_loop(vary_else: bool) -> (Function, ValueId, ValueId, BlockId) {
    let mut function = Function::new("join".to_string(), IrType::Void, PhpType::Void);
    let (first, second, join);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let then_block = b.create_named_block("then", vec![]);
        let else_block = b.create_named_block("else", vec![]);
        join = b.create_named_block("join", vec![(IrType::I64, PhpType::Int)]);
        let latch = b.create_named_block(
            "latch",
            vec![(IrType::Heap(IrHeapKind::Object), PhpType::Packed("Vec2".to_string()))],
        );
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, packed_buffer_type());

        b.position_at_end(entry);
        let handle = load_buffer(&mut b, slot, packed_buffer_type());
        let zero = b.emit_const_i64(0);
        b.terminate(Terminator::Br { target: header, args: vec![zero] });

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        first = emit_record_get(&mut b, handle, index);
        branch(&mut b, then_block, vec![], else_block, vec![]);

        b.position_at_end(then_block);
        b.terminate(Terminator::Br { target: join, args: vec![index] });

        b.position_at_end(else_block);
        let else_index = if vary_else {
            let one = b.emit_const_i64(1);
            b.emit_iadd(index, one)
        } else {
            index
        };
        b.terminate(Terminator::Br { target: join, args: vec![else_index] });

        b.position_at_end(join);
        let joined = b.block_param(join, 0);
        second = emit_record_get(&mut b, handle, joined);
        b.terminate(Terminator::Br { target: latch, args: vec![second] });

        b.position_at_end(latch);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    (function, first, second, join)
}

/// Two packed `$b[$i]` reads on either side of an `if` share one record address when the join
/// parameter receives the same index on every edge: the later read is dropped and its uses
/// (here the latch argument) now name the dominating read.
#[test]
fn shares_a_record_address_across_a_join() {
    let (function, first, second, join) = packed_join_loop(false);
    assert!(validate_function(&function).is_ok());
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    assert_eq!(defining_op(function, first), Op::BufferGetResolved, "the dominating read stays");
    assert_eq!(defining_op(function, second), Op::Nop, "the dominated read is dropped");
    match &function.block(join).expect("join exists").terminator {
        Some(Terminator::Br { args, .. }) => {
            assert_eq!(args, &vec![first], "the latch now receives the first record address")
        }
        other => panic!("unexpected join terminator {other:?}"),
    }
    assert!(validate_function(function).is_ok());
}

/// When one arm passes a different index into the join, the two reads address different
/// records and both stay.
#[test]
fn keeps_both_record_addresses_when_the_join_index_differs() {
    let (function, first, second, _) = packed_join_loop(true);
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    let function = &module.functions[0];
    assert_eq!(defining_op(function, first), Op::BufferGetResolved);
    assert_eq!(defining_op(function, second), Op::BufferGetResolved, "a different record");
    assert!(validate_function(function).is_ok());
}

/// Two scalar reads of the same element are never merged: a write between them can change
/// the value, so only packed record addresses are shared.
#[test]
fn keeps_repeated_scalar_element_reads() {
    let (function, _, _) = simple_loop(int_buffer_type(), |b, handle, index| {
        let value = emit_int_get(b, handle, index);
        emit_set(b, handle, index, value);
        emit_int_get(b, handle, index);
    });
    let mut module = module_with(function);

    assert!(hoist_buffer_resolution(&mut module));
    assert_eq!(count_ops(&module.functions[0], Op::BufferGetResolved), 2);
}

/// A loop that releases a refcounted value can run a destructor that frees the buffer through
/// an alias, so it keeps per-access resolution.
#[test]
fn keeps_a_loop_that_releases_a_value() {
    let (function, _, _) = simple_loop(int_buffer_type(), |b, handle, index| {
        emit_int_get(b, handle, index);
        let name = b.add_local(Some("s".to_string()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
        let text = b.emit_load_local(name, IrType::Str, PhpType::Str);
        b.emit(Op::Release, vec![text], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
    });
    assert!(validate_function(&function).is_ok());
    let mut module = module_with(function);

    assert!(!hoist_buffer_resolution(&mut module), "a release disqualifies the loop");
    assert_eq!(count_ops(&module.functions[0], Op::BufferGet), 1);
}

/// A header entered from a block that also branches elsewhere has no preheader to hold the
/// descriptor, so the loop is skipped.
#[test]
fn skips_a_loop_without_a_preheader() {
    let mut function = Function::new("no_preheader".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block("body", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        let slot = add_buffer_local(&mut b, int_buffer_type());

        b.position_at_end(entry);
        let handle = load_buffer(&mut b, slot, int_buffer_type());
        let zero = b.emit_const_i64(0);
        branch(&mut b, header, vec![zero], exit, vec![]);

        b.position_at_end(header);
        let index = b.block_param(header, 0);
        branch(&mut b, body, vec![], exit, vec![]);

        b.position_at_end(body);
        emit_int_get(&mut b, handle, index);
        back_edge(&mut b, header, index);

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }
    let mut module = module_with(function);

    assert!(!hoist_buffer_resolution(&mut module));
    assert_eq!(count_ops(&module.functions[0], Op::BufferResolve), 0);
}

/// A generator lets its caller run between iterations, so its loops are never rewritten.
#[test]
fn skips_generators() {
    let (mut function, _, _) = simple_loop(int_buffer_type(), |b, handle, index| {
        emit_int_get(b, handle, index);
    });
    function.flags.is_generator = true;
    let mut module = module_with(function);

    assert!(!hoist_buffer_resolution(&mut module));
    assert_eq!(count_ops(&module.functions[0], Op::BufferGet), 1);
}

/// `pcntl_async_signals()` anywhere in the module dispatches signal handlers between
/// instructions, so no function of that module is rewritten.
#[test]
fn skips_modules_that_enable_async_signals() {
    let (function, _, _) = simple_loop(int_buffer_type(), |b, handle, index| {
        emit_int_get(b, handle, index);
    });
    let mut signals = Function::new("enable_signals".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut signals);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        b.emit(
            Op::RuntimeCall,
            Vec::new(),
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Pcntl(PcntlRuntime::AsyncSignals))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.terminate(Terminator::Return { value: None });
    }
    let mut module = module_with(function);
    module.functions.push(signals);

    assert!(!hoist_buffer_resolution(&mut module));
    assert_eq!(count_ops(&module.functions[0], Op::BufferGet), 1);
}
