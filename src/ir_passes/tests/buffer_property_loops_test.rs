//! Purpose:
//! Pins guarded buffer-property loop hoisting and its conservative refusal boundaries.
//!
//! Called from:
//! - The EIR pass library test harness.
//!
//! Key details:
//! - Hand-built methods retain throwing property reads and raw buffer acquire/release pairs.

use std::collections::HashMap;

use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{
    validate_function, BlockId, Builder, CmpPredicate, DataId, Effects, Function, FunctionParam,
    Immediate, InstId, IrHeapKind, IrType, LocalKind, Module, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, Terminator, Value, ValueDef, ValueId,
};
use crate::ir_passes::buffer_access::hoist_buffer_resolution;
use crate::ir_passes::{compute_dominance, compute_loops};
use crate::ir_passes::rewrite::replace_all_uses;
use crate::types::PhpType;

/// An independently varied reason to retain the original property read.
#[derive(Clone, Copy)]
enum Shape { Body, Header, Call, Free, PropertyWrite, Conditional, LiveOut, OtherReceiver }

/// Emits the refined, potentially throwing read of a `buffer<int>` property.
fn property(builder: &mut Builder<'_>, receiver: ValueId) -> ValueId {
    builder.emit_with_effects(
        Op::PropGet, vec![receiver], Some(Immediate::Data(DataId::from_raw(0))),
        IrType::Heap(IrHeapKind::Buffer), PhpType::Buffer(Box::new(PhpType::Int)),
        Ownership::MaybeOwned, Effects::READS_HEAP | Effects::MAY_THROW, None,
    ).unwrap()
}

/// Constructs one canonical method loop and returns the original read and block ids.
fn fixture(shape: Shape) -> (Function, BlockId, BlockId, InstId) {
    let live_out = matches!(shape, Shape::LiveOut);
    let mut function = Function::new("Slots::fill".into(),
        if live_out { IrType::I64 } else { IrType::Void },
        if live_out { PhpType::Int } else { PhpType::Void });
    function.flags.is_method = true;
    function.params.push(FunctionParam {
        name: "this".into(), ir_type: IrType::Heap(IrHeapKind::Object),
        php_type: PhpType::Object("Slots".into()), by_ref: false, variadic: false,
    });
    let (entry, header, handle);
    {
        let mut b = Builder::new(&mut function);
        let this = b.add_local(Some("this".into()), IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Slots".into()), LocalKind::PhpLocal);
        let index_slot = b.add_local(Some("index".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
        let other = b.add_local(Some("other".into()), IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Slots".into()), LocalKind::PhpLocal);
        entry = b.create_named_block("entry", vec![]);
        header = b.create_named_block("header", vec![]);
        let body = b.create_named_block("body", vec![]);
        let read_body = b.create_named_block("read_body", vec![]);
        let latch = b.create_named_block("latch", vec![]);
        let exit = b.create_named_block("exit", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let initial = b.emit_const_i64(0);
        b.emit_store_local(index_slot, initial);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(header);
        let index = b.emit_load_local(index_slot, IrType::I64, PhpType::Int);
        let limit = b.emit_const_i64(4);
        let receiver_slot = if matches!(shape, Shape::OtherReceiver) { other } else { this };
        let header_handle = if matches!(shape, Shape::Header) {
            let receiver = b.emit_load_local(receiver_slot, IrType::Heap(IrHeapKind::Object), PhpType::Object("Slots".into()));
            Some(property(&mut b, receiver))
        } else { None };
        let condition = b.emit(Op::ICmp, vec![index, limit], Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
            IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
        b.terminate(Terminator::CondBr {
            cond: condition, then_target: body, then_args: vec![], else_target: exit, else_args: vec![],
        });
        b.position_at_end(body);
        if matches!(shape, Shape::Conditional) {
            let condition = b.emit_const_bool(true);
            b.terminate(Terminator::CondBr {
                cond: condition, then_target: read_body, then_args: vec![], else_target: latch, else_args: vec![],
            });
        } else { b.terminate(Terminator::Br { target: read_body, args: vec![] }); }
        // The ordinary body puts the first read directly in the header's entered successor.
        b.position_at_end(read_body);
        handle = header_handle.unwrap_or_else(|| {
            let receiver = b.emit_load_local(receiver_slot, IrType::Heap(IrHeapKind::Object), PhpType::Object("Slots".into()));
            property(&mut b, receiver)
        });
        let acquired = b.emit(Op::Acquire, vec![handle], None, IrType::Heap(IrHeapKind::Buffer),
            PhpType::Buffer(Box::new(PhpType::Int)), Ownership::Owned).unwrap();
        let value = b.emit_const_i64(7);
        b.emit(Op::BufferSet, vec![acquired, index, value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        match shape {
            Shape::Call => { b.emit(Op::Call, vec![], Some(Immediate::Data(DataId::from_raw(1))),
                IrType::Void, PhpType::Void, Ownership::NonHeap); }
            Shape::Free => { b.emit(Op::RuntimeCall, vec![handle], Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::BufferFree))), IrType::Void, PhpType::Void, Ownership::NonHeap); }
            Shape::PropertyWrite => {
                let receiver = b.emit_load_local(this, IrType::Heap(IrHeapKind::Object), PhpType::Object("Slots".into()));
                b.emit(Op::PropSet, vec![receiver, handle], Some(Immediate::Data(DataId::from_raw(0))),
                    IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            _ => {}
        }
        b.emit(Op::Release, vec![acquired], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        b.terminate(Terminator::Br { target: latch, args: vec![] });
        b.position_at_end(latch);
        let one = b.emit_const_i64(1);
        let next = b.emit_iadd(index, one);
        b.emit_store_local(index_slot, next);
        b.terminate(Terminator::Br { target: header, args: vec![] });
        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: live_out.then_some(index) });
    }
    // Collapse the empty unconditional body so ordinary fixtures have a direct body entry.
    if !matches!(shape, Shape::Conditional) {
        function.blocks[2].terminator = Some(Terminator::Unreachable);
        let read_body = function.blocks[3].id;
        if let Some(Terminator::CondBr { then_target, .. }) = &mut function.blocks[header.as_raw() as usize].terminator {
            *then_target = read_body;
        }
    }
    let ValueDef::Instruction { inst: read, .. } = function.values[handle.as_raw() as usize].def
        else { unreachable!("property handle has an instruction"); };
    (function, entry, header, read)
}

/// Runs the module pass and validates its SSA and edge contracts before and after rewriting.
fn optimize(function: Function) -> (bool, Function) {
    validate_function(&function).expect("input loop is valid");
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    module.class_methods.push(function);
    let changed = hoist_buffer_resolution(&mut module);
    let function = module.class_methods.remove(0);
    validate_function(&function).expect("optimized loop is valid");
    (changed, function)
}

/// Body-only reads gain a taken-only preheader containing both the property and resolver.
#[test]
fn buffer_property_loop_body_uses_guarded_resolution() {
    let (function, entry, header, read) = fixture(Shape::Body);
    let (changed, function) = optimize(function);
    assert!(changed);
    assert!(matches!(function.blocks[entry.as_raw() as usize].terminator, Some(Terminator::CondBr { .. })));
    let dominance = compute_dominance(&function);
    let loops = compute_loops(&function, &dominance);
    let loop_ref = loops.loops().iter().find(|loop_ref| loop_ref.header == header).unwrap();
    let preheader = loop_ref.preheader.unwrap();
    assert!(function.blocks[preheader.as_raw() as usize].instructions.contains(&read));
    assert!(function.blocks[preheader.as_raw() as usize].instructions.iter().any(|id| {
        function.instructions[id.as_raw() as usize].op == Op::BufferResolve
    }));
    assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::BufferSetResolved).count(), 1);
}

/// Header reads already execute on zero-trip loops and need no new entry guard.
#[test]
fn buffer_property_loop_header_read_keeps_unconditional_entry() {
    let (function, entry, _, read) = fixture(Shape::Header);
    let (changed, function) = optimize(function);
    assert!(changed);
    assert!(matches!(function.blocks[entry.as_raw() as usize].terminator, Some(Terminator::Br { .. })));
    assert!(function.blocks[entry.as_raw() as usize].instructions.contains(&read));
}

/// Calls, frees, stores, conditional access and live-out SSA refuse motion.
#[test]
fn buffer_property_loop_unsafe_shapes_remain_in_place() {
    for shape in [Shape::Call, Shape::Free, Shape::PropertyWrite, Shape::Conditional, Shape::LiveOut] {
        let (function, _, header, read) = fixture(shape);
        let (_, function) = optimize(function);
        let ValueDef::Instruction { block, .. } = function.values[function.instructions[read.as_raw() as usize].result.unwrap().as_raw() as usize].def
            else { unreachable!("property remains an instruction"); };
        let dominance = compute_dominance(&function);
        let loops = compute_loops(&function, &dominance);
        assert!(loops.loops().iter().find(|loop_ref| loop_ref.header == header).unwrap().contains(block));
    }
}

/// Stable foreign receivers retain the first checked access before returning to the header.
#[test]
fn buffer_object_loop_foreign_receiver_keeps_entry_access() {
    for body_parameter in [false, true] {
        let (mut function, _, header, read) = fixture(Shape::OtherReceiver);
        if body_parameter {
            let ValueDef::Instruction { block: body, .. } = function.values[function.instructions[read.as_raw() as usize].result.unwrap().as_raw() as usize].def
                else { unreachable!("property is in the body"); };
            let access = function.blocks[body.as_raw() as usize].instructions.iter().copied()
                .find(|id| function.instructions[id.as_raw() as usize].op == Op::BufferSet).unwrap();
            let initial = function.instructions[access.as_raw() as usize].operands[1];
            let param = ValueId::from_raw(function.values.len() as u32);
            function.values.push(Value {
                def: ValueDef::BlockParam { block: body, index: 0 },
                ..function.values[initial.as_raw() as usize].clone()
            });
            function.blocks[body.as_raw() as usize].params.push(param);
            function.instructions[access.as_raw() as usize].operands[1] = param;
            if let Some(Terminator::CondBr { then_args, .. }) = &mut function.blocks[header.as_raw() as usize].terminator {
                *then_args = vec![initial];
            }
        }
        let (changed, function) = optimize(function);
        assert!(changed);
        let dominance = compute_dominance(&function);
        let loops = compute_loops(&function, &dominance);
        let loop_ref = loops.loops().iter().find(|loop_ref| loop_ref.header == header).unwrap();
        let entry = function.block(loop_ref.preheader.unwrap()).unwrap();
        assert!(entry.instructions.contains(&read));
        assert!(entry.instructions.iter().any(|id| function.instructions[id.as_raw() as usize].op == Op::BufferSet));
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::BufferSetResolved).count(), 1);
    }
}

/// Reference-cell receivers cannot be reloaded as plain object words in a preheader.
#[test]
fn buffer_object_loop_reference_receiver_remains_in_place() {
    let (mut function, _, header, read) = fixture(Shape::OtherReceiver);
    function.locals[2].kind = LocalKind::RefCell;
    let (_, function) = optimize(function);
    let ValueDef::Instruction { block, .. } = function.values[function.instructions[read.as_raw() as usize].result.unwrap().as_raw() as usize].def
        else { unreachable!("property remains an instruction"); };
    let dominance = compute_dominance(&function);
    let loops = compute_loops(&function, &dominance);
    assert!(loops.loops().iter().find(|loop_ref| loop_ref.header == header).unwrap().contains(block));
}

/// Entry and zero-trip exit arguments substitute the initial value for the header parameter.
#[test]
fn buffer_property_loop_guard_remaps_nonempty_edge_arguments() {
    for reversed in [false, true] {
        let (mut function, entry, header, _) = fixture(Shape::Body);
        let exit = function.blocks[5].id;
        let old_index = function.instructions[function.blocks[header.as_raw() as usize].instructions[0].as_raw() as usize].result.unwrap();
        let initial = function.instructions[function.blocks[entry.as_raw() as usize].instructions[0].as_raw() as usize].result.unwrap();
        let index = ValueId::from_raw(function.values.len() as u32);
        function.values.push(Value {
            ir_type: IrType::I64, php_type: PhpType::Int, ownership: Ownership::NonHeap,
            def: ValueDef::BlockParam { block: header, index: 0 },
        });
        function.blocks[header.as_raw() as usize].params.push(index);
        replace_all_uses(&mut function, &HashMap::from([(old_index, index)]));
        let result = ValueId::from_raw(function.values.len() as u32);
        function.values.push(Value {
            ir_type: IrType::I64, php_type: PhpType::Int, ownership: Ownership::NonHeap,
            def: ValueDef::BlockParam { block: exit, index: 0 },
        });
        function.blocks[exit.as_raw() as usize].params.push(result);
        function.return_type = IrType::I64;
        function.return_php_type = PhpType::Int;
        function.blocks[exit.as_raw() as usize].terminator = Some(Terminator::Return { value: Some(result) });
        if let Some(Terminator::Br { args, .. }) = &mut function.blocks[entry.as_raw() as usize].terminator {
            *args = vec![initial];
        }
        let next = function.blocks[4].instructions.iter().find_map(|id| {
            let inst = &function.instructions[id.as_raw() as usize];
            (inst.op == Op::IAdd).then_some(inst.result).flatten()
        }).unwrap();
        if let Some(Terminator::Br { args, .. }) = &mut function.blocks[4].terminator {
            *args = vec![next];
        }
        if let Some(Terminator::CondBr { then_target, then_args, else_target, else_args, .. }) =
            &mut function.blocks[header.as_raw() as usize].terminator
        {
            *else_args = vec![index];
            if reversed {
                std::mem::swap(then_target, else_target);
                std::mem::swap(then_args, else_args);
            }
        }
        if reversed {
            let compare = function.blocks[header.as_raw() as usize].instructions.iter().copied()
                .find(|id| function.instructions[id.as_raw() as usize].op == Op::ICmp).unwrap();
            function.instructions[compare.as_raw() as usize].immediate = Some(Immediate::CmpPredicate(CmpPredicate::Sge));
        }
        let (changed, function) = optimize(function);
        assert!(changed);
        let Some(Terminator::CondBr { then_target, then_args, else_target, else_args, .. }) =
            &function.blocks[entry.as_raw() as usize].terminator else { panic!("entry guard"); };
        let exit_args = if *then_target == exit { then_args } else {
            assert_eq!(*else_target, exit);
            else_args
        };
        assert_eq!(exit_args, &vec![initial]);
    }
}
